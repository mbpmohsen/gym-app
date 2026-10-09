//! Live side of reception:
//! - follows face-service's event stream (SSE, resumed with Last-Event-ID) and turns
//!   recognitions into entries/exits (§4.3), unknown/uncertain into cards (§4.6);
//! - pushes what happened to the browser tabs over our own SSE (`/api/live`);
//! - closes forgotten visits every minute (auto-exit).

use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use chrono::{DateTime, Local, Timelike};
use futures_util::{stream, Stream, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{broadcast, Notify};

use crate::db;
use crate::visits::{self, LiveEvent};
use crate::AppState;

const LAST_ID_KEY: &str = "face_last_event_id";
/// face-service must not announce the same person again for this long (SPEC §4.3)
const FACE_COOLDOWN_SECS: f64 = 60.0;
/// events older than this (backlog after we were down) are recorded but not announced
const FRESH: chrono::Duration = chrono::Duration::seconds(60);
/// camera released this long after the last app window closed
const CAMERA_OFF_AFTER: Duration = Duration::from_secs(120);
/// snapshot photos are kept this long
const SNAPSHOT_DAYS: u64 = 90;

#[derive(Clone)]
pub struct Live {
    tx: broadcast::Sender<String>,
    connected: Arc<AtomicBool>,
    /// open app windows (browser tabs on /api/live)
    viewers: Arc<AtomicUsize>,
    /// a window opened: turn the camera on now, don't wait for the next tick
    wake: Arc<Notify>,
}

impl Default for Live {
    fn default() -> Self {
        Self { tx: broadcast::channel(64).0, connected: Arc::default(), viewers: Arc::default(), wake: Arc::default() }
    }
}

/// Counts one open window for as long as its stream lives.
struct Viewer(Arc<AtomicUsize>);

impl Drop for Viewer {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Live {
    pub fn send(&self, ev: &LiveEvent) {
        let _ = self.tx.send(serde_json::to_string(ev).unwrap());
    }

    pub fn viewers(&self) -> usize {
        self.viewers.load(Ordering::Relaxed)
    }

    /// Are we receiving face-service events right now?
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

/// `GET /api/live`: one SSE stream per browser tab. Nothing is replayed: on
/// reconnect the tab simply refetches its lists.
pub async fn sse(State(s): State<AppState>) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = s.live.tx.subscribe();
    s.live.viewers.fetch_add(1, Ordering::Relaxed);
    s.live.wake.notify_one();
    let viewer = Viewer(s.live.viewers.clone());
    let hello = stream::once(async { Ok(SseEvent::default().data(serde_json::to_string(&LiveEvent::refresh()).unwrap())) });
    // the Viewer travels with the stream; dropped when the window closes
    let live = stream::unfold((rx, viewer), |(mut rx, viewer)| async move {
        loop {
            match rx.recv().await {
                Ok(json) => return Some((Ok(SseEvent::default().data(json)), (rx, viewer))),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    return Some((Ok(SseEvent::default().data(serde_json::to_string(&LiveEvent::refresh()).unwrap())), (rx, viewer)))
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(hello.chain(live)).keep_alive(KeepAlive::default())
}

pub fn spawn(s: AppState) {
    tokio::spawn(camera_keeper(s.clone()));
    let follower = s.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = follow(&follower).await {
                tracing::warn!("face events: {e}");
            }
            if follower.live.connected.swap(false, Ordering::Relaxed) {
                follower.live.send(&LiveEvent::refresh());
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    tokio::spawn(async move {
        let mut tick = 0u64;
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            match visits::auto_exit(&s.db.conn(), visits::now()) {
                Ok(n) if n > 0 => s.live.send(&LiveEvent::refresh()),
                Ok(_) => {}
                Err(e) => tracing::warn!("auto-exit: {}", e.message()),
            }
            if tick % (24 * 60) == 0 {
                prune_snapshots(&s.snapshots_dir);
            }
            tick += 1;
        }
    });
}

/// Camera on while an app window is open; released CAMERA_OFF_AFTER after the last
/// one closes (so a reload doesn't flick it). Re-sent periodically because a
/// restarted face-service starts with the camera on.
async fn camera_keeper(s: AppState) {
    let mut last_viewer = Instant::now();
    let mut sent: Option<(bool, Instant)> = None;
    loop {
        let _ = tokio::time::timeout(Duration::from_secs(5), s.live.wake.notified()).await;
        if s.live.viewers.load(Ordering::Relaxed) > 0 {
            last_viewer = Instant::now();
        }
        let want = last_viewer.elapsed() < CAMERA_OFF_AFTER;
        if sent.is_some_and(|(v, at)| v == want && at.elapsed() < Duration::from_secs(30)) {
            continue;
        }
        match s.face.set_camera(want).await {
            Ok(()) => {
                if sent.is_none_or(|(v, _)| v != want) {
                    tracing::info!("camera {}", if want { "on (app window open)" } else { "off (no app window open)" });
                    // windows refetch the camera state once it has had time to open
                    let live = s.live.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        live.send(&LiveEvent::refresh());
                    });
                }
                sent = Some((want, Instant::now()));
            }
            Err(_) => sent = None, // face-service down; retry next tick
        }
    }
}

/// One connection to face-service's event stream, until it ends.
async fn follow(s: &AppState) -> anyhow::Result<()> {
    // our rule needs a short cooldown; face-service's default is 10 minutes
    if let Err(e) = s.face.ensure_cooldown(FACE_COOLDOWN_SECS).await {
        anyhow::bail!("cannot set cooldown: {}", e.message());
    }
    let last = db::get_setting(&s.db.conn(), LAST_ID_KEY)?;
    let res = s.face.events(last.as_deref()).await?;
    s.live.connected.store(true, Ordering::Relaxed);
    s.live.send(&LiveEvent::refresh());
    tracing::info!("following face-service events (after id {})", last.as_deref().unwrap_or("-"));

    let mut body = res.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = body.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?).replace("\r\n", "\n"));
        while let Some(end) = buf.find("\n\n") {
            let frame: String = buf.drain(..end + 2).collect();
            let (mut id, mut data) = (None, String::new());
            for line in frame.lines() {
                if let Some(v) = line.strip_prefix("id:") {
                    id = Some(v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("data:") {
                    data.push_str(v.strip_prefix(' ').unwrap_or(v));
                }
            }
            if data.is_empty() {
                continue; // keep-alive
            }
            match serde_json::from_str::<Value>(&data) {
                Ok(ev) => handle(s, &ev).await,
                Err(e) => tracing::warn!("bad face event: {e}"),
            }
            if let Some(id) = id {
                db::set_setting(&s.db.conn(), LAST_ID_KEY, &id)?;
            }
        }
    }
    anyhow::bail!("stream ended")
}

async fn handle(s: &AppState, ev: &Value) {
    let Some(id) = ev["id"].as_i64() else { return };
    let at = ev["ts"]
        .as_i64()
        .and_then(DateTime::from_timestamp_millis)
        .map(|t| t.with_timezone(&Local).naive_local())
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or_else(visits::now);
    let fresh = visits::now() - at < FRESH;
    let kind = ev["type"].as_str().unwrap_or("");

    let snapshot = save_snapshot(s, id).await;
    let result = match kind {
        "recognized" => {
            let Some(member) = ev["member_id"].as_str().and_then(|m| m.parse::<i64>().ok()) else { return };
            let r = visits::camera(&mut s.db.conn(), member, at, snapshot.as_deref());
            if matches!(r, Ok(None) | Ok(Some(LiveEvent { kind: "exit", .. }))) {
                remove_snapshot(s, snapshot.as_deref()); // only entries keep their photo
            }
            r
        }
        "unknown" | "uncertain" => {
            let r = s.db.conn().execute(
                "INSERT OR IGNORE INTO face_events (id, type, candidates, snapshot, at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, kind, ev.get("candidates").unwrap_or(&json!([])).to_string(), snapshot, visits::fmt(at)],
            );
            match r {
                Ok(_) => Ok(Some(LiveEvent { kind: "face", ..LiveEvent::refresh() })),
                Err(e) => Err(e.into()),
            }
        }
        _ => return,
    };
    match result {
        Ok(Some(mut out)) => {
            if !fresh {
                out.sound = None;
            }
            s.live.send(&out);
        }
        Ok(None) => {}
        Err(e) => tracing::warn!("face event {id} ({kind}): {}", e.message()),
    }
}

/// Copies face-service's snapshot of event `id` (it keeps only the latest ones).
async fn save_snapshot(s: &AppState, id: i64) -> Option<String> {
    let bytes = s.face.snapshot(id).await?;
    let file = format!("{id}.jpg");
    match tokio::fs::write(s.snapshots_dir.join(&file), bytes).await {
        Ok(()) => Some(file),
        Err(e) => {
            tracing::warn!("cannot save snapshot {file}: {e}");
            None
        }
    }
}

fn remove_snapshot(s: &AppState, file: Option<&str>) {
    if let Some(f) = file {
        let _ = std::fs::remove_file(s.snapshots_dir.join(f));
    }
}

fn prune_snapshots(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let max_age = Duration::from_secs(SNAPSHOT_DAYS * 86_400);
    let mut n = 0;
    for e in entries.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > max_age);
        if old && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        tracing::info!("removed {n} snapshot(s) older than {SNAPSHOT_DAYS} days");
    }
}
