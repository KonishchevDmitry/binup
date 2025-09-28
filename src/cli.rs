use std::path::PathBuf;

use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use const_format::formatcp;
use log::Level;
use url::Url;

use crate::core::GenericResult;
use crate::matcher::Matcher;
use crate::install::Mode;
use crate::tool::ToolSpec;
use crate::version::VersionSource;

pub struct CliArgs {
    pub log_level: Level,
    pub config_path: PathBuf,
    pub custom_config: bool,
    pub action: Action,
}

#[allow(clippy::large_enum_variant)]
pub enum Action {
    List {
        local: bool,
        prerelease: bool,
        full: bool,
    },
    Install {
        mode: Mode,
        names: Vec<String>,
    },
    InstallFromSpec {
        name: Option<String>,
        spec: ToolSpec,
        force: bool,
    },
    Uninstall {
        names: Vec<String>,
    }
}

macro_rules! long_about {
    ($text:expr) => {{
        textwrap::fill(indoc::indoc!($text).trim_matches('\n'), 100)
    }}
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

        .subcommand(Command::new("list").visible_alias("l")
            .about("List all configured tools")
            .args([
                Arg::new("local").short('l').long("local")
                    .action(ArgAction::SetTrue)
                    .help("Don't send any network requests and show only locally available information"),
                Arg::new("prerelease").short('u').long("prerelease")
                    .action(ArgAction::SetTrue)
                    .help("Don't filter out prerelease versions"),
                Arg::new("full").short('f').long("full")
                    .action(ArgAction::SetTrue)
                    .help("Show full information including changelog URL"),
            ]))

        .subcommand(Command::new("install").visible_alias("i")
            .about("Install all or only specified tools")
            .long_about(long_about!("
                When no arguments are specified, installs all the tools from the configuration file which aren't
                installed yet. When tool name(s) is specified, installs this specific tool(s). When --project is
                specified, adds a new tool to the configuration file and installs it.
            "))
            .args([
                Arg::new("name")
                    .value_name("NAME")
                    .action(ArgAction::Append)
                    .help("Tool name"),

                Arg::new("force").short('f').long("force")
                    .action(ArgAction::SetTrue)
                    .help("Force installation even if tool is already installed"),

                Arg::new("project").short('p').long("project")
                    .value_name("NAME")
                    .help("GitHub project to get the release from"),

                Arg::new("prerelease").short('u').long("prerelease")
                    .action(ArgAction::SetTrue)
                    .requires("project")
                    .help("Allow installation of prerelease version"),

                Arg::new("changelog").short('c').long("changelog")
                    .value_name("URL")
                    .requires("project")
                    .help("Project changelog URL"),

                Arg::new("release_matcher").short('r').long("release-matcher")
                    .value_name("PATTERN")
                    .requires("project")
                    .help("Release archive pattern"),

                Arg::new("binary_matcher").short('b').long("binary-matcher")
                    .value_name("PATTERN")
                    .requires("project")
                    .help("Binary path to look for inside the release archive"),

                Arg::new("version_source").short('v').long("version-source")
                    .value_name("SOURCE")
                    .requires("project")
                    .value_parser(value_parser!(VersionSource))
                    .help("Method which is used to determine current binary version [default: flag]"),

                Arg::new("path").short('d').long("path")
                    .value_name("PATH")
                    .requires("project")
                    .value_parser(value_parser!(PathBuf))
                    .help("Path where to install this specific tool to"),

                Arg::new("post").short('s').long("post")
                    .value_name("COMMAND")
                    .requires("project")
                    .help("Post-install command"),
            ]))

        .subcommand(Command::new("upgrade").visible_alias("u")
            .about("Upgrade all or only specified tools")
            .args([
                Arg::new("name")
                    .value_name("NAME")
                    .action(ArgAction::Append)
                    .help("Tool name"),
                Arg::new("prerelease").short('u').long("prerelease")
                    .action(ArgAction::SetTrue)
                    .help("Allow upgrade to prerelease version"),
            ]))

        .subcommand(Command::new("uninstall").visible_aliases(["remove", "r"])
            .about("Uninstall the specified tools")
            .arg(Arg::new("name")
                .value_name("NAME")
                .action(ArgAction::Append)
                .required(true)
                .help("Tool name")))

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
            local: matches.get_flag("local"),
            prerelease: matches.get_flag("prerelease"),
            full: matches.get_flag("full"),
        },

        "install" if matches.contains_id("project") => {
            let names = get_names(matches);
            if names.len() > 1 {
                return Err!("A single tool name must be specified when project is specified");
            }

            Action::InstallFromSpec {
                name: names.into_iter().next(),
                spec: get_tool_spec(matches)?,
                force: matches.get_flag("force"),
            }
        },

        "install" | "upgrade" => {
            let mode = match command {
                "install" => Mode::Install {
                    force: matches.get_flag("force"),
                    recheck_spec: false,
                },
                "upgrade" => Mode::Upgrade {
                    prerelease: matches.get_flag("prerelease"),
                },
                _ => unreachable!(),
            };

            Action::Install {
                mode,
                names: get_names(matches),
            }
        },

        "uninstall" => Action::Uninstall {names: get_names(matches)},

        _ => unreachable!(),
    };

    Ok(CliArgs {log_level, config_path, custom_config, action})
}

fn get_names(matches: &ArgMatches) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    if let Some(args) = matches.get_many("name") {
        for name in args {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }

    names
}

fn get_tool_spec(matches: &ArgMatches) -> GenericResult<ToolSpec> {
    let changelog = matches.get_one("changelog").map(|url: &String| {
        Url::parse(url).map_err(|e| format!("Invalid changelog URL: {e}"))
    }).transpose()?;

    let release_matcher = matches.get_one("release_matcher").map(|pattern: &String| {
        Matcher::new(pattern).map_err(|e| format!("Invalid release matcher: {e}"))
    }).transpose()?;

    let binary_matcher = matches.get_one("binary_matcher").map(|pattern: &String| {
        Matcher::new(pattern).map_err(|e| format!("Invalid binary matcher: {e}"))
    }).transpose()?;

    Ok(ToolSpec {
        project: matches.get_one("project").cloned().unwrap(),
        prerelease: matches.get_flag("prerelease"),

        changelog,
        release_matcher,
        binary_matcher,
        version_source: matches.get_one("version_source").cloned(),

        path: matches.get_one("path").cloned(),
        post: matches.get_one("post").cloned(),
    })
}