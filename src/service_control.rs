use once_cell::sync::OnceCell;
use std::boxed::Box;
use std::ffi::OsString;
use std::sync::Mutex;
use windows_service::define_windows_service;
use windows_service::service_dispatcher;

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

pub fn register_service(name: String, runner: Box<dyn FnMut() + Send>) -> Result<(), String> {
    SERVICE_TABLE
        .set(Mutex::new(ServiceEntry { name, runner }))
        .map_err(|_err| "Failed to register service".to_string())?;
    Ok(())
}

pub fn start_dispatch() -> Result<(), String> {
    let data = SERVICE_TABLE
        .get()
        .ok_or("No service registered yet".to_string())?
        .lock()
        .map_err(|_err| "Failed to lock service entry".to_string())?;
    let name = data.name.clone();
    service_dispatcher::start(name, ffi_service_main)
        .map_err(|_err| "Failed to start service dispatch".to_string())
}
