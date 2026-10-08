//! Frame sources. Each runs on its own thread and publishes only the LATEST frame:
//! if inference falls behind, old frames are dropped instead of queueing up, so the
//! system stays real-time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use image::RgbImage;
use nokhwa::{
    native_api_backend,
    pixel_format::RgbFormat,
    query,
    utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution},
    Camera,
};

pub struct Frame {
    pub seq: u64,
    pub image: RgbImage,
    pub at: Instant,
}

struct Slot {
    frame: Option<Arc<Frame>>,
    closed: bool,
}

/// Single-slot mailbox holding the newest frame.
#[derive(Clone)]
pub struct Latest(Arc<(Mutex<Slot>, Condvar)>);

impl Latest {
    fn new() -> Self {
        Self(Arc::new((Mutex::new(Slot { frame: None, closed: false }), Condvar::new())))
    }

    fn put(&self, f: Frame) {
        let (m, cv) = &*self.0;
        m.lock().unwrap().frame = Some(Arc::new(f));
        cv.notify_all();
    }

    fn close(&self) {
        let (m, cv) = &*self.0;
        m.lock().unwrap().closed = true;
        cv.notify_all();
    }

    /// Waits for a frame newer than `after`. Returns None once the source has
    /// closed and nothing newer is left.
    pub fn wait_newer(&self, after: u64) -> Option<Arc<Frame>> {
        loop {
            match self.wait_newer_for(after, Duration::from_millis(500)) {
                Wait::Frame(f) => return Some(f),
                Wait::Closed => return None,
                Wait::Timeout => {}
            }
        }
    }

    /// Like `wait_newer` but gives up after `timeout`, so callers can do other work.
    pub fn wait_newer_for(&self, after: u64, timeout: Duration) -> Wait {
        let (m, cv) = &*self.0;
        let deadline = Instant::now() + timeout;
        let mut slot = m.lock().unwrap();
        loop {
            if let Some(f) = &slot.frame {
                if f.seq > after {
                    return Wait::Frame(f.clone());
                }
            }
            if slot.closed {
                return Wait::Closed;
            }
            let now = Instant::now();
            if now >= deadline {
                return Wait::Timeout;
            }
            slot = cv.wait_timeout(slot, deadline - now).unwrap().0;
        }
    }
}

pub enum Wait {
    Frame(Arc<Frame>),
    Timeout,
    Closed,
}

pub struct Source {
    pub latest: Latest,
    pub description: String,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Source {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// "0", "1", ... => camera index; "auto" => first non-virtual camera;
/// anything else => folder of images replayed at `replay_fps` (for testing).
pub fn open_source(spec: &str, replay_fps: f32) -> Result<Source> {
    match spec {
        "auto" => spawn_camera(None),
        s if s.parse::<u32>().is_ok() => spawn_camera(Some(s.parse()?)),
        dir => spawn_folder(PathBuf::from(dir), replay_fps),
    }
}

/// First camera whose name doesn't look virtual (EShare, OBS, ...).
pub fn pick_camera() -> Result<u32> {
    let backend = native_api_backend().context("no native camera backend on this OS")?;
    let devices = query(backend).context("failed to query cameras")?;
    devices
        .iter()
        .find(|d| !d.human_name().to_lowercase().contains("virtual"))
        .or(devices.first())
        .and_then(|d| d.index().as_index().ok())
        .context("no camera found")
}

/// Opens a camera, trying format requests from best to most permissive.
pub fn open_camera(index: u32, verbose: bool) -> Result<Camera> {
    let vga = Resolution::new(640, 480);
    let attempts: Vec<(&str, RequestedFormatType)> = vec![
        ("640x480 MJPEG", RequestedFormatType::Closest(CameraFormat::new(vga, FrameFormat::MJPEG, 30))),
        ("640x480 YUYV", RequestedFormatType::Closest(CameraFormat::new(vga, FrameFormat::YUYV, 30))),
        ("640x480 NV12", RequestedFormatType::Closest(CameraFormat::new(vga, FrameFormat::NV12, 30))),
        ("highest framerate", RequestedFormatType::AbsoluteHighestFrameRate),
        ("highest resolution", RequestedFormatType::AbsoluteHighestResolution),
        ("no constraint", RequestedFormatType::None),
    ];
    for (label, req) in attempts {
        match Camera::new(CameraIndex::Index(index), RequestedFormat::new::<RgbFormat>(req)) {
            Ok(cam) => {
                if verbose {
                    println!("format request OK: {label}");
                }
                return Ok(cam);
            }
            Err(e) => {
                if verbose {
                    println!("format request failed: {label}: {e}");
                }
            }
        }
    }
    bail!("could not open camera {index} with any format request")
}

fn spawn_camera(index: Option<u32>) -> Result<Source> {
    let index = match index {
        Some(i) => i,
        None => pick_camera()?,
    };
    let latest = Latest::new();
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<String>>();

    // nokhwa's Camera isn't Send, so it is created and lives inside this thread.
    let (l, s) = (latest.clone(), stop.clone());
    let handle = thread::Builder::new().name("capture".into()).spawn(move || {
        let mut cam = match open_camera(index, false).and_then(|mut c| {
            c.open_stream()?;
            Ok(c)
        }) {
            Ok(c) => c,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                l.close();
                return;
            }
        };
        let _ = ready_tx.send(Ok(format!("camera {index}: {} ({})", cam.info().human_name(), cam.camera_format())));

        let mut seq = 0u64;
        let mut consecutive_errors = 0;
        while !s.load(Ordering::Relaxed) {
            match cam.frame().and_then(|f| f.decode_image::<RgbFormat>()) {
                Ok(image) => {
                    consecutive_errors = 0;
                    seq += 1;
                    l.put(Frame { seq, image, at: Instant::now() });
                }
                Err(e) => {
                    consecutive_errors += 1;
                    tracing::warn!("capture error: {e}");
                    if consecutive_errors > 50 {
                        tracing::error!("camera lost, stopping capture");
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
        let _ = cam.stop_stream();
        l.close();
    })?;

    let description = ready_rx.recv().context("capture thread died")??;
    Ok(Source { latest, description, stop, handle: Some(handle) })
}

fn spawn_folder(dir: PathBuf, fps: f32) -> Result<Source> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("not a camera index and not a folder: {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("jpg" | "jpeg" | "png")))
        .collect();
    files.sort();
    if files.is_empty() {
        bail!("no images in {}", dir.display());
    }
    let latest = Latest::new();
    let stop = Arc::new(AtomicBool::new(false));
    let description = format!("replay {} ({} frames @ {fps} fps)", dir.display(), files.len());
    let (l, s) = (latest.clone(), stop.clone());
    let period = Duration::from_secs_f32(1.0 / fps);
    let handle = thread::Builder::new().name("replay".into()).spawn(move || {
        for (i, f) in files.iter().enumerate() {
            if s.load(Ordering::Relaxed) {
                break;
            }
            match image::open(f) {
                Ok(img) => l.put(Frame { seq: i as u64 + 1, image: img.to_rgb8(), at: Instant::now() }),
                Err(e) => tracing::warn!("skip {}: {e}", f.display()),
            }
            thread::sleep(period);
        }
        l.close();
    })?;
    Ok(Source { latest, description, stop, handle: Some(handle) })
}
