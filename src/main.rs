#[macro_use] mod core;

mod cli;
mod config;

use std::io::{self, Write};
use std::path::Path;
use std::process;

use log::error;

use crate::config::Config;
use crate::core::EmptyResult;

fn main() {
    let args = cli::parse_args().unwrap_or_else(|e| {
        let _ = writeln!(io::stderr(), "{}.", e);
        process::exit(1);
    });

    if let Err(e) = easy_logging::init(module_path!().split("::").next().unwrap(), args.log_level) {
        let _ = writeln!(io::stderr(), "Failed to initialize the logging: {}.", e);
        process::exit(1);
    }

    if let Err(e) = run(&args.config_path) {
        error!("{}.", e);
        process::exit(1);
    }
}

fn run(config_path: &Path) -> EmptyResult {
    Config::load(config_path).map_err(|e| format!(
        "Error while reading {:?} configuration file: {}", config_path, e))?;

    Ok(())
}