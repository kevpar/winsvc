use crate::jobobjects;

use serde_derive::Deserialize;
use std::{
    collections,
    path::{PathBuf},
};

#[derive(Deserialize, Debug)]
pub struct JobObject {
    pub priority_class: Option<jobobjects::PriorityClass>,
}

#[derive(Deserialize, Debug)]
pub struct Registration {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Process {
    pub binary: String,
    pub args: Option<Vec<String>>,
    pub working_directory: Option<PathBuf>,
    pub environment: Option<collections::HashMap<String, String>>,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub registration: Registration,
    pub process: Process,
    pub job_object: Option<JobObject>,
    // config relative to winsvc path
    // user binary relative to config path
    // pid file
    // logging
    // console creation
}