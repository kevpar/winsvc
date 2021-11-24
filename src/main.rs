mod config;
mod jobobjects;
mod service_control;
mod svc;

use clap::{App, Arg, SubCommand};
use std::{ffi::OsString, fs, os::windows::io::AsRawHandle};
use winapi::{
    shared::minwindef,
    um::{errhandlingapi, processenv, winbase},
};
use windows_service::{
    service::{ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType},
    service_manager::{ServiceManager, ServiceManagerAccess},
};

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
            let config_path = matches.value_of("config").expect("--config is required");
            let config: config::Config = toml::from_str(
                &fs::read_to_string(config_path).expect("failed to read config file"),
            )
            .unwrap();
            if let Some(winsvc_config) = &config.winsvc {
                if let Some(path) = &winsvc_config.log_path {
                    let f = fs::File::create(path).expect("failed to open log file");
                    set_stdio(&f).expect("failed to set stdio");
                }
            }
            println!("config: {:?}", config);
            let name = config.registration.name.clone();
            let s = svc::Service::new(config);
            service_control::register_service(name, Box::new(move || s.run())).unwrap();
            service_control::dispatch_service().unwrap();
        }
        _ => {}
    }
}
