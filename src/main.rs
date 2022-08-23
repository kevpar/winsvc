mod config;
mod gensvc;
mod jobobjects;
mod svc;
mod winsvc;

use anyhow::Result;
use clap::Parser;
use std::{ffi::OsString, fs};

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
    #[clap(about = "Unregister a service")]
    Unregister {
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
}

#[derive(clap::Subcommand)]
enum ConfigCommand {
    #[clap(about = "Output a config file with default settings")]
    Default,
    #[clap(about = "Check a config file for errors")]
    Check {
        #[clap(help = "Path to the service config file")]
        config: std::path::PathBuf,
    },
}

fn read_config(path: &std::path::PathBuf) -> Result<config::Config> {
    toml::from_str::<config::Config>(&fs::read_to_string(path)?).map_err(anyhow::Error::from)
}

fn register_service(config: &config::Config, config_path: &std::path::PathBuf) -> Result<()> {
    winsvc::register(
        &config.registration.name,
        &config.registration.display_name,
        config.registration.description.as_deref(),
        std::env::current_exe()?,
        vec![OsString::from("run"), OsString::from(config_path)],
    )
}

fn unregister_service(config: &config::Config) -> Result<()> {
    winsvc::unregister(&config.registration.name)
}

fn run_service(config: config::Config) -> Result<()> {
    let name = config.registration.name.clone();
    let s = svc::Service::new(config);
    winsvc::start(vec![winsvc::ServiceEntry::new(
        name,
        Box::new(move |handler| s.run(handler).unwrap()),
    )])
}

fn setup_logging(config: &config::Config) -> Result<()> {
    if let Some(log_sink) = &config.winsvc.log_sink {
        match log_sink {
            config::LogSink::EventLog => eventlog::init("Application", log::Level::Trace)?,
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Register { config } => {
            let c = read_config(&config).expect("failed reading config");
            setup_logging(&c).expect("failed to setup logging");
            log::debug!(
                "registering service {} with config {}",
                c.registration.name,
                config.display()
            );
            register_service(&c, &config).expect("failed to register service");
        }
        Command::Unregister { config } => {
            let c = read_config(&config).expect("failed reading config");
            setup_logging(&c).expect("failed to setup logging");
            log::debug!(
                "unregistering service {} with config {}",
                c.registration.name,
                config.display()
            );
            unregister_service(&c).expect("failed to unregister service");
        }
        Command::Run { config } => {
            let c = read_config(&config).expect("failed reading config");
            setup_logging(&c).expect("failed to setup logging");
            log::debug!(
                "running service {} with config {}",
                c.registration.name,
                config.display()
            );
            run_service(c).expect("failed to run service");
        }
        Command::Config { command } => match command {
            ConfigCommand::Default => {
                let c = config::Config::default();
                println!("{}", toml::to_string(&c).unwrap());
            }
            ConfigCommand::Check { config } => {
                read_config(&config).expect("failed reading config");
            }
        },
    }
}
