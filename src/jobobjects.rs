use anyhow::Result;
use serde_derive::{Deserialize, Serialize};
use winapi::{
    shared::minwindef,
    shared::ntdef,
    um::{handleapi, jobapi2, minwinbase, processthreadsapi, winbase, winnt},
};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum PriorityClass {
    Idle = winbase::IDLE_PRIORITY_CLASS as isize,
    BelowNormal = winbase::BELOW_NORMAL_PRIORITY_CLASS as isize,
    Normal = winbase::NORMAL_PRIORITY_CLASS as isize,
    AboveNormal = winbase::ABOVE_NORMAL_PRIORITY_CLASS as isize,
    High = winbase::HIGH_PRIORITY_CLASS as isize,
    Realtime = winbase::REALTIME_PRIORITY_CLASS as isize,
}

pub struct ExtendedLimitInformation(winnt::JOBOBJECT_EXTENDED_LIMIT_INFORMATION);

impl ExtendedLimitInformation {
    pub fn new() -> Self {
        ExtendedLimitInformation(Default::default())
    }

    pub fn set_kill_on_close(&mut self) -> &mut Self {
        self.0.BasicLimitInformation.LimitFlags |= winnt::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        self
    }

    pub fn set_priority_class(&mut self, class: PriorityClass) -> &mut Self {
        self.0.BasicLimitInformation.PriorityClass = class as u32;
        self.0.BasicLimitInformation.LimitFlags |= winnt::JOB_OBJECT_LIMIT_PRIORITY_CLASS;
        self
    }
}

pub struct JobObject {
    handle: ntdef::HANDLE,
}

impl JobObject {
    pub fn new() -> Result<Self> {
        let handle = unsafe {
            jobapi2::CreateJobObjectW(0 as minwinbase::LPSECURITY_ATTRIBUTES, 0 as ntdef::LPCWSTR)
        };
        if handle == 0 as ntdef::HANDLE {
            return Err(anyhow::Error::from(std::io::Error::last_os_error()));
        }
        Ok(JobObject { handle: handle })
    }

    pub fn set_extended_limits(
        &self,
        mut limits: ExtendedLimitInformation,
    ) -> Result<(), std::io::Error> {
        let result = unsafe {
            jobapi2::SetInformationJobObject(
                self.handle,
                winnt::JobObjectExtendedLimitInformation,
                &mut limits.0 as *mut _ as minwindef::LPVOID,
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn add_self(&self) -> Result<(), std::io::Error> {
        let result = unsafe {
            jobapi2::AssignProcessToJobObject(self.handle, processthreadsapi::GetCurrentProcess())
        };
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
