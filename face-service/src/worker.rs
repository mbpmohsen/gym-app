//! The worker thread: owns the camera and the models, runs recognition or
//! enrollment on each frame (capped at max_fps), and publishes events, the
//! preview stream and status. The HTTP side talks to it through `Command`s.
//! If the camera disappears, it keeps retrying instead of exiting.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, Rgb, RgbImage};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, oneshot, watch};
use tracing::{error, info, warn};

use crate::camera::{self, Wait};
use crate::config::Recognition;
use crate::engine::{Engine, Event, FaceInfo};
use crate::enrollment::{EnrollSession, EnrollState, EnrollStatus};
use crate::gallery::Gallery;
use crate::recognizer::Embedding;
use crate::store::{now_ms, Store};

pub enum Command {
    StartEnroll { member_id: String, target: usize, reply: oneshot::Sender<Result<(), String>> },
    CancelEnroll,
    /// Hands over the samples of a finished (Ready) session for `member_id`.
    TakeEnrollment { member_id: String, reply: oneshot::Sender<Option<Vec<Embedding>>> },
    SetGallery(Gallery),
    SetRecognition(Recognition),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    pub camera: String,
    pub camera_connected: bool,
    pub fps: f32,
    pub avg_ms: f32,
    pub frames_processed: u64,
    pub enroll: Option<EnrollStatus>,
}

pub struct Shared {
    pub store: Store,
    /// (event id, rendered JSON)
    pub events: broadcast::Sender<(i64, String)>,
    pub preview: watch::Sender<Option<Arc<Vec<u8>>>>,
    /// JPEG encoding is skipped while nobody watches the preview.
    pub preview_clients: AtomicUsize,
    pub status: Mutex<Status>,
    pub snapshots_dir: PathBuf,
    pub stop: AtomicBool,
}

impl Shared {
    pub fn new(store: Store, snapshots_dir: PathBuf) -> Self {
        Self {
            store,
            events: broadcast::channel(256).0,
            preview: watch::channel(None).0,
            preview_clients: AtomicUsize::new(0),
            status: Mutex::new(Status::default()),
            snapshots_dir,
            stop: AtomicBool::new(false),
        }
    }
}

/// Adds `id` and `snapshot` to a stored event body.
pub fn render_event(id: i64, body: &str) -> String {
    let mut v: Value = serde_json::from_str(body).unwrap_or(json!({}));
    v["id"] = json!(id);
    v["snapshot"] = json!(format!("/v1/snapshots/{id}.jpg"));
    v.to_string()
}

