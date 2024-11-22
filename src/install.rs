use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::SystemTime;

use easy_logging::GlobalContext;
use log::{Level, debug, info, warn, error};
use semver::Version;
use url::Url;

use crate::config::Config;
use crate::core::{EmptyResult, GenericResult};
use crate::download;
use crate::github::{self, Github};
use crate::matcher::Matcher;
use crate::release::{self, Release};
use crate::tool::ToolSpec;
use crate::util;
use crate::version::{self, ReleaseVersion};

#[derive(Clone, Copy)]
pub enum Mode {
    Install {
        force: bool,
        recheck_spec: bool,
    },
    Upgrade,
}

pub fn install(config: &Config, mode: Mode, names: Vec<String>) -> GenericResult<ExitCode> {
    let tools: Vec<(&String, &ToolSpec)> = if names.is_empty() {
        config.tools.iter().collect()
    } else {
        let mut selected = Vec::new();

        for name in &names {
            let tool = config.tools.get(name).ok_or_else(|| format!(
                "{name:?} tool is not specified in the configuration file"))?;
            selected.push((name, tool));
        }

        selected
    };

    let github = Github::new(&config.github)?;

    for (name, spec) in tools {
        let _logging_context = GlobalContext::new_conditional(Level::Debug, name);

        if names.is_empty() {
            info!("Checking {name}...");
        }

        let install_path = config.get_tool_path(name, spec);
        install_tool(name, spec, &github, mode, &install_path).map_err(|e| format!(
            "{name}: {e}"))?;
    }

    Ok(ExitCode::SUCCESS)
}

pub fn install_spec(config: &mut Config, name: Option<String>, spec: ToolSpec, force: bool) -> GenericResult<ExitCode> {
    let name = match name {
        Some(name) => name,
        None => github::parse_project_name(&spec.project)?.name,
    };

    let mut update_config = true;

    if let Some(registered) = config.tools.get(&name) {
        if *registered == spec {
            update_config = false
        } else if !force && !util::confirm("The tool is already registered with different configuration. Override it?") {
            return Ok(ExitCode::FAILURE);
        }
    }

    let github = Github::new(&config.github)?;
    let install_path = config.get_tool_path(&name, &spec);
    let install_mode = Mode::Install {force, recheck_spec: update_config};

    if update_config {
        config.edit(
            |config, raw| config.update_tool(raw, &name, &spec),
            |_| install_tool(&name, &spec, &github, install_mode, &install_path),
        )?;
    } else {
        install_tool(&name, &spec, &github, install_mode, &install_path)?;
    }

    Ok(ExitCode::SUCCESS)
}

fn install_tool(name: &str, spec: &ToolSpec, github: &Github, mut mode: Mode, install_path: &Path) -> EmptyResult {
    let tool = crate::tool::check(&install_path)?;

    match (mode, tool.is_some()) {
        (Mode::Install{force: false, recheck_spec: false}, true) => {
            info!("{name} is already installed.");
            return Ok(());
        },
        (Mode::Upgrade, false) => {
            mode = Mode::Install{force: false, recheck_spec: false};
        }
        _ => {},
    }

    let release = github.get_release(&spec.project).map_err(|e| format!(
        "Failed to get latest release info for {}: {e}", spec.project))?;

    let release_version = &release.version;
    let changelog = spec.changelog.as_ref().unwrap_or(&release.project.changelog);

    debug!("The latest release is {}:", release.version);
    for asset in &release.assets {
        debug!("* {}", asset.name)
    }

    let asset = release.select_asset(name, spec.release_matcher.as_ref())?;
    let release_time: SystemTime = asset.time.into();
    let current_version = tool.as_ref().and_then(|_|
        version::get_binary_version(&install_path));

    match mode {
        Mode::Install {force, recheck_spec: _} => if tool.is_none() {
            info!("Installing {name}...");
        } else if force {
            match current_version {
                Some(current_version) => info!(
                    "Reinstalling {name}: {current_version} -> {release_version}{changelog}",
                    changelog=format_changelog(changelog, Some(&current_version), release_version),
                ),

                None => info!("Reinstalling {name}..."),
            }
        } else {
            info!("{name} is already installed.");
            return Ok(());
        },

        Mode::Upgrade => {
            if match (tool.as_ref(), current_version.as_ref(), &release_version) {
                (_, Some(current_version), ReleaseVersion::Version(latest_version)) => current_version >= latest_version,
                (Some(tool), _, _) if tool.modify_time == release_time => true,
                _ => false,
            } {
                info!("{name} is already up-to-date.");
                return Ok(());
            }

            match current_version {
                Some(current_version) => info!(
                    "Upgrading {name}: {current_version} -> {release_version}{changelog}",
                    changelog=format_changelog(changelog, Some(&current_version), release_version),
                ),

                None => info!(
                    "Upgrading {name} to {release_version}{changelog}",
                    changelog=format_changelog(changelog, None, release_version),
                ),
            }
        },
    }

    let mut installer = Installer::new(name, &release, spec.binary_matcher.clone(), &install_path, release_time);

    download::download(&asset.url, &asset.name, &mut installer).map_err(|e| format!(
        "Failed to download {}: {e}", asset.url))?;

    installer.finish(&asset.url)?;

    if let Some(script) = spec.post.as_ref() {
        run_post_script(script)?;
    }

    Ok(())
}

