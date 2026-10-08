//! face-service: the local face recognition service.
//!
//!   face-service [run]  [--config <path>]   run in this terminal (Ctrl+C to stop)
//!   face-service install [--config <path>]  register as a Windows service (Administrator)
//!   face-service uninstall | start | stop | status
//!
//! Config defaults to ./face-service.toml if it exists (development), else
//! face-service.toml next to the executable (installed); it is created
//! with defaults (and a random token) on first run.

use std::path::PathBuf;

use anyhow::{bail, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().filter(|a| !a.starts_with("--")).map(String::as_str).unwrap_or("run");
    let config = config_path(&args)?;

    match command {
        "run" => face_service::server::run(&config, face_service::server::Shutdown::CtrlC, true),
        #[cfg(windows)]
        "install" => face_service::service::install(&config),
        #[cfg(windows)]
        "uninstall" => face_service::service::uninstall(),
        #[cfg(windows)]
        "start" => face_service::service::start(),
        #[cfg(windows)]
        "stop" => face_service::service::stop(),
        #[cfg(windows)]
        "status" => face_service::service::status(),
        #[cfg(windows)]
        "service-run" => face_service::service::dispatch(config),
        other => bail!(
            "unknown command {other:?}\nusage: face-service [run|install|uninstall|start|stop|status] [--config <path>]"
        ),
    }
}

/// --config <path>, else ./face-service.toml if present, else next to the exe. Always absolute,
/// because a Windows service starts in System32, not in our folder.
fn config_path(args: &[String]) -> Result<PathBuf> {
    let p = match args.iter().position(|a| a == "--config").and_then(|i| args.get(i + 1)) {
        Some(p) => PathBuf::from(p),
        None if PathBuf::from("face-service.toml").is_file() => PathBuf::from("face-service.toml"),
        None => std::env::current_exe()?.with_file_name("face-service.toml"),
    };
    Ok(std::path::absolute(p)?)
}
