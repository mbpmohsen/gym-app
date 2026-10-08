//! Logging: daily-rotated files in <data>/logs (14 days kept), plus the console
//! when there is one. A Windows service has no console, so the files are the only
//! place to look when a gym says "it doesn't work".

use std::path::Path;

use anyhow::Result;
use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{
    filter::LevelFilter,
    fmt::{self, time::ChronoLocal},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

const KEEP_DAYS: usize = 14;

/// Keep the returned guard alive for the life of the process (it flushes on drop).
pub fn init(logs_dir: &Path, console: bool) -> Result<WorkerGuard> {
    std::fs::create_dir_all(logs_dir)?;
    let appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix("face-service")
        .filename_suffix("log")
        .max_log_files(KEEP_DAYS)
        .build(logs_dir)?;
    let (file_writer, guard) = tracing_appender::non_blocking(appender);

    // local time: logs are read by people at the gym, not in UTC
    let time = || ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".into());
    let file_layer = fmt::layer().with_writer(file_writer).with_ansi(false).with_target(false).with_timer(time());
    let console_layer = console.then(|| fmt::layer().with_target(false).with_timer(time()));
    tracing_subscriber::registry()
        .with(LevelFilter::INFO) // hides axum/hyper internals
        .with(file_layer)
        .with(console_layer)
        .try_init()?;
    Ok(guard)
}

/// Console only, for the dev CLIs.
pub fn init_console() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .with_target(false)
        .with_timer(ChronoLocal::new("%H:%M:%S".into()))
        .try_init();
}
