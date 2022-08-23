use crate::gensvc;

use anyhow::Result;
use once_cell::sync::OnceCell;
use std::boxed::Box;
use std::ffi::OsString;
use std::sync::Mutex;
use std::time::Duration;
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceErrorControl, ServiceExitCode, ServiceInfo,
    ServiceStartType, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{ServiceControlHandlerResult, ServiceStatusHandle};
use windows_service::service_dispatcher;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

define_windows_service!(ffi_service_main, service_main);

pub struct ServiceEntry {
    name: String,
    runner: Box<dyn Fn(Box<dyn gensvc::Handler>) + Send>,
}

impl ServiceEntry {
    pub fn new(name: String, runner: Box<dyn Fn(Box<dyn gensvc::Handler>) + Send>) -> Self {
        ServiceEntry { name, runner }
    }
}

/// Stores the global information needed to run the service.
/// Right now this is only a singleton, but could be extended to hold multiple entries in the future.
static SERVICE_TABLE: OnceCell<Mutex<ServiceEntry>> = OnceCell::new();

fn service_main(_args: Vec<OsString>) {
    let s = SERVICE_TABLE.get().unwrap().lock().unwrap();
    let handler = ServiceControlHandler::new(&s.name).unwrap();
    (s.runner)(Box::new(handler));
}

pub fn start(mut services: Vec<ServiceEntry>) -> Result<()> {
    if services.len() != 1 {
        return Err(anyhow::anyhow!("service table must contain a single entry"));
    }
    let service_entry = services.pop().unwrap();
    let name = service_entry.name.clone();
    SERVICE_TABLE
        .set(Mutex::new(service_entry))
        .map_err(|_e| anyhow::anyhow!("a service has already been started"))?;
    service_dispatcher::start(name, ffi_service_main).map_err(anyhow::Error::from)
}

pub fn register(
    name: &str,
    display_name: &str,
    description: Option<&str>,
    exe: std::path::PathBuf,
    args: Vec<OsString>,
) -> Result<()> {
    let scm = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;
    let info = ServiceInfo {
        name: OsString::from(name),
        display_name: OsString::from(display_name),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe,
        launch_arguments: args,
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let service = scm.create_service(&info, ServiceAccess::CHANGE_CONFIG)?;
    if let Some(desc) = description {
        service.set_description(desc)?;
    }
    Ok(())
}

pub fn unregister(name: &str) -> Result<()> {
    let scm = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;
    let service = scm.open_service(name, ServiceAccess::DELETE)?;
    service.delete()?;
    Ok(())
}

struct ServiceControlHandler {
    rx: crossbeam_channel::Receiver<()>,
    handle: ServiceStatusHandle,
}

impl ServiceControlHandler {
    fn new(name: &str) -> Result<Self> {
        let (tx, rx) = crossbeam_channel::bounded(0);
        let status_handle = windows_service::service_control_handler::register(name, move |sc| {
            Self::handle(&tx, sc)
        })?;
        Ok(ServiceControlHandler {
            rx,
            handle: status_handle,
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

impl gensvc::Handler for ServiceControlHandler {
    fn chan(&self) -> &crossbeam_channel::Receiver<()> {
        &self.rx
    }

    fn update(
        &self,
        status: gensvc::ServiceState,
        controls_accepted: gensvc::ServiceControlAccept,
    ) -> Result<()> {
        self.handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: status,
            controls_accepted: controls_accepted,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;
        Ok(())
    }
}
