use std::env::consts;
#[cfg(test)] use std::ops::Deref;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use platforms::{Arch, OS};
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

pub fn generate_release_matchers(binary_name: &str, release: &Release) -> Vec<Matcher> {
    generate_release_matchers_inner(binary_name, &release.project.name, consts::OS, consts::ARCH).unwrap_or_default()
}

// XXX(konishchev): Rewrite
pub fn generate_release_matchers_inner(binary_name: &str, project_name: &str, os: &str, arch: &str) -> Option<Vec<Matcher>> {
    let os = OS::from_str(os).ok()?;
    let arch = Arch::from_str(arch).ok()?;

    let os_regex = match os {
        OS::Linux => "(?:linux|unknown-linux(?:-gnu)?)",
        OS::MacOS => "(?:apple-darwin|darwin|macos)",
        _ => return None,
    };

    let arch_regex = match arch {
        Arch::AArch64 => "(?:aarch64|arm64)",
        Arch::X86_64 => "(?:amd64|x64|x86_64)",
        _ => return None,
    };

    let basic_regex = Regex::new(&format!(r"[-_.](?:{os_regex}[-_.]{arch_regex}|{arch_regex}[-_.]{os_regex})(?:[-_.].+?)?\.tar\.[^.]+$")).unwrap();

    let project_regex = Regex::new(&format!("^{project}([-_][^-_]+?)?{basic}",
        project=regex::escape(project_name), basic=basic_regex)).unwrap();

    let binary_regex = Regex::new(&format!("^{binary}{basic}",
        binary=regex::escape(binary_name), basic=basic_regex)).unwrap();

    Some(vec![
        Matcher::Regex(binary_regex),
        Matcher::Regex(project_regex),
        Matcher::Regex(basic_regex),
    ])
}

pub fn generate_binary_matcher(binary_name: &str, release: &Release) -> Matcher {
    generate_binary_matcher_inner(binary_name, &release.project.name)
}

