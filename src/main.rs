#[macro_use] mod core;

mod cli;
mod config;
mod download;
mod github;
mod install;
mod list;
mod matcher;
mod project;
mod release;
mod tool;
mod util;
mod version;

use std::io::{self, Write};
use std::path::Path;
use std::process;

use easy_logging::LoggingConfig;
use log::error;

use crate::cli::Action;
use crate::config::Config;
use crate::core::EmptyResult;

fn main() {
    let args = cli::parse_args().unwrap_or_else(|e| {
        let _ = writeln!(io::stderr(), "{}.", e);
        process::exit(1);
    });

    if let Err(err) = LoggingConfig::new(module_path!(), args.log_level).minimal().build() {
        let _ = writeln!(io::stderr(), "Failed to initialize the logging: {}.", err);
        process::exit(1);
    }

    if let Err(err) = run(&args.config_path, args.action) {
        let message = err.to_string();

        if message.contains('\n') || message.ends_with('.') {
            error!("{message}");
        } else {
            error!("{message}.");
        }

        process::exit(1);
    }
}

fn run(config_path: &Path, action: Action) -> EmptyResult {
    let config = Config::load(config_path).map_err(|e| format!(
        "Error while reading {:?} configuration file: {}", config_path, e))?;

    match action {
        Action::List {full} => crate::list::list(&config, full),
        Action::Install {mode, tools} => crate::install::install(&config, mode, tools),
    }
}