pub fn run(mut engine: Engine, mut rec_cfg: Recognition, camera_spec: String, shared: Arc<Shared>, rx: mpsc::Receiver<Command>) {
    let mut enroll: Option<EnrollSession> = None;
    apply_recognition(&mut engine, &rec_cfg);

    while !shared.stop.load(Ordering::Relaxed) {
        let source = match camera::open_source(&camera_spec, 10.0) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("unavailable: {e:#}");
                if shared.status.lock().unwrap().camera != msg {
                    warn!("camera {msg}");
                }
                set_camera(&shared, msg, false);
                // stay responsive to commands while waiting to retry
                for _ in 0..30 {
                    handle_commands(&rx, &mut engine, &mut enroll, &mut rec_cfg, &shared);
                    std::thread::sleep(Duration::from_millis(100));
                }
                continue;
            }
        };
        info!("camera: {}", source.description);
        set_camera(&shared, source.description.clone(), true);

        let mut last_seq = 0;
        let mut window = (Instant::now(), 0u32);
        loop {
            handle_commands(&rx, &mut engine, &mut enroll, &mut rec_cfg, &shared);
            if shared.stop.load(Ordering::Relaxed) {
                return;
            }
            let frame = match source.latest.wait_newer_for(last_seq, Duration::from_millis(200)) {
                Wait::Frame(f) => f,
                Wait::Timeout => continue,
                Wait::Closed => {
                    warn!("camera lost, reconnecting");
                    set_camera(&shared, "disconnected, retrying".into(), false);
                    break;
                }
            };
            last_seq = frame.seq;
            let t = Instant::now();

            let collecting = enroll.as_ref().is_some_and(|s| s.state == EnrollState::Collecting);
            let mut infos = Vec::new();
            if collecting {
                let s = enroll.as_mut().unwrap();
                if let Err(e) = s.feed(&mut engine.det, &mut engine.rec, &frame.image, frame.at) {
                    error!("enroll error: {e:#}");
                }
            } else {
                match engine.process(&frame.image, frame.at) {
                    Ok((events, i)) => {
                        publish(&shared, &frame.image, events, &i);
                        infos = i;
                    }
                    Err(e) => error!("recognition error: {e:#}"),
                }
            }

            if shared.preview_clients.load(Ordering::Relaxed) > 0 {
                let jpg = render_preview(&frame.image, &infos, engine.cfg.threshold);
                shared.preview.send_replace(Some(Arc::new(jpg)));
            }

            // stats
            let busy = t.elapsed();
            window.1 += 1;
            {
                let mut st = shared.status.lock().unwrap();
                st.frames_processed += 1;
                let ms = busy.as_secs_f32() * 1e3;
                st.avg_ms = if st.avg_ms == 0.0 { ms } else { st.avg_ms * 0.95 + ms * 0.05 };
                st.enroll = enroll.as_ref().map(|s| s.status());
                if window.0.elapsed() >= Duration::from_secs(2) {
                    st.fps = window.1 as f32 / window.0.elapsed().as_secs_f32();
                    window = (Instant::now(), 0);
                }
            }

            // CPU budget: don't process more than max_fps
            let period = Duration::from_secs_f32(1.0 / rec_cfg.max_fps);
            if let Some(rest) = period.checked_sub(t.elapsed()) {
                std::thread::sleep(rest);
            }
        }
    }
}

fn apply_recognition(engine: &mut Engine, r: &Recognition) {
    engine.cfg.threshold = r.threshold;
    engine.cfg.cooldown = Duration::from_secs_f32(r.cooldown_secs);
}

fn set_camera(shared: &Shared, desc: String, ok: bool) {
    let mut st = shared.status.lock().unwrap();
    st.camera = desc;
    st.camera_connected = ok;
    if !ok {
        st.fps = 0.0;
    }
}

fn handle_commands(
    rx: &mpsc::Receiver<Command>,
    engine: &mut Engine,
    enroll: &mut Option<EnrollSession>,
    rec_cfg: &mut Recognition,
    shared: &Shared,
) {
    while let Ok(cmd) = rx.try_recv() {
        match cmd {
            Command::StartEnroll { member_id, target, reply } => {
                if enroll.as_ref().is_some_and(|s| s.state == EnrollState::Collecting) {
                    let _ = reply.send(Err("another enrollment is in progress".into()));
                } else {
                    *enroll = Some(EnrollSession::new(&member_id, target));
                    let _ = reply.send(Ok(()));
                }
            }
            Command::CancelEnroll => *enroll = None,
            Command::TakeEnrollment { member_id, reply } => {
                let ready = enroll.as_ref().is_some_and(|s| s.member_id == member_id && s.state == EnrollState::Ready);
                let _ = reply.send(if ready { enroll.take().map(|s| s.kept) } else { None });
            }
            Command::SetGallery(g) => engine.gallery = g,
            Command::SetRecognition(r) => {
                apply_recognition(engine, &r);
                *rec_cfg = r;
            }
        }
        shared.status.lock().unwrap().enroll = enroll.as_ref().map(|s| s.status());
    }
}

