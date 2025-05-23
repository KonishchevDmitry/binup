use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::time::SystemTime;

use ansi_term::Color;
use is_terminal::IsTerminal;
use log::{debug, error};
use tabled::{Table, Tabled};
use tabled::settings::{Alignment, Height, Remove, object::{Rows, Columns}, style::Style};

use crate::config::Config;
use crate::core::GenericResult;
use crate::github::Github;
use crate::tool::ToolSpec;
use crate::version::{self, ReleaseVersion};

pub fn list(config: &Config, full: bool) -> GenericResult<ExitCode> {
    if config.tools.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    let mut rows = Vec::new();
    let github = Github::new(&config.github)?;
    let colored = std::io::stdout().is_terminal();

    for (name, spec) in &config.tools {
        debug!("Checking {name}...");
        let install_path = config.get_tool_path(name, spec);
        rows.push(list_tool(name, spec, &github, &install_path, colored));
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    table.modify(Rows::first(), Height::increase(2));
    if colored {
        table.modify(Rows::first(), tabled::settings::Color::BOLD);
    }
    table.modify(Columns::new(1..=2), Alignment::center());
    if !full {
        table.with(Remove::column(Columns::single(3)));
    }

    let _ = writeln!(std::io::stdout(), "{}", table);
    Ok(ExitCode::SUCCESS)
}

#[derive(Tabled)]
struct ToolInfo {
    #[tabled(rename = "Name")]
    name: String,

    #[tabled(rename = "Installed")]
    installed: String,

    #[tabled(rename = "Latest")]
    latest: String,

    #[tabled(rename = "Changelog")]
    changelog: String,
}

fn list_tool(name: &str, spec: &ToolSpec, github: &Github, install_path: &Path, colored: bool) -> ToolInfo {
    let tool = crate::tool::check(install_path).unwrap_or_else(|e| {
        error!("{name}: {e}.");
        None
    });

    let installed_version = tool.as_ref().and_then(|_|
        version::get_binary_version(install_path, spec.version_source.unwrap_or_default()));

    let mut info = ToolInfo {
        name: name.to_owned(),
        installed: installed_version.as_ref().map(|version| version.to_string()).unwrap_or_default(),
        latest: String::new(),
        changelog: spec.changelog.as_ref().map(ToString::to_string).unwrap_or_default(),
    };

    let release = match github.get_release(&spec.project) {
        Ok(release) => release,
        Err(err) => {
            error!("{name}: Failed to get latest release info for {}: {err}.", spec.project);
            return info;
        }
    };

    info.latest = release.version.to_string();
    if info.changelog.is_empty() {
        info.changelog = release.project.changelog.to_string();
    }

    if colored {
        let release_time: Option<SystemTime> = match release.select_asset(name, spec.release_matcher.as_ref()) {
            Ok(asset) => Some(asset.time.into()),
            Err(_) => {
                info.latest = Color::Yellow.paint(info.latest).to_string();
                None
            },
        };

        if let (Some(current), ReleaseVersion::Version(latest)) = (installed_version, release.version) {
            let color = if current >= latest {
                Color::Green
            } else {
                Color::Yellow
            };
            info.installed = color.paint(info.installed).to_string();
        } else if let (Some(tool), Some(release_time)) = (tool, release_time) {
            let color = if tool.modify_time >= release_time {
                Color::Green
            } else {
                Color::Yellow
            };
            info.installed = color.paint(info.installed).to_string();
        }
    }

    info
}
