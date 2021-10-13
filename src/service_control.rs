use std::boxed::Box;
use std::ffi::OsString;
use std::sync::Mutex;
use once_cell::sync::OnceCell;
use windows_service::define_windows_service;
use windows_service::service_dispatcher;

define_windows_service!(ffi_service_main, service_main);

struct ServiceEntry {
    name: String,
    runner: Box<dyn FnMut() + Send>,
}

static SERVICE_RUNNER: OnceCell<Mutex<ServiceEntry>> = OnceCell::new();

fn service_main(_args: Vec<OsString>) {
    (SERVICE_RUNNER.get().unwrap().lock().unwrap().runner)();
}

pub fn register_service(name: String, runner: Box<dyn FnMut() + Send>) -> Result<(), String>
{
    SERVICE_RUNNER.set(Mutex::new(ServiceEntry{name, runner}))
        .map_err(|_err| "Failed to register service".to_string())?;
    Ok(())
}

pub fn dispatch_service() -> Result<(), String> {
    let data = SERVICE_RUNNER
        .get().ok_or("No service registered yet".to_string())?
        .lock().map_err(|_err| "Failed to lock service entry".to_string())?;
    let name = data.name.clone();
    drop(data);
    service_dispatcher::start(name, ffi_service_main)
        .map_err(|_err| "Failed to start service dispatch".to_string())
}