use std::path::PathBuf;

use clap::{Arg, ArgAction, Command, value_parser};
use const_format::formatcp;
use log::Level;

use crate::core::GenericResult;
use crate::install::Mode;

pub struct CliArgs {
    pub log_level: Level,
    pub config_path: PathBuf,
    pub custom_config: bool,
    pub action: Action,
}

pub enum Action {
    List {
        full: bool,
    },
    Install {
        mode: Mode,
        tools: Option<Vec<String>>,
    },
}

pub fn parse_args() -> GenericResult<CliArgs> {
    const DEFAULT_CONFIG_PATH: &str = formatcp!("~/.config/{}/config.yaml", env!("CARGO_PKG_NAME"));

    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .about(env!("CARGO_PKG_DESCRIPTION"))

        .dont_collapse_args_in_usage(true)
        .disable_help_subcommand(true)
        .subcommand_required(true)
        .help_expected(true)

        .arg(Arg::new("config").short('c').long("config")
            .value_name("PATH")
            .value_parser(value_parser!(PathBuf))
            .help(formatcp!("Configuration file path [default: {}]", DEFAULT_CONFIG_PATH)))

        .arg(Arg::new("verbose")
            .short('v').long("verbose")
            .action(ArgAction::Count)
            .help("Set verbosity level"))

        .subcommand(Command::new("list")
            .about("List all configured tools")
            .args([
                Arg::new("full").short('f').long("full")
                    .help("Show full information including changelog URL")
                    .action(ArgAction::SetTrue),
            ]))

        .subcommand(Command::new("install")
            .about("Install all or only specified tools")
            .args([
                Arg::new("force").short('f').long("force")
                    .help("Force installation even if tool is already installed")
                    .action(ArgAction::SetTrue),

                Arg::new("NAME").help("Tool name").action(ArgAction::Append),
            ]))

        .subcommand(Command::new("upgrade")
            .about("Upgrade all or only specified tools")
            .arg(Arg::new("NAME").help("Tool name").action(ArgAction::Append)))

        .get_matches();

    let log_level = match matches.get_count("verbose") {
        0 => log::Level::Info,
        1 => log::Level::Debug,
        2 => log::Level::Trace,
        _ => return Err!("Invalid verbosity level"),
    };

    let (config_path, custom_config) = match matches.get_one("config").cloned() {
        Some(path) => (path, true),
        None => (PathBuf::from(shellexpand::tilde(DEFAULT_CONFIG_PATH).to_string()), false),
    };

    let (command, matches) = matches.subcommand().unwrap();

    let action = match command {
        "list" => Action::List {
            full: matches.get_flag("full"),
        },

        "install" | "upgrade" => {
            let mode = match command {
                "install" => Mode::Install {
                    force: matches.get_flag("force"),
                },
                "upgrade" => Mode::Upgrade,
                _ => unreachable!(),
            };

            let tools = matches.get_many::<String>("NAME").map(|tools| tools.cloned().collect());

            Action::Install {mode, tools}
        }

        _ => unreachable!(),
    };

    Ok(CliArgs {log_level, config_path, custom_config, action})
}