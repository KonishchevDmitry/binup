use std::fmt::{self, Display, Formatter};
use std::path::Path;
use std::process::Command;

use log::debug;
use semver::Version;

use crate::util;

pub enum ReleaseVersion {
    Version(Version),
    Tag(String)
}

impl ReleaseVersion {
    pub fn new(tag: &str) -> ReleaseVersion {
        let mut version = tag;
        if version.starts_with('v') {
            version = &version[1..];
        }

        match Version::parse(version) {
            Ok(version) => ReleaseVersion::Version(version),
            Err(_) => ReleaseVersion::Tag(tag.to_owned()),
        }
    }
}

impl Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match self {
            ReleaseVersion::Version(version) => version.fmt(formatter),
            ReleaseVersion::Tag(tag) => tag.fmt(formatter),
        }
    }
}

pub fn get_binary_version(path: &Path) -> Option<Version> {
    let mut command = Command::new(path);
    command.arg("--version");

    debug!("Trying to determine {path:?} version by spawning `{command:?}`...");

    match command.output() {
        Ok(result) => if result.status.success() {
            debug!("Got the following output:{}", util::format_multiline(&String::from_utf8_lossy(&result.stdout)));

            match String::from_utf8(result.stdout).ok().and_then(|stdout| parse_binary_version(&stdout)) {
                Some(version) => {
                    debug!("Got the following version: {}.", version);
                    Some(version)
                },
                None => {
                    debug!("Failed to found the version in the program output.");
                    None
                },
            }
        } else {
            debug!(
                "The program returned an error ({}):{}", result.status,
                util::format_multiline(&String::from_utf8_lossy(&result.stderr)));
            None
        },
        Err(err) => {
            debug!("Failed to spawn `{command:?}`: {err}.");
            None
        },
    }
}

fn parse_binary_version(stdout: &str) -> Option<Version> {
    for word in stdout.split('\n').next().unwrap().split(' ') {
        if let Ok(version) = Version::parse(word) {
            return Some(version);
        }
    }
    None
}

// FIXME(konishchev): vmctl version vmctl-20240425-145537-tags-v1.101.0-0-g5334f0c2c
// FIXME(konishchev): victoria-metrics-20240425-145433-tags-v1.101.0-0-g5334f0c2c
#[cfg(test)]
mod tests {
    use indoc::indoc;
    use rstest::rstest;
    use super::*;

    #[rstest(stdout, version,
        case(indoc!(r#"
            binup 0.3.0
        "#), "0.3.0"),

        case(indoc!(r#"
            prometheus, version 2.51.2 (branch: HEAD, revision: b4c0ab52c3e9b940ab803581ddae9b3d9a452337)
              build user:       root@b63f02a423d9
              build date:       20240410-14:05:54
              go version:       go1.22.2
              platform:         linux/amd64
              tags:             netgo,builtinassets,stringlabels
        "#), "2.51.2")
    )]
    fn parse(stdout: &str, version: &str) {
        let version = Version::parse(version).unwrap();

        assert_eq!(stdout, stdout.trim_start());
        assert_ne!(stdout, stdout.trim_end());

        assert_eq!(parse_binary_version(stdout), Some(version.clone()));
        assert_eq!(parse_binary_version(stdout.trim_end()), Some(version));
    }
}