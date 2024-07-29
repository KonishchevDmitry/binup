use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use easy_logging::GlobalContext;
use globset::GlobMatcher;
use itertools::Itertools;
use log::{debug, info, error};
use url::Url;

use crate::config::{Config, Tool};
use crate::core::{EmptyResult, GenericResult};
use crate::download;
use crate::github;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Install,
    Upgrade,
}

pub fn install(config: &Config, mode: Mode, names: Option<Vec<String>>) -> EmptyResult {
    let tools: Vec<(&String, &Tool)> = match names {
        Some(ref names) => {
            let mut selected = Vec::new();

            for name in names {
                let tool = config.tools.get(name).ok_or_else(|| format!(
                    "{name:?} tool is not specified in the configuration file"))?;
                selected.push((name, tool));
            }

            selected
        },
        None => config.tools.iter().collect(),
    };

    for (name, tool) in tools {
        let _logging_context = GlobalContext::new(name);
        let path = tool.path.as_ref().unwrap_or(&config.path);
        install_tool(&name, tool, mode, path).map_err(|e| format!(
            "{name}: {e}"))?;
    }

    Ok(())
}

fn install_tool(name: &str, tool: &Tool, mode: Mode, path: &Path) -> EmptyResult {
    let project = &tool.project;
    let install_path = path.join(name);

    let current_state = check_tool(&install_path)?;
    if current_state.is_some() && mode == Mode::Install {
        info!("{name} is already installed.");
        return Ok(());
    }

    let release = github::get_release(&tool.project).map_err(|e| format!(
        "Failed to get latest release info for {project}: {e}"))?;
    let release_version = &release.tag;

    debug!("The latest release is {release_version}:");
    for asset in &release.assets {
        debug!("* {}", asset.name)
    }

    let assets: Vec<_> = release.assets.iter()
        .filter(|asset| tool.release_matcher.is_match(&asset.name))
        .collect();

    let asset = match assets.len() {
        0 => if release.assets.is_empty() {
            return Err!("The latest release of {project} ({release_version}) has no assets");
        } else {
            return Err!(
                "The specified release matcher matches none of the following assets:{}",
                format_list(release.assets.iter().map(|asset| &asset.name)));
        },
        1 => *assets.first().unwrap(),
        _ => {
            return Err!(
                "The specified release matcher matches multiple assets:{}",
                format_list(assets.iter().map(|asset| &asset.name)));
        }
    };

    match mode {
        Mode::Install => info!("Installing {name}..."),
        Mode::Upgrade => {
            // FIXME(konishchev): Check version
            info!("Upgrading {name} to {release_version}...");
        },
    }

    let binary_matcher = match tool.binary_matcher {
        None => Matcher::Simple(PathBuf::from(name)),
        Some(ref glob) => Matcher::Glob(glob.clone()),
    };

    let mut installer = Installer::new(binary_matcher, &install_path);

    download::download(&asset.url, &asset.name, &mut installer).map_err(|e| format!(
        "Failed to download {}: {e}", asset.url))?;

    installer.finish(&asset.url)
}

enum Matcher {
    Simple(PathBuf),
    Glob(GlobMatcher),
}

struct Installer {
    matcher: Matcher,
    matches: Vec<PathBuf>,

    temp_path: Option<PathBuf>,
    path: PathBuf,
}

impl Installer {
    fn new(matcher: Matcher, path: &Path) -> Installer {
        Installer {
            matcher,
            matches: Vec::new(),
            temp_path: None,
            path: path.to_owned(),
        }
    }

    fn finish(mut self, url: &Url) -> EmptyResult {
        match self.matches.len() {
            0 => return Err!("The specified binary matcher matches none of release ({url}) files"),
            1 => {},
            _ => {
                return Err!(
                    "The specified binary matcher matches multiple release ({url}) files:{}",
                    format_list(self.matches.iter().map(|path| path.display())));
            }
        }

        let temp_path = self.temp_path.take().expect(
            "An attempt to finish non-successful installation");

        fs::rename(&temp_path, &self.path).map_err(|e| format!(
            "Unable to rename {temp_path:?} to {:?}: {e}", self.path))?;

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
        if !match self.matcher {
            Matcher::Simple(ref name) => path == name,
            Matcher::Glob(ref glob) => glob.is_match(path),
        } {
            return Ok(());
        }

        debug!("{path:?} matches binary matcher.");

        self.matches.push(path.to_owned());
        if self.matches.len() > 1 {
            return Ok(()); // We'll return error later when collect all matches
        }

        if mode & 0o100 == 0 {
            return Err!("{path:?} in the archive is not executable");
        }

        let file_name = self.path.file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("Got an unexpected install path: {:?}", self.path))?;
        let temp_path = self.path.with_file_name(format!(".{file_name}.{ext}", ext=env!("CARGO_PKG_NAME")));

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
        file.sync_all()?;

        Ok(())
    }
}

struct ToolState {
    #[allow(dead_code)] // FIXME(konishchev): Support
    modify_time: SystemTime,
    // FIXME(konishchev): Version
}

fn check_tool(path: &Path) -> GenericResult<Option<ToolState>> {
    debug!("Checking {path:?}...");

    let modify_time = match fs::metadata(path).and_then(|metadata| metadata.modified()) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(None);
        },
        Err(err) => {
            return Err!("Failed to stat {path:?}: {err}");
        },
    };

    Ok(Some(ToolState {modify_time}))
}

fn format_list<T: Display, I: Iterator<Item = T>>(mut iter: I) -> String {
    "\n* ".to_owned() + &iter.join("\n* ")
}