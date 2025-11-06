use std::fmt::{self, Display, Formatter};
use std::path::Path;
use std::process::Command;

use clap::builder::PossibleValue;
use log::debug;
use semver::Version;
use serde::Deserialize;
use strum::VariantArray;
use strum_macros::{VariantArray, IntoStaticStr};

use crate::util;

#[cfg_attr(test, derive(Debug))]
pub enum ReleaseVersion {
    Version(Version),
    Tag(String)
}

impl ReleaseVersion {
    pub fn new(tag: &str) -> ReleaseVersion {
        if let Some(revision) = parse_revision(tag) {
            return ReleaseVersion::Version(revision)
        }

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

#[derive(VariantArray, IntoStaticStr, Deserialize, PartialEq, Default, Clone, Copy)]
#[serde(rename_all="kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum VersionSource {
    #[default]
    Flag,   // binary --version
    Command // binary version
}

impl clap::ValueEnum for VersionSource {
    fn value_variants<'a>() -> &'a [Self] {
        VersionSource::VARIANTS
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(Into::<&str>::into(self)))
    }
}

pub fn get_binary_version(path: &Path, method: VersionSource) -> Option<Version> {
    let mut command = Command::new(path);

    command.arg(match method {
        VersionSource::Flag => "--version",
        VersionSource::Command => "version",
    });

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

// TODO(konishchev): We probably should have our own version type which supports (and distinguishes) both semver and
// revision versions. Waiting for more real cases here.
fn parse_revision(string: &str) -> Option<Version> {
    let revision = string.strip_prefix('r')?.parse::<u64>().ok()?;
    Some(Version::new(revision, 0, 0))
}

fn parse_binary_version(stdout: &str) -> Option<Version> {
    if let Some(revision) = parse_revision(stdout.trim()) {
        return Some(revision);
    }

    for word in stdout.split('\n').next().unwrap().split(' ') {
        for token in word.split('-') {
            let token = token.strip_prefix('v').unwrap_or(token);
            if let Ok(version) = Version::parse(token) {
                return Some(version);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use indoc::indoc;
    use rstest::rstest;
    use super::*;

    #[rstest(tag, version,
        case("v1.7.2",  "1.7.2"),
        case("r38",    "38.0.0"), // https://github.com/gokcehan/lf
    )]
    fn release_version_parsing(tag: &str, version: &str) {
        let expected = Version::parse(version).unwrap();
        assert_matches!(
            ReleaseVersion::new(tag),
            ReleaseVersion::Version(version) if version == expected
        );
    }

    #[rstest(stdout, version,
        case(indoc!(r#"
            binup 0.3.0
        "#), "0.3.0"),

        // https://github.com/gokcehan/lf
        case(indoc!(r#"
            r38
        "#), "38.0.0"),

        case(indoc!(r#"
            victoria-metrics-20240425-145433-tags-v1.101.0-0-g5334f0c2c
        "#), "1.101.0"),

        case(indoc!(r#"
            vmctl version vmctl-20240425-145537-tags-v1.101.0-0-g5334f0c2c
        "#), "1.101.0"),

        case(indoc!(r#"
            hugo v0.145.0-666444f0a52132f9fec9f71cf25b441cc6a4f355 darwin/arm64 BuildDate=2025-02-26T15:41:25Z VendorInfo=gohugoio
        "#), "0.145.0"),

        case(indoc!(r#"
            prometheus, version 2.51.2 (branch: HEAD, revision: b4c0ab52c3e9b940ab803581ddae9b3d9a452337)
              build user:       root@b63f02a423d9
              build date:       20240410-14:05:54
              go version:       go1.22.2
              platform:         linux/amd64
              tags:             netgo,builtinassets,stringlabels
        "#), "2.51.2")
    )]
    fn binary_version_parsing(stdout: &str, version: &str) {
        let version = Version::parse(version).unwrap();

        assert_eq!(stdout, stdout.trim_start());
        assert_ne!(stdout, stdout.trim_end());

        assert_eq!(parse_binary_version(stdout), Some(version.clone()));
        assert_eq!(parse_binary_version(stdout.trim_end()), Some(version));
    }
}