// XXX(konishchev): Rewrite
fn generate_binary_matcher_inner(binary_name: &str, project_name: &str) -> Matcher {
    Matcher::Regex(Regex::new(&format!(
        "(?:^|/)(?:{binary}|{project})$",
        binary=regex::escape(binary_name), project=regex::escape(project_name),
    )).unwrap())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use super::*;

    #[test]
    fn platform_support() {
        let os = consts::OS;
        let arch = consts::ARCH;

        assert!(
            generate_release_matchers_inner("", "", os, arch).is_some(),
            "Unsupported OS/architecture: {os}/{arch}",
        );
    }

    #[rstest(binary_name, project_name, assets, matches, matcher_index,
        case("binup", "binup", &[
            "binup-linux-x64-v1.1.0.tar.bz2",
            "binup-macos-arm64-v1.1.0.tar.bz2",
            "binup-macos-x64-v1.1.0.tar.bz2",
        ], &[
            (OS::Linux, Arch::X86_64, "binup-linux-x64-v1.1.0.tar.bz2"),
            (OS::MacOS, Arch::X86_64, "binup-macos-x64-v1.1.0.tar.bz2"),
            (OS::MacOS, Arch::AArch64, "binup-macos-arm64-v1.1.0.tar.bz2"),
        ], 0),

        case("dnscrypt-proxy", "dnscrypt-proxy", &[
            "dnscrypt-proxy-android_arm-2.1.5.zip",
            "dnscrypt-proxy-android_arm-2.1.5.zip.minisig",
            "dnscrypt-proxy-android_arm64-2.1.5.zip",
            "dnscrypt-proxy-android_arm64-2.1.5.zip.minisig",
            "dnscrypt-proxy-android_i386-2.1.5.zip",
            "dnscrypt-proxy-android_i386-2.1.5.zip.minisig",
            "dnscrypt-proxy-android_x86_64-2.1.5.zip",
            "dnscrypt-proxy-android_x86_64-2.1.5.zip.minisig",
            "dnscrypt-proxy-dragonflybsd_amd64-2.1.5.tar.gz",
            "dnscrypt-proxy-dragonflybsd_amd64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-freebsd_amd64-2.1.5.tar.gz",
            "dnscrypt-proxy-freebsd_amd64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-freebsd_arm-2.1.5.tar.gz",
            "dnscrypt-proxy-freebsd_arm-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-freebsd_i386-2.1.5.tar.gz",
            "dnscrypt-proxy-freebsd_i386-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_arm-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_arm-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_arm64-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_arm64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_i386-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_i386-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_mips-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_mips-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_mips64-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_mips64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_mips64le-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_mips64le-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_mipsle-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_mipsle-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_riscv64-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_riscv64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-linux_x86_64-2.1.5.tar.gz",
            "dnscrypt-proxy-linux_x86_64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-macos_arm64-2.1.5.zip",
            "dnscrypt-proxy-macos_arm64-2.1.5.zip.minisig",
            "dnscrypt-proxy-macos_x86_64-2.1.5.zip",
            "dnscrypt-proxy-macos_x86_64-2.1.5.zip.minisig",
            "dnscrypt-proxy-netbsd_amd64-2.1.5.tar.gz",
            "dnscrypt-proxy-netbsd_amd64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-netbsd_i386-2.1.5.tar.gz",
            "dnscrypt-proxy-netbsd_i386-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-openbsd_amd64-2.1.5.tar.gz",
            "dnscrypt-proxy-openbsd_amd64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-openbsd_i386-2.1.5.tar.gz",
            "dnscrypt-proxy-openbsd_i386-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-solaris_amd64-2.1.5.tar.gz",
            "dnscrypt-proxy-solaris_amd64-2.1.5.tar.gz.minisig",
            "dnscrypt-proxy-win32-2.1.5.zip",
            "dnscrypt-proxy-win32-2.1.5.zip.minisig",
            "dnscrypt-proxy-win64-2.1.5.zip",
            "dnscrypt-proxy-win64-2.1.5.zip.minisig",
        ], &[
            (OS::Linux, Arch::X86_64, "dnscrypt-proxy-linux_x86_64-2.1.5.tar.gz"),

            // FIXME(konishchev): Support it?
            // (OS::MacOS, Arch::X86_64, "dnscrypt-proxy-macos_x86_64-2.1.5.zip"),
            // (OS::MacOS, Arch::AArch64, "dnscrypt-proxy-macos_arm64-2.1.5.zip"),
        ], 0),

        case("ssservice", "shadowsocks-rust", &[
            "shadowsocks-v1.20.3.aarch64-apple-darwin.tar.xz",
            "shadowsocks-v1.20.3.aarch64-apple-darwin.tar.xz.sha256",
            "shadowsocks-v1.20.3.aarch64-unknown-linux-gnu.tar.xz",
            "shadowsocks-v1.20.3.aarch64-unknown-linux-gnu.tar.xz.sha256",
            "shadowsocks-v1.20.3.aarch64-unknown-linux-musl.tar.xz",
            "shadowsocks-v1.20.3.aarch64-unknown-linux-musl.tar.xz.sha256",
            "shadowsocks-v1.20.3.arm-unknown-linux-gnueabi.tar.xz",
            "shadowsocks-v1.20.3.arm-unknown-linux-gnueabi.tar.xz.sha256",
            "shadowsocks-v1.20.3.arm-unknown-linux-gnueabihf.tar.xz",
            "shadowsocks-v1.20.3.arm-unknown-linux-gnueabihf.tar.xz.sha256",
            "shadowsocks-v1.20.3.arm-unknown-linux-musleabi.tar.xz",
            "shadowsocks-v1.20.3.arm-unknown-linux-musleabi.tar.xz.sha256",
            "shadowsocks-v1.20.3.arm-unknown-linux-musleabihf.tar.xz",
            "shadowsocks-v1.20.3.arm-unknown-linux-musleabihf.tar.xz.sha256",
            "shadowsocks-v1.20.3.armv7-unknown-linux-gnueabihf.tar.xz",
            "shadowsocks-v1.20.3.armv7-unknown-linux-gnueabihf.tar.xz.sha256",
            "shadowsocks-v1.20.3.armv7-unknown-linux-musleabihf.tar.xz",
            "shadowsocks-v1.20.3.armv7-unknown-linux-musleabihf.tar.xz.sha256",
            "shadowsocks-v1.20.3.i686-unknown-linux-musl.tar.xz",
            "shadowsocks-v1.20.3.i686-unknown-linux-musl.tar.xz.sha256",
            "shadowsocks-v1.20.3.x86_64-apple-darwin.tar.xz",
            "shadowsocks-v1.20.3.x86_64-apple-darwin.tar.xz.sha256",
            "shadowsocks-v1.20.3.x86_64-pc-windows-gnu.zip",
            "shadowsocks-v1.20.3.x86_64-pc-windows-gnu.zip.sha256",
            "shadowsocks-v1.20.3.x86_64-pc-windows-msvc.zip",
            "shadowsocks-v1.20.3.x86_64-pc-windows-msvc.zip.sha256",
            "shadowsocks-v1.20.3.x86_64-unknown-linux-gnu.tar.xz",
            "shadowsocks-v1.20.3.x86_64-unknown-linux-gnu.tar.xz.sha256",
            "shadowsocks-v1.20.3.x86_64-unknown-linux-musl.tar.xz",
            "shadowsocks-v1.20.3.x86_64-unknown-linux-musl.tar.xz.sha256",
        ], &[
            // FIXME(konishchev): Support?
            // (OS::Linux, Arch::X86_64, "shadowsocks-v1.20.3.x86_64-unknown-linux-gnu.tar.xz"),
            (OS::MacOS, Arch::X86_64, "shadowsocks-v1.20.3.x86_64-apple-darwin.tar.xz"),
            (OS::MacOS, Arch::AArch64, "shadowsocks-v1.20.3.aarch64-apple-darwin.tar.xz"),
        ], 2),

        case("prometheus-node-exporter", "node_exporter", &[
            "node_exporter-1.8.2.darwin-amd64.tar.gz",
            "node_exporter-1.8.2.darwin-arm64.tar.gz",
            "node_exporter-1.8.2.linux-386.tar.gz",
            "node_exporter-1.8.2.linux-amd64.tar.gz",
            "node_exporter-1.8.2.linux-arm64.tar.gz",
            "node_exporter-1.8.2.linux-armv5.tar.gz",
            "node_exporter-1.8.2.linux-armv6.tar.gz",
            "node_exporter-1.8.2.linux-armv7.tar.gz",
            "node_exporter-1.8.2.linux-mips.tar.gz",
            "node_exporter-1.8.2.linux-mips64.tar.gz",
            "node_exporter-1.8.2.linux-mips64le.tar.gz",
            "node_exporter-1.8.2.linux-mipsle.tar.gz",
            "node_exporter-1.8.2.linux-ppc64.tar.gz",
            "node_exporter-1.8.2.linux-ppc64le.tar.gz",
            "node_exporter-1.8.2.linux-riscv64.tar.gz",
            "node_exporter-1.8.2.linux-s390x.tar.gz",
            "node_exporter-1.8.2.netbsd-386.tar.gz",
            "node_exporter-1.8.2.netbsd-amd64.tar.gz",
            "node_exporter-1.8.2.openbsd-amd64.tar.gz",
            "sha256sums.txt",
        ], &[
            (OS::Linux, Arch::X86_64, "node_exporter-1.8.2.linux-amd64.tar.gz"),
            (OS::MacOS, Arch::X86_64, "node_exporter-1.8.2.darwin-amd64.tar.gz"),
            (OS::MacOS, Arch::AArch64, "node_exporter-1.8.2.darwin-arm64.tar.gz"),
        ], 1),
    )]
    fn release_matcher(binary_name: &str, project_name: &str, assets: &[&str], matches: &[(OS, Arch, &str)], matcher_index: usize) {
        for (os, arch, expected) in matches {
            let matchers = generate_release_matchers_inner(binary_name, project_name, os.as_str(), arch.as_str()).unwrap();

            for (index, matcher) in matchers[..matcher_index].iter().enumerate() {
                println!("#{index}: {matcher}");

                let result: Vec<&str> = assets.iter()
                    .filter(|asset| matcher.matches(asset))
                    .map(Deref::deref).collect();

                assert_eq!(result, Vec::<&str>::new(), "{os}/{arch}");
            }

            let matcher = &matchers[matcher_index];
            println!("#{matcher_index}: {matcher}");

            let result: Vec<&str> = assets.iter()
                .filter(|asset| matcher.matches(asset))
                .map(Deref::deref).collect();

            assert_eq!(&result, &[expected as &str]);
        }
    }

    #[rstest(binary_name, project_name, file,
        case("tool", "tool", "tool"),
        case("binary", "project", "binary"),
        case("binary", "project", "directory/binary"),
        case("binary", "project", "directory/sub-directory/binary"),
        case("binary", "project", "project"),
        case("binary", "project", "directory/project"),
        case("binary", "project", "directory/sub-directory/project"),
    )]
    fn binary_matcher(binary_name: &str, project_name: &str, file: &str) {
        let matcher = generate_binary_matcher_inner(binary_name, project_name);
        assert!(matcher.matches(file), "{matcher} vs {file}");
    }
}