use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::ExitCode;

use itertools::Itertools;
use log::{info, error};

use crate::config::Config;
use crate::core::GenericResult;
use crate::util;

pub fn uninstall(config: &mut Config, names: Vec<String>) -> GenericResult<ExitCode> {
    let mut tools = Vec::new();
    let mut invalid = Vec::new();

    for name in &names {
        match config.tools.get(name) {
            Some(spec) => tools.push((name, config.get_tool_path(name, spec))),
            None => invalid.push(name),
        }
    }

    if !invalid.is_empty() {
        return Err!("The following tools aren't specified in the configuration file: {}", invalid.iter().join(", "));
    } else if !util::confirm("Are you sure want to uninstall the specified tools?") {
        return Ok(ExitCode::FAILURE);
    }

    let mut exit_code = ExitCode::SUCCESS;

    for (name, path) in tools {
        match config.edit(
            |config, raw| config.remove_tool(raw, name),
            |_| uninstall_tool(&path),
        ) {
            Ok(deleted) => if deleted {
                info!("{name} ({}) is uninstalled.", path.display());
            } else {
                info!("{name} is uninstalled.");
            },
            Err(err) => {
                error!("Failed to uninstall {name}: {err}.");
                exit_code = ExitCode::FAILURE;
            }
        }
    }

    Ok(exit_code)
}

fn uninstall_tool(path: &Path) -> GenericResult<bool> {
    Ok(match fs::remove_file(path) {
        Ok(()) => true,
        Err(err) if err.kind() == ErrorKind::NotFound => false,
        Err(err) => return Err!("Unable to delete {path:?}: {err}"),
    })
}