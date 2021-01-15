use clap::{App, Arg, SubCommand};
use serde_derive::Deserialize;
use shared_child::SharedChild;
use std::{
    ffi::OsString,
    fs,
    os::windows::io::AsRawHandle,
    path::{PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};
use winapi::{
    shared::minwindef,
    shared::ntdef,
    um::{errhandlingapi, handleapi, jobapi2, minwinbase, processenv, processthreadsapi, winbase, winnt},
};
use windows_service::{
    service::{ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_manager::{ServiceManager, ServiceManagerAccess},
};

#[derive(Deserialize)]
#[derive(Debug)]
#[derive(Clone)]
struct Config {
    name: String,
    display_name: String,
    description: Option<String>,
    binary: String,
    args: Option<Vec<String>>,
    output_dir: Option<PathBuf>,
    // env vars
    // binary relative to config path
    // configure job object
    // pid file
    // logging
    // console creation
    // working directory
}

struct JobObject {
    handle: ntdef::HANDLE,
}

impl JobObject {
    fn new() -> Result<Self, std::io::Error> {
        let handle = unsafe { jobapi2::CreateJobObjectW(0 as minwinbase::LPSECURITY_ATTRIBUTES, 0 as ntdef::LPCWSTR) };
        if handle == 0 as ntdef::HANDLE {
            return Err(std::io::Error::last_os_error());
        }
        Ok(JobObject{handle: handle})
    }

    fn set_kill_on_close(&self) -> Result<(), std::io::Error> {
        let mut info: winnt::JOBOBJECT_EXTENDED_LIMIT_INFORMATION = Default::default();
        info.BasicLimitInformation.LimitFlags |= winnt::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let result = unsafe {jobapi2::SetInformationJobObject(self.handle, winnt::JobObjectExtendedLimitInformation, &mut info as *mut _ as minwindef::LPVOID, std::mem::size_of_val(&info) as u32) };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn add_self(&self) -> Result<(), std::io::Error> {
        let result = unsafe { jobapi2::AssignProcessToJobObject(self.handle, processthreadsapi::GetCurrentProcess()) };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe { handleapi::CloseHandle(self.handle) };
    }
}

struct Service {
    config: Config,
}

impl Service {
    fn new(config: Config) -> Self {
        Service{
            config: config,
        }
    }

    fn set_status(status_handle: ServiceStatusHandle, status: ServiceState, controls_accepted: ServiceControlAccept) -> windows_service::Result<()> {
        status_handle.set_service_status(windows_service::service::ServiceStatus {
            service_type: windows_service::service::ServiceType::OWN_PROCESS,
            current_state: status,
            controls_accepted: controls_accepted,
            exit_code: windows_service::service::ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
    }

    fn run(&self) {
        self.run_inner().unwrap();
    }

    fn run_inner(&self) -> windows_service::Result<()> {
        let (tx, rx) = crossbeam_channel::bounded(0);
        let tx = Arc::new(tx);
        let handler = ServiceControlHandler::new(&tx);
        let status_handle = service_control_handler::register(&self.config.name, move |sc| handler.handle(sc))?;
        let args = match &self.config.args {
            Some(v) => v.iter().map(|s| OsString::from(s)).collect(),
            None => Vec::new()
        };
        let job = JobObject::new().map_err(|err| windows_service::Error::Winapi(err))?;
        job.set_kill_on_close().map_err(|err| windows_service::Error::Winapi(err))?;
        job.add_self().map_err(|err| windows_service::Error::Winapi(err))?;
        let mut c = Command::new(&self.config.binary);
        c.args(args);
        let child = SharedChild::spawn(&mut c).map_err(|err| windows_service::Error::Winapi(err))?;
        let child = Arc::new(child);
        let waiter_child = child.clone();
        println!("child started with pid {}", child.id());
        let (child_tx, child_rx) = crossbeam_channel::bounded(0);
        let _t = std::thread::spawn(move || {
            waiter_child.wait().unwrap();
            child_tx.send(true).unwrap();
        });
        Service::set_status(status_handle, ServiceState::Running, ServiceControlAccept::STOP)?;
        loop {
            crossbeam_channel::select! {
                recv(rx) -> msg => {
                    msg.unwrap();
                    println!("stop signal received");
                    Service::set_status(status_handle, ServiceState::StopPending, ServiceControlAccept::empty())?;
                    child.kill().unwrap();
                },
                recv(child_rx) -> msg => {
                    msg.unwrap();
                    println!("child terminated");
                    Service::set_status(status_handle, ServiceState::Stopped, ServiceControlAccept::empty())?;
                    return Ok(());
                }
            }
        }
    }
}

struct ServiceControlHandler {
    chan: Arc<crossbeam_channel::Sender<bool>>,
}

impl ServiceControlHandler {
    fn new(chan: &Arc<crossbeam_channel::Sender<bool>>) -> Self {
        ServiceControlHandler{chan: chan.clone()}
    }

    fn handle(&self, sc: ServiceControl) -> ServiceControlHandlerResult {
        match sc {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                self.chan.send(true).unwrap();
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented
        }
    }
}

fn set_stdio(f: &std::fs::File) -> Result<(), minwindef::DWORD> {
    let h = f.as_raw_handle();
    unsafe {
        if processenv::SetStdHandle(winbase::STD_OUTPUT_HANDLE, h) == 0 {
            return Err(errhandlingapi::GetLastError());
        }
        if processenv::SetStdHandle(winbase::STD_ERROR_HANDLE, h) == 0 {
            return Err(errhandlingapi::GetLastError());
        }
    }
    Ok(())
}

mod service_control {
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
}

fn main() {
    let matches = App::new("Windows Service Shim")
        .version("0.1")
        .about("Helps run programs as Windows services in a sane way.")
        .subcommand(
            SubCommand::with_name("run")
                .arg(Arg::with_name("config").help("Path to the service config file")),
        )
        .subcommand(
            SubCommand::with_name("register")
                .arg(Arg::with_name("config").help("Path to the service config file")),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("register") {
        let config = matches.value_of("config").expect("--config is required");
        let c: Config =
            toml::from_str(&fs::read_to_string(config).expect("failed to read config file"))
                .expect("config failed to parse");
        let scm = ServiceManager::local_computer(
            None::<&str>,
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
        ).unwrap();
        let info = ServiceInfo {
            name: OsString::from(&c.name),
            display_name: OsString::from(&c.display_name),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: std::env::current_exe().unwrap(),
            launch_arguments: vec![OsString::from("run"), OsString::from(config)],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };
        let service = scm.create_service(&info, ServiceAccess::CHANGE_CONFIG).unwrap();
        if let Some(desc) = c.description {
            service.set_description(desc).unwrap()
        }
    } else if let Some(matches) = matches.subcommand_matches("run") {
        let f = fs::File::create("c:\\svc\\log.txt").expect("failed to open log file");
        set_stdio(&f).expect("failed to set stdio");
        let config = matches.value_of("config").expect("--config is required");
        let c: Config = toml::from_str(&fs::read_to_string(config).expect("failed to read config file")).unwrap();
        let name = c.name.clone();
        let s = Service::new(c);
        service_control::register_service(name, Box::new(move || s.run())).unwrap();
        service_control::dispatch_service().unwrap();
    }
}