use std::path::PathBuf;

use clap::{Command, Arg, ArgAction, value_parser};
use const_format::formatcp;
use log::Level;

use crate::core::GenericResult;

pub struct CliArgs {
    pub log_level: Level,
    pub config_path: PathBuf,
}

pub fn parse_args() -> GenericResult<CliArgs> {
    const DEFAULT_CONFIG_PATH: &str = "~/.config/get-release/config.yaml";

    let matches = Command::new("get-release")
        .version(env!("CARGO_PKG_VERSION"))

        .dont_collapse_args_in_usage(true)
        .disable_help_subcommand(true)
        .help_expected(true)

        .arg(Arg::new("config").short('c').long("config")
            .value_name("PATH")
            .value_parser(value_parser!(PathBuf))
            .help(formatcp!("Configuration file path [default: {}]", DEFAULT_CONFIG_PATH)))

        .arg(Arg::new("verbose")
            .short('v').long("verbose")
            .action(ArgAction::Count)
            .help("Set verbosity level"))

        .get_matches();

    let log_level = match matches.get_count("verbose") {
        0 => log::Level::Info,
        1 => log::Level::Debug,
        2 => log::Level::Trace,
        _ => return Err!("Invalid verbosity level"),
    };

    let config_path = matches.get_one("config").cloned().unwrap_or_else(||
        PathBuf::from(shellexpand::tilde(DEFAULT_CONFIG_PATH).to_string()));

    Ok(CliArgs {log_level, config_path})
}