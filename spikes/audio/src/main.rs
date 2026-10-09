//! audio-spike: can a browser tab keep playing sounds pushed over SSE for hours,
//! minimized, after the PC sleeps?
//!
//! usage: audio-spike [interval_secs=60]
//! open http://127.0.0.1:7470 , click "Start", then leave it (minimize, lock, sleep...).
//! Every interval the server sends a sound event; the page plays it and reports back.
//! Results go to audio-spike.log: one line per event, "PLAYED" / "FAILED <reason>",
//! or "MISSING" when the page never reported back within 30s.

use std::collections::HashMap;
use std::convert::Infallible;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse,
    },
    routing::{get, post},
    Router,
};
use futures_util::{stream, Stream};
use serde::Deserialize;
use tokio::sync::broadcast;

const SOUNDS: [&str; 4] = ["welcome", "end-of-tuition", "wrong-shift", "goodbye"];

macro_rules! voice {
    ($g:literal, $n:literal) => {
        include_bytes!(concat!("../../../assets/voices/", $g, "/", $n, ".mp3")) as &[u8]
    };
}

fn voice(gender: &str, name: &str) -> Option<&'static [u8]> {
    Some(match (gender, name) {
        ("male", "welcome") => voice!("male", "welcome"),
        ("male", "goodbye") => voice!("male", "goodbye"),
        ("male", "end-of-tuition") => voice!("male", "end-of-tuition"),
        ("male", "wrong-shift") => voice!("male", "wrong-shift"),
        ("female", "welcome") => voice!("female", "welcome"),
        ("female", "goodbye") => voice!("female", "goodbye"),
        ("female", "end-of-tuition") => voice!("female", "end-of-tuition"),
        ("female", "wrong-shift") => voice!("female", "wrong-shift"),
        _ => return None,
    })
}

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<(u64, &'static str)>,
    /// event id -> sent time, until the page reports back
    pending: Arc<Mutex<HashMap<u64, String>>>,
    log: Arc<Mutex<std::fs::File>>,
}

fn now() -> String {
    // local-ish wall clock without extra deps: seconds since epoch + HH:MM:SS UTC
    let s = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let tehran = s + 3 * 3600 + 1800;
    format!("{:02}:{:02}:{:02}", (tehran / 3600) % 24, (tehran / 60) % 60, tehran % 60)
}

impl AppState {
    fn log(&self, line: &str) {
        let line = format!("[{}] {line}", now());
        println!("{line}");
        let _ = writeln!(self.log.lock().unwrap(), "{line}");
    }
}

#[tokio::main]
async fn main() {
    let interval: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(60);
    let log = std::fs::OpenOptions::new().create(true).append(true).open("audio-spike.log").unwrap();
    let state = AppState {
        tx: broadcast::channel(64).0,
        pending: Arc::new(Mutex::new(HashMap::new())),
        log: Arc::new(Mutex::new(log)),
    };
    state.log(&format!("--- spike started, one sound every {interval}s ---"));

    // ticker: send a sound, then check 30s later whether the page reported back
    let s = state.clone();
    tokio::spawn(async move {
        let mut id = 0u64;
        let mut tick = tokio::time::interval(Duration::from_secs(interval));
        loop {
            tick.tick().await;
            id += 1;
            let sound = SOUNDS[(id as usize - 1) % SOUNDS.len()];
            let receivers = s.tx.receiver_count();
            s.pending.lock().unwrap().insert(id, now());
            let _ = s.tx.send((id, sound));
            if receivers == 0 {
                s.log(&format!("#{id} {sound}: NO PAGE CONNECTED"));
            }
            let s2 = s.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(30)).await;
                if let Some(sent) = s2.pending.lock().unwrap().remove(&id) {
                    if receivers > 0 {
                        s2.log(&format!("#{id} {sound}: MISSING (sent {sent}, page never reported)"));
                    }
                }
            });
        }
    });

    let app = Router::new()
        .route("/", get(|| async { Html(include_str!("index.html")) }))
        .route("/events", get(events))
        .route("/report", post(report))
        .route("/voice/{gender}/{name}", get(serve_voice))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:7470").await.unwrap();
    println!("open http://127.0.0.1:7470  (log: audio-spike.log)");
    axum::serve(listener, app).await.unwrap();
}

async fn events(State(s): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    s.log("page connected (SSE)");
    let rx = s.tx.subscribe();
    let st = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok((id, sound)) => return Some((Ok(Event::default().id(id.to_string()).data(sound)), rx)),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    });
    Sse::new(st).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct Report {
    id: u64,
    result: String,
    hidden: bool,
}

async fn report(State(s): State<AppState>, Query(r): Query<Report>) -> &'static str {
    let sent = s.pending.lock().unwrap().remove(&r.id);
    let tab = if r.hidden { "tab hidden" } else { "tab visible" };
    match sent {
        Some(sent) => s.log(&format!("#{} {} (sent {sent}, {tab})", r.id, r.result)),
        None => s.log(&format!("#{} {} LATE (after the 30s window, {tab})", r.id, r.result)),
    }
    "ok"
}

async fn serve_voice(Path((gender, name)): Path<(String, String)>) -> impl IntoResponse {
    let name = name.trim_end_matches(".mp3").to_string();
    match voice(&gender, &name) {
        Some(b) => ([(header::CONTENT_TYPE, "audio/mpeg"), (header::CACHE_CONTROL, "max-age=86400")], b).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}
