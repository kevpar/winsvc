use crate::config;
use crate::service_control;
use crate::svc;
use clap::Parser;
use std::{fs, os::windows::io::AsRawHandle};
use winapi::{
    shared::minwindef,
    um::{errhandlingapi, processenv, winbase},
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

#[derive(clap::Parser)]
#[clap(author, version, about)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    #[clap(about = "Register a service to run")]
    Register {
        #[clap(help = "Path to the service config file")]
        config: std::path::PathBuf,
    },
    #[clap(about = "Run a service", hide = true)]
    Run {
        #[clap(help = "Path to the service config file")]
        config: std::path::PathBuf,
    },
    #[clap(about = "Interact with config files")]
    Config {
        #[clap(subcommand)]
        command: ConfigCommand,
    },
    // TODO diag
    // inspect (see child pid etc)
    // something with sd notify protocol?
    // rotate logs?
}

#[derive(clap::Subcommand)]
enum ConfigCommand {
    #[clap(about = "Output a config file with default settings")]
    Default,
}

fn read_config(path: &std::path::PathBuf) -> config::Config {
    toml::from_str(&fs::read_to_string(path).expect("failed to read config file"))
        .expect("failed to parse config file")
}

fn register_service(config: &config::Config, config_path: &std::path::PathBuf) {
    service_control::register(
        &config.registration.name,
        &config.registration.display_name,
        config.registration.description.as_deref(),
        config_path,
    )
    .unwrap();
}

fn run_service(config: config::Config) {
    if let Some(winsvc_config) = &config.winsvc {
        if let Some(path) = &winsvc_config.log_path {
            let f = fs::File::create(path).expect("failed to open log file");
            set_stdio(&f).expect("failed to set stdio");
        }
    }
    println!("config: {:?}", config);
    let name = config.registration.name.clone();
    let s = svc::Service::new(config);
    service_control::register_service(name, Box::new(move |handler| s.run(handler).unwrap()))
        .unwrap();
    service_control::start_dispatch().unwrap();
}

pub fn run() {
    let cli = Cli::parse();

    match cli.command {
        Command::Register { config } => {
            let c = read_config(&config);
            register_service(&c, &config);
        }
        Command::Run { config } => {
            let c = read_config(&config);
            run_service(c);
        }
        Command::Config { command } => match command {
            ConfigCommand::Default => {
                let c = config::Config::default();
                println!("{}", toml::to_string(&c).unwrap());
            }
        },
    }
}
