use crate::config;
use crate::gensvc;
use crate::jobobjects;

use anyhow::Result;
use shared_child::SharedChild;
use std::{
    fs::{self, OpenOptions},
    process::Command,
    sync::Arc,
};

pub struct Service {
    config: config::Config,
}

impl Service {
    pub fn new(config: config::Config) -> Self {
        Service { config: config }
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
                let f = oo.open(path)?;
                Ok(Into::into(f))
            }
        }
    }

    pub fn run(&self, handler: Box<dyn gensvc::Handler>) -> Result<()> {
        let job = jobobjects::JobObject::new()?;
        let mut limits = jobobjects::ExtendedLimitInformation::new();
        limits.set_kill_on_close();
        if let Some(job_object_config) = &self.config.job_object {
            if let Some(class) = &job_object_config.priority_class {
                limits.set_priority_class(*class);
            }
        }
        job.set_extended_limits(limits)?;
        job.add_self()?;

        let mut c = Command::new(&self.config.process.binary);
        c.args(&self.config.process.args);
        c.envs(&self.config.process.environment);
        if let Some(wd) = &self.config.process.working_directory {
            fs::create_dir_all(wd)?;
            c.current_dir(wd);
        }
        c.stdout(Service::output_stream(&self.config.process.stdout)?);
        c.stderr(Service::output_stream(&self.config.process.stderr)?);

        let child = SharedChild::spawn(&mut c)?;
        let child = Arc::new(child);
        let waiter_child = child.clone();
        log::debug!("child started with pid {}", child.id());
        let (child_tx, child_rx) = crossbeam_channel::bounded(0);
        let _t = std::thread::spawn(move || {
            waiter_child.wait().unwrap();
            child_tx.send(()).unwrap();
        });
        handler.update(
            gensvc::ServiceState::Running,
            gensvc::ServiceControlAccept::STOP,
        )?;
        loop {
            crossbeam_channel::select! {
                recv(handler.chan()) -> msg => {
                    msg.unwrap();
                    log::debug!("stop signal received");
                    handler.update(gensvc::ServiceState::StopPending, gensvc::ServiceControlAccept::empty())?;
                    child.kill().unwrap();
                },
                recv(child_rx) -> msg => {
                    msg.unwrap();
                    log::debug!("child terminated");
                    handler.update(gensvc::ServiceState::Stopped, gensvc::ServiceControlAccept::empty())?;
                    return Ok(());
                }
            }
        }
    }
}
