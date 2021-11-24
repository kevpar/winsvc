use crate::jobobjects;

use serde_derive::{Deserialize, Serialize};
use std::collections;
use std::path::PathBuf;

#[derive(Deserialize, Debug, Serialize)]
pub struct JobObject {
    pub priority_class: Option<jobobjects::PriorityClass>,
}

#[derive(Deserialize, Debug, Default, Serialize)]
pub struct Registration {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
}

#[derive(Deserialize, Debug, Serialize)]
#[serde(tag = "type")]
pub enum ExistBehavior {
    Append,
    Truncate,
}

impl Default for ExistBehavior {
    fn default() -> Self {
        ExistBehavior::Append
    }
}

#[derive(Deserialize, Debug, Serialize)]
#[serde(tag = "type")]
pub enum OutputStream {
    Null,
    File {
        path: PathBuf,
        #[serde(default)]
        exist_behavior: ExistBehavior,
    },
}

impl Default for OutputStream {
    fn default() -> Self {
        OutputStream::Null
    }
}

#[derive(Deserialize, Debug, Default, Serialize)]
pub struct Process {
    pub binary: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub working_directory: Option<PathBuf>,
    #[serde(default)]
    pub environment: collections::HashMap<String, String>,
    #[serde(default)]
    pub stdout: OutputStream,
    #[serde(default)]
    pub stderr: OutputStream,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct WinSvc {
    pub log_path: Option<PathBuf>,
}

#[derive(Deserialize, Debug, Default, Serialize)]
pub struct Config {
    pub winsvc: Option<WinSvc>,
    pub registration: Registration,
    pub process: Process,
    pub job_object: Option<JobObject>,
    // config relative to winsvc path
    // user binary relative to config path
    // pid file
    // logging
    // console creation
}