fn publish(shared: &Shared, img: &RgbImage, events: Vec<Event>, infos: &[FaceInfo]) {
    for ev in events {
        let (track_id, body) = match &ev {
            Event::Recognized { track_id, member_id, score } => {
                info!("RECOGNIZED {member_id} ({score:.3})");
                (*track_id, json!({"type": "recognized", "member_id": member_id, "score": score, "track_id": track_id}))
            }
            Event::Unknown { track_id } => {
                info!("UNKNOWN (track {track_id})");
                (*track_id, json!({"type": "unknown", "track_id": track_id}))
            }
            Event::Uncertain { track_id, candidates } => {
                info!("UNCERTAIN (track {track_id})");
                (*track_id, json!({"type": "uncertain", "track_id": track_id, "candidates": candidates}))
            }
            // within cooldown: nothing for clients to do
            Event::Suppressed { .. } => continue,
        };
        let ts = now_ms();
        let mut body = body;
        body["ts"] = json!(ts);
        let body = body.to_string();
        let id = match shared.store.insert_event(ts, &body) {
            Ok(id) => id,
            Err(e) => {
                error!("event store error: {e:#}");
                continue;
            }
        };
        if let Some(info) = infos.iter().find(|i| i.track_id == track_id) {
            if let Err(e) = save_snapshot(shared, id, img, &info.face.bbox) {
                error!("snapshot error: {e:#}");
            }
        }
        if id % 100 == 0 {
            prune_snapshots(shared);
        }
        let _ = shared.events.send((id, render_event(id, &body)));
    }
}

/// Face crop with context (2x the box), at most 256px, as JPEG.
fn save_snapshot(shared: &Shared, id: i64, img: &RgbImage, bbox: &[f32; 4]) -> anyhow::Result<()> {
    let [x, y, w, h] = *bbox;
    let side = w.max(h) * 2.0;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let x0 = (cx - side / 2.0).max(0.0) as u32;
    let y0 = (cy - side / 2.0).max(0.0) as u32;
    let x1 = ((cx + side / 2.0) as u32).min(img.width());
    let y1 = ((cy + side / 2.0) as u32).min(img.height());
    if x1 <= x0 || y1 <= y0 {
        return Ok(());
    }
    let mut crop = image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
    if crop.width().max(crop.height()) > 256 {
        let s = 256.0 / crop.width().max(crop.height()) as f32;
        crop = image::imageops::resize(&crop, (crop.width() as f32 * s) as u32, (crop.height() as f32 * s) as u32, FilterType::Triangle);
    }
    std::fs::write(shared.snapshots_dir.join(format!("{id}.jpg")), encode_jpeg(&crop, 80))?;
    Ok(())
}

fn prune_snapshots(shared: &Shared) {
    let Ok(oldest) = shared.store.oldest_event_id() else { return };
    let Ok(dir) = std::fs::read_dir(&shared.snapshots_dir) else { return };
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(id) = name.strip_suffix(".jpg").and_then(|s| s.parse::<i64>().ok()) {
            if id < oldest {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

const GREEN: Rgb<u8> = Rgb([40, 200, 80]);
const RED: Rgb<u8> = Rgb([230, 50, 50]);
const YELLOW: Rgb<u8> = Rgb([250, 200, 0]);

/// Preview frame: boxes colored by state (green match, red no match, yellow
/// rejected by quality). Enrollment progress is shown by the client, not drawn here.
fn render_preview(img: &RgbImage, infos: &[FaceInfo], threshold: f32) -> Vec<u8> {
    let mut out = img.clone();
    for i in infos {
        let color = match (&i.rejected, &i.best) {
            (Some(_), _) => YELLOW,
            (None, Some(c)) if c.score >= threshold => GREEN,
            _ => RED,
        };
        draw_rect(&mut out, &i.face.bbox, color, 2);
    }
    encode_jpeg(&out, 70)
}

fn draw_rect(img: &mut RgbImage, bbox: &[f32; 4], color: Rgb<u8>, thick: i64) {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let x0 = bbox[0] as i64;
    let y0 = bbox[1] as i64;
    let x1 = (bbox[0] + bbox[2]) as i64;
    let y1 = (bbox[1] + bbox[3]) as i64;
    let mut put = |x: i64, y: i64| {
        if x >= 0 && y >= 0 && x < w && y < h {
            img.put_pixel(x as u32, y as u32, color);
        }
    };
    for t in 0..thick {
        for x in x0..=x1 {
            put(x, y0 + t);
            put(x, y1 - t);
        }
        for y in y0..=y1 {
            put(x0 + t, y);
            put(x1 - t, y);
        }
    }
}

fn encode_jpeg(img: &RgbImage, quality: u8) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = JpegEncoder::new_with_quality(&mut buf, quality).encode_image(img);
    buf
}
