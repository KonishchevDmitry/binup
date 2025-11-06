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
use crate::github::{self, Github};
use crate::tool::ToolSpec;
use crate::version::{self, ReleaseVersion};

pub fn list(config: &Config, local: bool, prerelease: bool, full: bool) -> GenericResult<ExitCode> {
    if config.tools.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    let mut rows = Vec::new();
    let github = (!local).then(|| Github::new(&config.github)).transpose()?;
    let colored = std::io::stdout().is_terminal();

    for (name, spec) in &config.tools {
        debug!("Checking {name}...");

        let mut spec = spec.clone();
        spec.prerelease |= prerelease;

        let install_path = config.get_tool_path(name, &spec);
        rows.push(list_tool(name, &spec, github.as_ref(), &install_path, colored));
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    table.modify(Rows::first(), Height::increase(2));
    if colored {
        table.modify(Rows::first(), tabled::settings::Color::BOLD);
    }

    table.modify(Columns::one(1), Alignment::center()); // status
    table.modify(Columns::one(2), Alignment::right()); // version

    // changelog
    if !full {
        table.with(Remove::column(Columns::one(4)));
    }

    // latest version
    if local {
        table.with(Remove::column(Columns::one(3)));
    } else {
        table.modify(Columns::one(3), Alignment::right());
    }

    let _ = writeln!(std::io::stdout(), "{}", table);
    Ok(ExitCode::SUCCESS)
}

#[derive(Tabled)]
struct ToolInfo {
    #[tabled(rename = "Name")]
    name: String,

    #[tabled(rename = "Status")]
    status: String,

    #[tabled(rename = "Version")]
    version: String,

    #[tabled(rename = "Latest")]
    latest: String,

    #[tabled(rename = "Changelog")]
    changelog: String,
}

fn list_tool(name: &str, spec: &ToolSpec, github: Option<&Github>, install_path: &Path, colored: bool) -> ToolInfo {
    let (status, tool) = crate::tool::check(install_path).map(|tool| (
        if tool.is_some() { "installed" } else { "not installed" }, tool
    )).unwrap_or_else(|e| {
        error!("{name}: {e}.");
        ("unknown", None)
    });

    let installed_version = tool.as_ref().and_then(|_|
        version::get_binary_version(install_path, spec.version_source.unwrap_or_default()));

    let project = github::parse_project_name(&spec.project).inspect_err(|e| {
        error!("{name}: {}: {e}.", spec.project);
    }).ok();

    let mut info = ToolInfo {
        name: name.to_owned(),
        status: status.to_owned(),
        version: installed_version.as_ref().map(|version| version.to_string()).unwrap_or_default(),
        latest: String::new(),
        changelog: spec.changelog.as_ref()
            .or_else(|| project.as_ref().map(|project| &project.changelog))
            .map(ToString::to_string)
            .unwrap_or_default(),
    };

    let (Some(github), Some(_project)) = (github, project) else {
        return info;
    };

    let release = match github.get_release(&spec.project, spec.prerelease) {
        Ok(Some(release)) => release,
        Ok(None) => return info,
        Err(err) => {
            error!("{name}: Failed to get latest release info for {}: {err}.", spec.project);
            return info;
        }
    };
    info.latest = release.version.to_string();

    let release_time: Option<SystemTime> = match release.select_asset(name, spec.release_matcher.as_ref()) {
        Ok(asset) => Some(asset.time.into()),
        Err(_) => {
            if colored {
                info.latest = Color::Yellow.paint(info.latest).to_string();
            }
            None
        },
    };

    let up_to_date = if let (Some(current), ReleaseVersion::Version(latest)) = (installed_version, release.version) {
        Some(current >= latest)
    } else if let (Some(tool), Some(release_time)) = (tool, release_time) {
        Some(tool.modify_time >= release_time)
    } else {
        None
    };

    if let Some(up_to_date) = up_to_date {
        let (status, color) = if up_to_date {
            ("up to date", Color::Green)
        } else {
            ("outdated", Color::Yellow)
        };

        info.status = status.to_owned();
        if colored {
            info.status = color.paint(info.status).to_string();
        }
    }

    info
}
