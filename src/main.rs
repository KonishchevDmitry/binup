#[macro_use] mod core;

mod cli;
mod config;
mod download;
mod file_types;
mod github;
mod install;
mod list;
mod matcher;
mod project;
mod release;
mod tool;
mod uninstall;
mod util;
mod version;

use core::GenericResult;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use easy_logging::LoggingConfig;
use log::error;

use crate::cli::Action;
use crate::config::Config;

fn main() -> ExitCode {
    let args = match cli::parse_args() {
        Ok(args) => args,
        Err(err) => {
            let _ = writeln!(io::stderr(), "{}.", err);
            return ExitCode::FAILURE;
        },
    };

    if let Err(err) = LoggingConfig::new(module_path!(), args.log_level).minimal().build() {
        let _ = writeln!(io::stderr(), "Failed to initialize the logging: {}.", err);
        return ExitCode::FAILURE;
    }

    match run(&args.config_path, args.custom_config, args.action) {
        Ok(code) => code,
        Err(err) => {
            let message = err.to_string();

            if message.contains('\n') || message.ends_with('.') {
                error!("{message}");
            } else {
                error!("{message}.");
            }

            ExitCode::FAILURE
        },
    }
}

fn run(config_path: &Path, custom_config: bool, action: Action) -> GenericResult<ExitCode> {
    let mut config = Config::load(config_path, custom_config).map_err(|e| format!(
        "Error while reading {:?} configuration file: {}", config_path, e))?;

    match action {
        Action::List {local, prerelease, full} => list::list(&config, local, prerelease, full),
        Action::Install {mode, names} => install::install(&config, mode, names),
        Action::InstallFromSpec {name, spec, force} => install::install_spec(&mut config, name, spec, force),
        Action::Uninstall {names} => uninstall::uninstall(&mut config, names),
    }
}