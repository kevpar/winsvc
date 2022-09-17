use anyhow::Result;

pub type ServiceControlAccept = windows_service::service::ServiceControlAccept;
pub type ServiceState = windows_service::service::ServiceState;
pub type ServiceControl = windows_service::service::ServiceControl;

pub trait Handler {
    fn chan(&self) -> &crossbeam_channel::Receiver<ServiceControl>;
    fn update(&self, status: ServiceState, controls_accepted: ServiceControlAccept) -> Result<()>;
}
