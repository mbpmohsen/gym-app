//! gym-server: the gym management app (API + embedded web UI).
//!
//!   gym-server [--config gym-server.toml]
//!
//! Config defaults to ./gym-server.toml if present, else next to the exe;
//! created with defaults on first run.

mod assets;
mod auth;
mod db;
mod domain;
mod members;
mod plans;
mod validate;
mod error;
mod face;
mod live;
mod rules;
mod jalali;
mod reports;
mod settings;
#[cfg(windows)]
mod service;
mod shifts;
mod visits;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::{
    middleware,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// loopback only: the app must not be reachable from the network
    pub bind: String,
    pub data_dir: String,
    pub face_service_url: String,
    pub face_service_token: String,
    /// Alternative to face_service_token: path to face-service.toml to read the token from
    /// (relative to this config file). Handy in development.
    pub face_service_config: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:7470".into(),
            data_dir: "data".into(),
            face_service_url: "http://127.0.0.1:7480".into(),
            face_service_token: String::new(),
            face_service_config: String::new(),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<db::Db>,
    pub sessions: Arc<auth::Sessions>,
    pub config: Arc<Config>,
    pub face: face::FaceClient,
    pub live: live::Live,
    pub snapshots_dir: PathBuf,
}

/// How `serve` is told to stop.
pub enum Shutdown {
    CtrlC,
    /// Windows service stop request
    #[allow(dead_code)]
    Notify(Arc<tokio::sync::Notify>),
}

const USAGE: &str = "usage: gym-server [run|install|uninstall|start|stop|status] [--config <path>]";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().filter(|a| !a.starts_with("--")).map(String::as_str).unwrap_or("run");
    let config_path = match args.iter().position(|a| a == "--config").and_then(|i| args.get(i + 1)) {
        Some(p) => PathBuf::from(p),
        None if Path::new("gym-server.toml").is_file() => PathBuf::from("gym-server.toml"),
        None => std::env::current_exe()?.with_file_name("gym-server.toml"),
    };
    // absolute: a Windows service starts in System32, not in our folder
    let config_path = std::path::absolute(config_path)?;
    match command {
        "run" => serve(&config_path, Shutdown::CtrlC, false),
        #[cfg(windows)]
        "install" => service::install(&config_path),
        #[cfg(windows)]
        "uninstall" => service::uninstall(),
        #[cfg(windows)]
        "start" => service::start(),
        #[cfg(windows)]
        "stop" => service::stop(),
        #[cfg(windows)]
        "status" => service::status(),
        #[cfg(windows)]
        "service-run" => service::dispatch(config_path),
        other => bail!("unknown command {other:?}\n{USAGE}"),
    }
}

/// Runs the server until `shutdown`. `log_file`: also write the log to data/logs
/// (a service has no console).
pub fn serve(config_path: &Path, shutdown: Shutdown, log_file: bool) -> Result<()> {
    let config: Config = if config_path.exists() {
        toml::from_str(&std::fs::read_to_string(config_path)?).with_context(|| format!("invalid {}", config_path.display()))?
    } else {
        let c = Config::default();
        std::fs::write(config_path, toml::to_string_pretty(&c)?)?;
        c
    };
    let data = config_path.parent().unwrap().join(&config.data_dir);
    std::fs::create_dir_all(&data)?;

    let timer = tracing_subscriber::fmt::time::ChronoLocal::new("%Y-%m-%d %H:%M:%S".into());
    if log_file {
        let logs = data.join("logs");
        std::fs::create_dir_all(&logs)?;
        let file = std::fs::OpenOptions::new().create(true).append(true).open(logs.join("gym-server.log"))?;
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_target(false)
            .with_ansi(false)
            .with_timer(timer)
            .with_writer(std::sync::Mutex::new(file))
            .init();
    } else {
        tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).with_target(false).with_timer(timer).init();
    }
    info!("gym-server {} config {}", env!("CARGO_PKG_VERSION"), config_path.display());

    let addr: SocketAddr = config.bind.parse().context("invalid bind address")?;
    if !addr.ip().is_loopback() {
        bail!("bind must be a loopback address (127.0.0.1)");
    }
    let db = db::Db::open(&data.join("gym.db"))?;

    let token = face_token(&config, config_path.parent().unwrap());
    if token.is_empty() {
        tracing::warn!("no face-service token configured (face_service_token / face_service_config): face features will fail");
    }
    let face = face::FaceClient::new(&config.face_service_url, &token);
    let snapshots_dir = data.join("snapshots");
    std::fs::create_dir_all(&snapshots_dir)?;
    let state = AppState { db: Arc::new(db), sessions: Arc::default(), config: Arc::new(config), face, live: live::Live::default(), snapshots_dir };

    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let result = rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("cannot listen on {addr}"))?;
        info!("listening on http://{addr}/");
        face::spawn_reconcile(state.clone());
        live::spawn(state.clone());
        axum::serve(listener, router(state))
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
    if let Err(e) = &result {
        tracing::error!("{e:#}");
    }
    // open SSE streams (browser tabs, face-service) would keep the runtime busy
    rt.shutdown_timeout(std::time::Duration::from_secs(2));
    result
}