struct Installer {
    matcher: Matcher,
    automatic_matcher: bool,

    binaries: Vec<PathBuf>,
    matches: Vec<PathBuf>,
    temp_path: Option<PathBuf>,

    path: PathBuf,
    time: SystemTime,
}

impl Installer {
    fn new(name: &str, release: &Release, matcher: Option<Matcher>, path: &Path, time: SystemTime) -> Installer {
        let mut automatic_matcher = false;

        let matcher = matcher.unwrap_or_else(|| {
            automatic_matcher = true;
            release::generate_binary_matcher(name, release)
        });

        Installer {
            matcher,
            automatic_matcher,

            binaries: Vec::new(),
            matches: Vec::new(),

            temp_path: None,
            path: path.to_owned(),
            time,
        }
    }

    fn finish(mut self, url: &Url) -> EmptyResult {
        if self.automatic_matcher && self.matches.is_empty() && self.binaries.len() == 1 {
            debug!(concat!(
                "Automatic binary matcher found zero binaries, ",
                "but the release archive has only one executable, so using it."
            ));
        } else if self.matches.len() != 1 {
            if self.automatic_matcher {
                let message = format!("Unable to automatically choose the proper executable from release ({url}) binaries");

                if self.binaries.is_empty() {
                    return Err!("{message}: the release has no executable binaries")
                } else {
                    return Err!(
                        "{message}:{}\n\nBinary matcher should be specified.",
                        util::format_list(self.binaries.iter().map(|path| path.display())));
                }
            } else {
                if !self.matches.is_empty() {
                    return Err!(
                        "The specified binary matcher matches multiple release ({url}) files:{}",
                        util::format_list(self.matches.iter().map(|path| path.display())));
                }

                let message = format!("The specified binary matcher matches none of release ({url}) files");

                if self.binaries.is_empty() {
                    return Err!("{message}. The release has no executable binaries at all");
                } else {
                    return Err!(
                        "{message}. The release has the following executable binaries:{}",
                        util::format_list(self.binaries.iter().map(|path| path.display())));
                }
            }
        }

        let temp_path = self.temp_path.take().expect(
            "An attempt to finish non-successful installation");

        fs::rename(&temp_path, &self.path).map_err(|e| format!(
            "Unable to rename {temp_path:?} to {:?}: {e}", self.path))?;

        debug!("The tool is installed as {:?}.", self.path);

        Ok(())
    }
}

impl Drop for Installer {
    fn drop(&mut self) {
        if let Some(temp_path) = self.temp_path.take() {
            if let Err(err) = fs::remove_file(&temp_path) {
                error!("Unable to delete {temp_path:?}: {err}.");
            }
        }
    }
}

impl download::Installer for Installer {
    fn on_file(&mut self, path: &Path, mode: u32, data: &mut dyn Read) -> EmptyResult {
        let is_executable = mode & 0o100 != 0;

        if is_executable {
            self.binaries.push(path.to_owned());
        }

        if self.matcher.matches(path) {
            debug!("{path:?} matches binary matcher.");

            self.matches.push(path.to_owned());
            if self.matches.len() > 1 {
                return Ok(()); // We'll return error later when collect all matches
            }

            if !is_executable {
                return Err!("{path:?} in the archive is not executable");
            }
        } else if self.automatic_matcher && is_executable && self.temp_path.is_none() {
            debug!(concat!(
                "Got first executable in archive: {:?}. ",
                "Download it for the case if it's the only one executable in archive.",
            ), path);
        } else {
            return Ok(());
        }

        let temp_path = match self.temp_path.as_ref() {
            Some(path) => path.to_owned(),
            None => {
                let file_name = self.path.file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| format!("Got an unexpected install path: {:?}", self.path))?;

                self.path.with_file_name(format!(".{file_name}.{ext}", ext=env!("CARGO_PKG_NAME")))
            },
        };

        debug!("Downloading {path:?} to {temp_path:?}...");

        let mut file = OpenOptions::new()
            .create(true)
            .mode(0o755)
            .write(true)
            .truncate(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp_path)
            .map_err(|e| format!("Unable to create {temp_path:?}: {e}"))?;
        self.temp_path.replace(temp_path);

        io::copy(data, &mut file)?;
        file.set_modified(self.time)?;
        file.sync_all()?;

        Ok(())
    }
}

fn run_post_script(script: &str) -> EmptyResult {
    debug!("Executing post-install script:{}", util::format_multiline(script));

    let result = Command::new("bash").args(["-c", script]).output().map_err(|e| format!(
        "Failed to execute post-install script: unable to spawn bash process: {e}"))?;

    let stderr = String::from_utf8_lossy(&result.stderr);
    if !result.status.success() {
        return Err!(
            "Post-install script returned an error ({}):{}",
            result.status, util::format_multiline(&stderr));
    }

    if stderr.trim().is_empty() {
        debug!("Post-install script has finished.");
    } else {
        warn!("Post-install script:{}", util::format_multiline(&stderr));
    }

    Ok(())
}

fn format_changelog(changelog: &Url, from: Option<&Version>, to: &ReleaseVersion) -> String {
    match (from, to) {
        // We don't place ellipsis after changelog, because at least iTerm2 parses URL improperly in this case
        (Some(from), ReleaseVersion::Version(to)) if from == to => "...".to_owned(),
        _ => format!(" (see {changelog})")
    }
}