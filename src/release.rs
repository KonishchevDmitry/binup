use std::env::consts;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use platforms::{OS, Arch};
use regex::{self, Regex};
use url::Url;

use crate::matcher::Matcher;
use crate::project::Project;
use crate::version::ReleaseVersion;

pub struct Release {
    pub project: Project,
    pub version: ReleaseVersion,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn new(project: Project, tag: &str, assets: Vec<Asset>) -> Release {
        Release {
            project,
            version: ReleaseVersion::new(tag),
            assets,
        }
    }
}

pub struct Asset {
    pub name: String,
    pub time: DateTime<Utc>,
    pub url: Url,
}

pub fn generate_release_matchers(release: &Release) -> Vec<Matcher> {
    // XXX(konishchev): HERE
    Vec::new()
}

pub fn generate_release_matchers_inner(project_name: &str, binary_name: &str, os: &str, arch: &str) -> Option<Vec<Matcher>> {
    let os = OS::from_str(os).ok()?;
    let arch = Arch::from_str(arch).ok()?;

    let os_regex = match os {
        OS::Linux => "linux",
        OS::MacOS => "macos",
        _ => return None,
    };

    let arch_regex = match arch {
        Arch::AArch64 => "arm64",
        Arch::X86_64 => "x64",
        _ => return None,
    };

    let basic_regex = Regex::new(&format!("{os_regex}-{arch_regex}")).unwrap();

    Some(vec![
        Matcher::Regex(basic_regex),
    ])
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use rstest::rstest;
    use super::*;

    #[test]
    fn support() {
        let os = consts::OS;
        let arch = consts::ARCH;

        assert!(
            generate_release_matchers_inner("", "", os, arch).is_some(),
            "Unsupported OS/architecture: {os}/{arch}",
        );
    }

    #[rstest(project, binary, assets, matches, matcher_index,
        case("binup", "binup", &[
            "binup-linux-x64-v1.1.0.tar.bz2",
            "binup-macos-arm64-v1.1.0.tar.bz2",
            "binup-macos-x64-v1.1.0.tar.bz2",
        ], &[
            (OS::Linux, Arch::X86_64, "binup-linux-x64-v1.1.0.tar.bz2"),
            (OS::MacOS, Arch::X86_64, "binup-macos-x64-v1.1.0.tar.bz2"),
            (OS::MacOS, Arch::AArch64, "binup-macos-arm64-v1.1.0.tar.bz2"),
        ], 0),
    )]
    fn matcher(project: &str, binary: &str, assets: &[&str], matches: &[(OS, Arch, &str)], matcher_index: usize) {
        for (os, arch, expected) in matches {
            let matchers = generate_release_matchers_inner(project, binary, os.as_str(), arch.as_str()).unwrap();

            for matcher in &matchers[..matcher_index] {
                let result: Vec<&str> = assets.iter()
                    .filter(|asset| matcher.matches(asset))
                    .map(Deref::deref).collect();
                assert_eq!(result, Vec::<&str>::new(), "{os}/{arch}");
            }

            let matcher = &matchers[matcher_index];
            let result: Vec<&str> = assets.iter()
                .filter(|asset| matcher.matches(asset))
                .map(Deref::deref).collect();

            assert_eq!(&result, &[expected as &str], "{os}/{arch}");
        }
    }
}