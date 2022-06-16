use once_cell::sync::OnceCell;
use std::boxed::Box;
use std::ffi::OsString;
use std::sync::Mutex;
use std::time::Duration;
use windows_service::define_windows_service;
use windows_service::service_dispatcher;

type Result<T> = windows_service::Result<T>;
type ServiceStatusHandle = windows_service::service_control_handler::ServiceStatusHandle;
type ServiceControlHandlerResult =
    windows_service::service_control_handler::ServiceControlHandlerResult;
type ServiceControl = windows_service::service::ServiceControl;
type ServiceControlAccept = windows_service::service::ServiceControlAccept;
type ServiceState = windows_service::service::ServiceState;
type ServiceStatus = windows_service::service::ServiceStatus;
type ServiceType = windows_service::service::ServiceType;
type ServiceExitCode = windows_service::service::ServiceExitCode;

define_windows_service!(ffi_service_main, service_main);

struct ServiceEntry {
    name: String,
    runner: Box<dyn FnMut() + Send>,
}

/// Stores the global information needed to run the service.
/// Right now this is only a singleton, but could be extended to hold multiple entries in the future.
static SERVICE_TABLE: OnceCell<Mutex<ServiceEntry>> = OnceCell::new();

fn service_main(_args: Vec<OsString>) {
    (SERVICE_TABLE.get().unwrap().lock().unwrap().runner)();
}

pub fn register_service(
    name: String,
    runner: Box<dyn FnMut() + Send>,
) -> std::result::Result<(), String> {
    SERVICE_TABLE
        .set(Mutex::new(ServiceEntry { name, runner }))
        .map_err(|_err| "Failed to register service".to_string())?;
    Ok(())
}

pub fn start_dispatch() -> std::result::Result<(), String> {
    let data = SERVICE_TABLE
        .get()
        .ok_or("No service registered yet".to_string())?
        .lock()
        .map_err(|_err| "Failed to lock service entry".to_string())?;
    let name = data.name.clone();
    service_dispatcher::start(name, ffi_service_main)
        .map_err(|_err| "Failed to start service dispatch".to_string())
}

pub struct ServiceControlHandler {
    rx: crossbeam_channel::Receiver<()>,
    handle: ServiceStatusHandle,
}

impl ServiceControlHandler {
    pub fn new(name: &str) -> Result<Self> {
        let (tx, rx) = crossbeam_channel::bounded(0);
        let status_handle = windows_service::service_control_handler::register(name, move |sc| {
            Self::handle(&tx, sc)
        })?;
        Ok(ServiceControlHandler {
            rx,
            handle: status_handle,
        })
    }

    pub fn chan(&self) -> &crossbeam_channel::Receiver<()> {
        &self.rx
    }

    pub fn update(
        &self,
        status: ServiceState,
        controls_accepted: ServiceControlAccept,
    ) -> Result<()> {
        self.handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: status,
            controls_accepted: controls_accepted,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
    }

    fn handle(
        tx: &crossbeam_channel::Sender<()>,
        sc: ServiceControl,
    ) -> ServiceControlHandlerResult {
        match sc {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                tx.send(()).unwrap();
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    }
}