fn face_token(config: &Config, dir: &Path) -> String {
    if !config.face_service_token.is_empty() || config.face_service_config.is_empty() {
        return config.face_service_token.clone();
    }
    let path = dir.join(&config.face_service_config);
    let token = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| t.get("token").and_then(|v| v.as_str()).map(str::to_string));
    match token {
        Some(t) => t,
        None => {
            tracing::warn!("could not read token from {}", path.display());
            String::new()
        }
    }
}

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/me", get(|| async { Json(json!({ "ok": true })) }))
        .route("/settings", get(settings::get).put(settings::put))
        .route("/auth/password", post(auth::change_password))
        .route("/plans", get(plans::list).post(plans::create))
        .route("/plans/{id}", put(plans::update))
        .route("/members", get(members::list).post(members::create))
        .route("/members/{id}", get(members::get).put(members::update))
        .route("/members/{id}/archive", post(members::archive))
        .route("/members/{id}/subscriptions", post(members::sell))
        .route("/members/{id}/subscriptions/preview", get(members::preview))
        .route("/subscriptions/{id}/payments", post(members::pay))
        .route("/face/health", get(face::health))
        .route("/face/preview", get(face::preview))
        .route("/face/enroll", get(face::enroll_status).delete(face::enroll_cancel))
        .route("/members/{id}/face", axum::routing::delete(face::delete))
        .route("/members/{id}/face/start", post(face::enroll_start))
        .route("/members/{id}/face/commit", post(face::enroll_commit))
        .route("/shifts", get(shifts::list).post(shifts::create))
        .route("/shifts/current", get(shifts::current))
        .route("/shifts/{id}", put(shifts::update).delete(shifts::delete))
        .route("/reports/{name}", get(reports::get))
        .route("/dashboard", get(reports::dashboard))
        .route("/reception", get(visits::reception))
        .route("/live", get(live::sse))
        .route("/members/{id}/visits", get(visits::member_visits))
        .route("/members/{id}/enter", post(visits::manual_enter))
        .route("/members/{id}/exit", post(visits::manual_exit))
        .route("/visits/{id}/cancel", post(visits::cancel_visit))
        .route("/face-events/{id}/dismiss", post(visits::dismiss_face_event))
        .route("/snapshots/{file}", get(visits::snapshot))
        .layer(middleware::from_fn_with_state(state.clone(), auth::require));

    let api = Router::new()
        .route("/health", get(|| async { Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })) }))
        .route("/auth/state", get(auth::state))
        .route("/auth/setup", post(auth::setup))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .merge(protected)
        .fallback(|| async { error::ApiError::new(axum::http::StatusCode::NOT_FOUND, "not_found", "مسیر نامعتبر") });

    Router::new()
        .nest("/api", api)
        .route("/voices/{gender}/{name}", get(assets::voice))
        .fallback(assets::web)
        .with_state(state)
}
