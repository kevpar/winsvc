mod config;
mod jobobjects;

use clap::{App, Arg, SubCommand};
use shared_child::SharedChild;
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    os::windows::io::AsRawHandle,
    process::Command,
    sync::Arc,
    time::Duration,
};
use winapi::{
    shared::minwindef,
    um::{errhandlingapi, processenv, winbase},
};
use windows_service::{
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceInfo,
        ServiceStartType, ServiceState, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_manager::{ServiceManager, ServiceManagerAccess},
};

struct Service {
    config: config::Config,
}

impl Service {
    fn new(config: config::Config) -> Self {
        Service { config: config }
    }

    fn set_status(
        status_handle: ServiceStatusHandle,
        status: ServiceState,
        controls_accepted: ServiceControlAccept,
    ) -> windows_service::Result<()> {
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

    fn output_stream(
        config: &config::OutputStream,
    ) -> windows_service::Result<std::process::Stdio> {
        match config {
            config::OutputStream::Null => Ok(std::process::Stdio::null()),
            config::OutputStream::File {
                path,
                exist_behavior,
            } => {
                let mut oo = OpenOptions::new();
                oo.write(true);
                oo.create(true);
                match exist_behavior {
                    config::ExistBehavior::Append => {
                        oo.append(true);
                    }
                    config::ExistBehavior::Truncate => {
                        oo.truncate(true);
                    }
                }
                let f = oo
                    .open(path)
                    .map_err(|err| windows_service::Error::Winapi(err))?;
                Ok(Into::into(f))
            }
        }
    }

    fn run_inner(&self) -> windows_service::Result<()> {
        let (tx, rx) = crossbeam_channel::bounded(0);
        let handler = ServiceControlHandler::new(tx.clone());
        let status_handle =
            service_control_handler::register(&self.config.registration.name, move |sc| {
                handler.handle(sc)
            })?;

        let job =
            jobobjects::JobObject::new().map_err(|err| windows_service::Error::Winapi(err))?;
        let mut limits = jobobjects::ExtendedLimitInformation::new();
        limits.set_kill_on_close();
        if let Some(job_object_config) = &self.config.job_object {
            if let Some(class) = &job_object_config.priority_class {
                limits.set_priority_class(*class);
            }
        }
        job.set_extended_limits(limits)
            .map_err(|err| windows_service::Error::Winapi(err))?;
        job.add_self()
            .map_err(|err| windows_service::Error::Winapi(err))?;

        let mut c = Command::new(&self.config.process.binary);
        c.args(&self.config.process.args);
        c.envs(&self.config.process.environment);
        if let Some(wd) = &self.config.process.working_directory {
            fs::create_dir_all(wd).map_err(|err| windows_service::Error::Winapi(err))?;
            c.current_dir(wd);
        }
        c.stdout(Service::output_stream(&self.config.process.stdout)?);
        c.stderr(Service::output_stream(&self.config.process.stderr)?);

        let child =
            SharedChild::spawn(&mut c).map_err(|err| windows_service::Error::Winapi(err))?;
        let child = Arc::new(child);
        let waiter_child = child.clone();
        println!("child started with pid {}", child.id());
        let (child_tx, child_rx) = crossbeam_channel::bounded(0);
        let _t = std::thread::spawn(move || {
            waiter_child.wait().unwrap();
            child_tx.send(()).unwrap();
        });
        Service::set_status(
            status_handle,
            ServiceState::Running,
            ServiceControlAccept::STOP,
        )?;
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
    chan: crossbeam_channel::Sender<()>,
}

impl ServiceControlHandler {
    fn new(chan: crossbeam_channel::Sender<()>) -> Self {
        ServiceControlHandler { chan }
    }

    fn handle(&self, sc: ServiceControl) -> ServiceControlHandlerResult {
        match sc {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                self.chan.send(()).unwrap();
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
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

    static SERVICE_RUNNER: OnceCell<Mutex<ServiceEntry>> = OnceCell::new();

    fn service_main(_args: Vec<OsString>) {
        (SERVICE_RUNNER.get().unwrap().lock().unwrap().runner)();
    }

    pub fn register_service(name: String, runner: Box<dyn FnMut() + Send>) -> Result<(), String> {
        SERVICE_RUNNER
            .set(Mutex::new(ServiceEntry { name, runner }))
            .map_err(|_err| "Failed to register service".to_string())?;
        Ok(())
    }

    pub fn dispatch_service() -> Result<(), String> {
        let data = SERVICE_RUNNER
            .get()
            .ok_or("No service registered yet".to_string())?
            .lock()
            .map_err(|_err| "Failed to lock service entry".to_string())?;
        let name = data.name.clone();
        drop(data);
        service_dispatcher::start(name, ffi_service_main)
            .map_err(|_err| "Failed to start service dispatch".to_string())
    }
}

fn main() {
    let matches = App::new("Windows Service Shim")
        .version("0.1")
        .about("Adapts a console application to run as a Windows service.")
        .subcommand(
            SubCommand::with_name("run")
                .arg(Arg::with_name("config").help("Path to the service config file")),
        )
        .subcommand(
            SubCommand::with_name("register")
                .arg(Arg::with_name("config").help("Path to the service config file")),
        )
        .subcommand(SubCommand::with_name("config").subcommand(SubCommand::with_name("default")))
        .get_matches();

    match matches.subcommand() {
        ("config", Some(matches)) => {
            if let Some(_) = matches.subcommand_matches("default") {
                let c = config::Config::default();
                println!("{}", toml::to_string(&c).unwrap());
            }
        }
        ("register", Some(matches)) => {
            let config = matches.value_of("config").expect("--config is required");
            let c: config::Config =
                toml::from_str(&fs::read_to_string(config).expect("failed to read config file"))
                    .expect("config failed to parse");
            let scm = ServiceManager::local_computer(
                None::<&str>,
                ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
            )
            .unwrap();
            let info = ServiceInfo {
                name: OsString::from(&c.registration.name),
                display_name: OsString::from(&c.registration.display_name),
                service_type: ServiceType::OWN_PROCESS,
                start_type: ServiceStartType::AutoStart,
                error_control: ServiceErrorControl::Normal,
                executable_path: std::env::current_exe().unwrap(),
                launch_arguments: vec![OsString::from("run"), OsString::from(config)],
                dependencies: vec![],
                account_name: None,
                account_password: None,
            };
            let service = scm
                .create_service(&info, ServiceAccess::CHANGE_CONFIG)
                .unwrap();
            if let Some(desc) = c.registration.description {
                service.set_description(desc).unwrap()
            }
        }
        ("run", Some(matches)) => {
            let f = fs::File::create("c:\\svc\\log.txt").expect("failed to open log file");
            set_stdio(&f).expect("failed to set stdio");
            let config = matches.value_of("config").expect("--config is required");
            let c: config::Config =
                toml::from_str(&fs::read_to_string(config).expect("failed to read config file"))
                    .unwrap();
            println!("config: {:?}", c);
            let name = c.registration.name.clone();
            let s = Service::new(c);
            service_control::register_service(name, Box::new(move || s.run())).unwrap();
            service_control::dispatch_service().unwrap();
        }
        _ => {}
    }
}
