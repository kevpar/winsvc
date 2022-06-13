use crate::config;
use crate::jobobjects;

use shared_child::SharedChild;
use std::{
    fs::{self, OpenOptions},
    process::Command,
    sync::Arc,
    time::Duration,
};
use windows_service::{
    service::{ServiceControl, ServiceControlAccept, ServiceState},
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
};

pub struct Service {
    config: config::Config,
}

impl Service {
    pub fn new(config: config::Config) -> Self {
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

    pub fn run(&self) {
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

    // TODO remove notes
    // service_control (winsvc?) provides all special bindings to tie to SCM
    // svc provides core concept of a runnable service, receives some abstraction over control handling and setting service status
    //   should be something we can pull out and test with a mock SCM

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
