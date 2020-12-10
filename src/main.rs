use clap::{App, Arg, SubCommand};
use serde_derive::Deserialize;
use std::fs;
use winapi::um::winsvc::SC_HANDLE;

#[derive(Deserialize)]
struct Config {
    binary: String,
    // binary relative to config path
    // configure job object
    // pid file
    // logging
    // console creation
}

struct SCM {
    h: SC_HANDLE,
}

fn main() {
    println!("Hello, world!");
    let p = MyEvents::new();
    p.foo(None, "test");
    let _a = 5;
    println!("{}", _a);

    let matches = App::new("Windows Service Shim")
        .version("0.1")
        .about("Helps run programs as Windows services in a sane way.")
        .subcommand(SubCommand::with_name("register")
            .about("Registers a service to run via winsvc")
            .arg(Arg::with_name("name")
                .help("Name to register the service as"))
            .arg(Arg::with_name("config")
                .help("Path to the service config file")))
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("register") {
        println!("in register");
        if matches.is_present("name") {
            let name = matches.value_of("name").expect("--name is required");
            let config = matches.value_of("config").expect("--config is required");
            let c: Config = toml::from_str(&fs::read_to_string(config).expect("failed to read config file")).expect("config failed to parse");
                println!("{}", c.binary);
        }
    }
}

#[win_etw_macros::trace_logging_provider(guid = "b6d467c1-b86a-489d-a358-3908adc26243")]
pub trait MyEvents {
    fn foo(s: &str);
}