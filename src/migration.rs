use std::collections::{BTreeSet, HashSet};
use std::ffi::OsString;

use itertools::Itertools;
use log::{debug, warn};

use crate::config::{self, Config};
use crate::util;

// v2.0.0:
//
// Attention:
//
// This version introduces a breaking change: if binup is run from root user, /usr/local/bin is used as the default
// installation path instead of /root/.local/bin in previous versions. There are no changes for unprivileged users –
// ~/.local/bin is used as before.
//
// Since binup is fully stateless, existing configurations can't be reliably migrated automatically, but it will try to
// detect the legacy configuration (no installation path overrides in the configuration file + installed binaries in
// /root/.local/bin) and will display a warning in this case.
pub fn check_for_legacy_configuration(config: &Config) {
    if config.path.is_some() || !util::is_root_user() {
        return;
    }

    let tools: HashSet<OsString> = config.tools.iter().filter_map(|(name, spec)| {
        if spec.path.is_some() {
            None
        } else {
            Some(OsString::from(name))
        }
    }).collect();

    if tools.is_empty() {
        return;
    }

    let legacy_install_path = config::default_unprivileged_install_path();
    let Ok(entries) = std::fs::read_dir(&legacy_install_path) else {
        return;
    };

    let installed: BTreeSet<OsString> = entries.filter_map(|entry| {
        let entry = entry.ok()?;
        if !entry.metadata().ok()?.is_file() {
            return None;
        }

        let name = entry.file_name();
        if !tools.contains(&name) {
            return None;
        }

        Some(name)
    }).collect();

    if installed.is_empty() {
        return;
    }

    debug!("Found the following tools in the legacy install path {}: {}.", legacy_install_path.display(),
        installed.iter().map(|name| name.to_string_lossy()).join(", "));

    if (installed.len() as f64 / tools.len() as f64) < 0.5 {
        return;
    }

    warn!(concat!(
        "A possible legacy configuration is detected: {} will be used as the default installation path instead of {} ",
        "which was used in the previous versions of binup. See {} for details.\n"
    ), config::default_privileged_install_path().display(), legacy_install_path.display(),
        "https://github.com/KonishchevDmitry/binup/releases/tag/v2.0.0",
    );
}