//! Runs the whole service (models, worker thread, HTTP API) until `shutdown` fires.
//! Used both from the console (`face-service run`) and as a Windows service.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use tokio::sync::Notify;
use tracing::{error, info};

use crate::api::{self, AppState};
use crate::config::Loaded;
use crate::detector::Detector;
use crate::engine::{Engine, EngineConfig};
use crate::recognizer::Recognizer;
use crate::store::Store;
use crate::worker::{self, Shared};

pub enum Shutdown {
    CtrlC,
    /// fired by the Windows service control handler
    Notify(Arc<Notify>),
}

pub fn default_onnxruntime() -> &'static str {
    if cfg!(windows) {
        "onnxruntime.dll"
    } else {
        "libonnxruntime.so"
    }
}

pub fn run(config_path: &Path, shutdown: Shutdown, console: bool) -> Result<()> {
    let loaded = Loaded::load_or_create(config_path, default_onnxruntime())?;
    let cfg = loaded.config.clone();
    let data = loaded.resolve(&cfg.data_dir);
    let _log_guard = crate::logging::init(&data.join("logs"), console)?;
    info!("face-service {} starting, config {}", env!("CARGO_PKG_VERSION"), loaded.path.display());

    let result = run_inner(loaded, shutdown);
    if let Err(e) = &result {
        error!("fatal: {e:#}");
    }
    result
}

fn run_inner(loaded: Loaded, shutdown: Shutdown) -> Result<()> {
    let cfg = loaded.config.clone();
    let addr: SocketAddr = cfg.bind.parse().context("invalid bind address")?;
    if !addr.ip().is_loopback() {
        bail!("bind must be a loopback address (127.0.0.1); the service must not be exposed to the network");
    }

    let ort = if cfg.onnxruntime.is_empty() { None } else { Some(loaded.resolve(&cfg.onnxruntime)) };
    let ort = crate::init_runtime(ort.as_deref())?;
    info!("onnxruntime: {}", ort.display());

    let models = loaded.resolve(&cfg.models_dir);
    let data = loaded.resolve(&cfg.data_dir);
    let snapshots = data.join("snapshots");
    std::fs::create_dir_all(&snapshots)?;

    let store = Store::open(&data.join("face.db"))?;
    let gallery = store.load_gallery()?;
    info!("gallery: {} member(s), {} sample(s)", gallery.member_count(), gallery.sample_count());

    let det = Detector::new(&models.join("face_detection_yunet_2023mar.onnx"), 2)?;
    let rec = Recognizer::new(&models.join("face_recognition_sface_2021dec.onnx"), 2)?;
    let engine = Engine::new(det, rec, gallery, EngineConfig::default());

    let shared = Arc::new(Shared::new(store, snapshots));
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let worker = {
        let (shared, rec_cfg, camera) = (shared.clone(), cfg.recognition.clone(), cfg.camera.clone());
        std::thread::Builder::new()
            .name("worker".into())
            .spawn(move || worker::run(engine, rec_cfg, camera, shared, cmd_rx))?
    };

    let state = AppState { shared: shared.clone(), cmd: cmd_tx, config: Arc::new(Mutex::new(loaded)), started: Instant::now() };
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let served = rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("cannot listen on {addr}"))?;
        let q = if cfg.token.is_empty() { String::new() } else { format!("?token={}", cfg.token) };
        info!("listening on http://{addr}/{q}");
        axum::serve(listener, api::router(state))
            .with_graceful_shutdown(async move {
                match shutdown {
                    Shutdown::CtrlC => {
                        let _ = tokio::signal::ctrl_c().await;
                    }
                    Shutdown::Notify(n) => n.notified().await,
                }
                info!("shutting down");
            })
            .await?;
        anyhow::Ok(())
    });

    shared.stop.store(true, Ordering::Relaxed);
    let _ = worker.join();
    info!("stopped");
    served
}
