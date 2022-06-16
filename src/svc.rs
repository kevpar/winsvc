use crate::config;
use crate::jobobjects;
use crate::service_control;

use shared_child::SharedChild;
use std::{
    fs::{self, OpenOptions},
    process::Command,
    sync::Arc,
};

// windows_service type aliases.
// Maybe replace these with our own abstraction in the future.
type Result<T> = windows_service::Result<T>;
type Error = windows_service::Error;
type ServiceControlAccept = windows_service::service::ServiceControlAccept;
type ServiceState = windows_service::service::ServiceState;

pub struct Service {
    config: config::Config,
}

impl Service {
    pub fn new(config: config::Config) -> Self {
        Service { config: config }
    }

    pub fn run(&self) {
        self.run_inner(
            service_control::ServiceControlHandler::new(&self.config.registration.name).unwrap(),
        )
        .unwrap();
    }

    fn output_stream(config: &config::OutputStream) -> Result<std::process::Stdio> {
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
                let f = oo.open(path).map_err(|err| Error::Winapi(err))?;
                Ok(Into::into(f))
            }
        }
    }

    // TODO remove notes
    // service_control (winsvc?) provides all special bindings to tie to SCM
    // svc provides core concept of a runnable service, receives some abstraction over control handling and setting service status
    //   should be something we can pull out and test with a mock SCM
    //
    // outer wrapper talks SCM concepts to SCM
    //   ServiceControlHandler provides a way to receive events as well as synchronously set service status
    // inner wrapper takes a ServiceControlHandler and provides a way to run some arbitrary code
    // concrete implementation of inner wrapper uses this system to run a Windows service

    fn run_inner(&self, handler: service_control::ServiceControlHandler) -> Result<()> {
        let rx = handler.chan();

        let job = jobobjects::JobObject::new().map_err(|err| Error::Winapi(err))?;
        let mut limits = jobobjects::ExtendedLimitInformation::new();
        limits.set_kill_on_close();
        if let Some(job_object_config) = &self.config.job_object {
            if let Some(class) = &job_object_config.priority_class {
                limits.set_priority_class(*class);
            }
        }
        job.set_extended_limits(limits)
            .map_err(|err| Error::Winapi(err))?;
        job.add_self().map_err(|err| Error::Winapi(err))?;

        let mut c = Command::new(&self.config.process.binary);
        c.args(&self.config.process.args);
        c.envs(&self.config.process.environment);
        if let Some(wd) = &self.config.process.working_directory {
            fs::create_dir_all(wd).map_err(|err| Error::Winapi(err))?;
            c.current_dir(wd);
        }
        c.stdout(Service::output_stream(&self.config.process.stdout)?);
        c.stderr(Service::output_stream(&self.config.process.stderr)?);

        let child = SharedChild::spawn(&mut c).map_err(|err| Error::Winapi(err))?;
        let child = Arc::new(child);
        let waiter_child = child.clone();
        println!("child started with pid {}", child.id());
        let (child_tx, child_rx) = crossbeam_channel::bounded(0);
        let _t = std::thread::spawn(move || {
            waiter_child.wait().unwrap();
            child_tx.send(()).unwrap();
        });
        handler.update(ServiceState::Running, ServiceControlAccept::STOP)?;
        loop {
            crossbeam_channel::select! {
                recv(rx) -> msg => {
                    msg.unwrap();
                    println!("stop signal received");
                    handler.update(ServiceState::StopPending, ServiceControlAccept::empty())?;
                    child.kill().unwrap();
                },
                recv(child_rx) -> msg => {
                    msg.unwrap();
                    println!("child terminated");
                    handler.update(ServiceState::Stopped, ServiceControlAccept::empty())?;
                    return Ok(());
                }
            }
        }
    }
}
