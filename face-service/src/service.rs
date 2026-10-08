//! Windows service: install / uninstall / start / stop / status, and the entry
//! point the Service Control Manager calls.
//!
//! The service runs as LocalSystem, starts with Windows, and is restarted by
//! Windows 5s after a crash (3 times, counter resets daily).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::Notify;
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept, ServiceErrorControl,
        ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType, ServiceState,
        ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};

pub const NAME: &str = "FaceService";
const DISPLAY_NAME: &str = "Face Recognition Service";
const DESCRIPTION: &str = "Local face recognition for the gym management app (127.0.0.1 only).";

/// Config path handed from `main` to the SCM callback.
static CONFIG: OnceLock<PathBuf> = OnceLock::new();

fn friendly(e: windows_service::Error) -> anyhow::Error {
    if let windows_service::Error::Winapi(io) = &e {
        match io.raw_os_error() {
            Some(5) => return anyhow::anyhow!("access denied: run this command from an Administrator terminal"),
            Some(1060) => return anyhow::anyhow!("service is not installed"),
            Some(1073) => return anyhow::anyhow!("service is already installed"),
            _ => {}
        }
    }
    anyhow::Error::new(e)
}

pub fn install(config: &Path) -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)
        .map_err(friendly)?;
    let exe = std::env::current_exe()?;
    let info = ServiceInfo {
        name: NAME.into(),
        display_name: DISPLAY_NAME.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.clone(),
        launch_arguments: vec![OsString::from("service-run"), OsString::from("--config"), config.as_os_str().to_owned()],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };
    let service = manager
        .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
        .map_err(friendly)?;
    service.set_description(DESCRIPTION).map_err(friendly)?;
    service
        .update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 3600)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![
                ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(5) },
                ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(5) },
                ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(30) },
            ]),
        })
        .map_err(friendly)?;
    // also restart when we exit with an error (not just on a crash)
    service.set_failure_actions_on_non_crash_failures(true).map_err(friendly)?;
    println!("installed {NAME}\n  exe:    {}\n  config: {}", exe.display(), config.display());
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(friendly)?;
    let service = manager
        .open_service(NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE)
        .map_err(friendly)?;
    if service.query_status().map_err(friendly)?.current_state != ServiceState::Stopped {
        let _ = service.stop();
        wait_for(&service, ServiceState::Stopped)?;
    }
    service.delete().map_err(friendly)?;
    println!("uninstalled {NAME}");
    Ok(())
}

pub fn start() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(friendly)?;
    let service = manager.open_service(NAME, ServiceAccess::START | ServiceAccess::QUERY_STATUS).map_err(friendly)?;
    service.start(&[] as &[&str]).map_err(friendly)?;
    wait_for(&service, ServiceState::Running)?;
    println!("{NAME} running");
    Ok(())
}

pub fn stop() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(friendly)?;
    let service = manager.open_service(NAME, ServiceAccess::STOP | ServiceAccess::QUERY_STATUS).map_err(friendly)?;
    service.stop().map_err(friendly)?;
    wait_for(&service, ServiceState::Stopped)?;
    println!("{NAME} stopped");
    Ok(())
}

pub fn status() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(friendly)?;
    let service = manager.open_service(NAME, ServiceAccess::QUERY_STATUS).map_err(friendly)?;
    let st = service.query_status().map_err(friendly)?;
    println!("{NAME}: {:?}{}", st.current_state, st.process_id.map(|p| format!(" (pid {p})")).unwrap_or_default());
    Ok(())
}

fn wait_for(service: &windows_service::service::Service, want: ServiceState) -> Result<()> {
    for _ in 0..60 {
        if service.query_status().map_err(friendly)?.current_state == want {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    anyhow::bail!("timed out waiting for {NAME} to become {want:?}; see data\\logs")
}

// ---- running under the Service Control Manager ----

define_windows_service!(ffi_service_main, service_main);

/// Called by `main` for `service-run`. Blocks until the service stops.
pub fn dispatch(config: PathBuf) -> Result<()> {
    let _ = CONFIG.set(config);
    service_dispatcher::start(NAME, ffi_service_main).context("not started by the Service Control Manager")?;
    Ok(())
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = run_service() {
        // logging may not be up yet: leave a note next to the exe
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::fs::write(exe.with_file_name("face-service-error.txt"), format!("{e:#}"));
        }
    }
}

fn run_service() -> Result<()> {
    let notify = Arc::new(Notify::new());
    let n = notify.clone();
    let handle = service_control_handler::register(NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            n.notify_one();
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;

    let status = |state: ServiceState, code: u32, wait: Duration| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: ServiceExitCode::Win32(code),
        checkpoint: 0,
        wait_hint: wait,
        process_id: None,
    };

    handle.set_service_status(status(ServiceState::Running, 0, Duration::ZERO))?;
    let config = CONFIG.get().cloned().context("config path missing")?;
    let result = crate::server::run(&config, crate::server::Shutdown::Notify(notify), false);
    // a non-zero exit code makes Windows apply the restart policy
    let code = if result.is_ok() { 0 } else { 1 };
    handle.set_service_status(status(ServiceState::Stopped, code, Duration::ZERO))?;
    result
}
