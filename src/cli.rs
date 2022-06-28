use crate::config;
use crate::service_control;
use crate::svc;
use anyhow::Result;
use clap::Parser;
use std::fs;

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
}

#[derive(clap::Subcommand)]
enum ConfigCommand {
    #[clap(about = "Output a config file with default settings")]
    Default,
}

fn read_config(path: &std::path::PathBuf) -> Result<config::Config> {
    toml::from_str::<config::Config>(&fs::read_to_string(path)?).map_err(anyhow::Error::from)
}

fn register_service(config: &config::Config, config_path: &std::path::PathBuf) -> Result<()> {
    service_control::register(
        &config.registration.name,
        &config.registration.display_name,
        config.registration.description.as_deref(),
        config_path,
    )
}

fn run_service(config: config::Config) -> Result<()> {
    let name = config.registration.name.clone();
    let s = svc::Service::new(config);
    service_control::register_service(name, Box::new(move |handler| s.run(handler).unwrap()))?;
    service_control::start_dispatch()
}

fn setup_logging(config: &config::Config) -> Result<()> {
    if let Some(log_sink) = &config.winsvc.log_sink {
        match log_sink {
            config::LogSink::EventLog => eventlog::init("Application", log::Level::Trace)?,
        }
    }
    Ok(())
}

pub fn run() {
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
        },
    }
}
