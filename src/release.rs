use std::env::consts;
#[cfg(test)] use std::ops::Deref;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use itertools::Itertools;
use platforms::{Arch, OS};
use regex::{self, Regex};
use url::Url;

use crate::core::GenericResult;
use crate::download::COMPRESSION_EXTENSION_REGEX;
use crate::matcher::Matcher;
use crate::project::Project;
use crate::util;
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

    pub fn select_asset(&self, binary_name: &str, matcher: Option<&Matcher>) -> GenericResult<&Asset> {
        if self.assets.is_empty() {
            return Err!("The latest release of {project} ({version}) has no assets",
                project=self.project.full_name(), version=self.version);
        }

        if let Some(matcher) = matcher {
            let assets: Vec<_> = self.assets.iter()
                .filter(|asset| matcher.matches(&asset.name))
                .collect();

            return Ok(match assets.len() {
                0 => {
                    return Err!(
                        "The specified release matcher matches none of the following assets:{}",
                        util::format_list(self.assets.iter().map(|asset| &asset.name)));
                },
                1 => assets[0],
                _ => {
                    return Err!(
                        "The specified release matcher matches multiple assets:{}",
                        util::format_list(assets.iter().map(|asset| &asset.name)));
                }
            });
        }

        let matchers = generate_release_matchers(binary_name, &self.project.name, consts::OS, consts::ARCH)
            .unwrap_or_default();

        for matcher in matchers {
            let assets: Vec<_> = self.assets.iter()
                .filter(|asset| matcher.matches(&asset.name))
                .collect();

            if assets.len() == 1 {
                return Ok(assets[0]);
            }
        }

        Err!(concat!(
            "Unable to automatically choose the proper release from the following assets:{}\n\n",
            "Release matcher should be specifed.",
        ), util::format_list(self.assets.iter().map(|asset| &asset.name)))
    }
}

pub struct Asset {
    pub name: String,
    pub time: DateTime<Utc>,
    pub url: Url,
}

fn generate_release_matchers(binary_name: &str, project_name: &str, os: &str, arch: &str) -> Option<Vec<Matcher>> {
    let os = OS::from_str(os).ok()?;
    let arch = Arch::from_str(arch).ok()?;

    let os_regex = match os {
        OS::Linux => "linux",
        OS::MacOS => "(?:apple-darwin|darwin|macos)",
        _ => return None,
    };

    let arch_regex = match arch {
        Arch::AArch64 => "(?:aarch64|arm64)",
        Arch::X86_64 => "(?:amd64|x64|x86_64)",
        _ => return None,
    };

    let separator_regex = "[-._]";
    let any_fields_regex = format!("(?:{separator_regex}[^/]+)?");

    let platform_regex = format!("(?:{os_regex}[-_]{arch_regex}|{arch_regex}[-_]{os_regex})");
    let optional_compression_extension_regex = format!(r"(?:{COMPRESSION_EXTENSION_REGEX})?");
    let archive_regex = format!(
        r"{separator_regex}{platform_regex}{any_fields_regex}\.tar\.[^/.]+$",
    );

    // Prioritized list of matchers
    let mut matchers = Vec::new();
    let name_regexes = [binary_name, project_name].into_iter().dedup().map(get_name_matcher).collect_vec();

    for name_regex in &name_regexes {
        // Archive with strict name and platform spec
        matchers.push(Regex::new(&format!(
            r"^{name_regex}{any_fields_regex}{archive_regex}")).unwrap());

        // Binary with strict name and platform spec
        matchers.push(Regex::new(&format!(
            r"^{name_regex}{separator_regex}{platform_regex}{optional_compression_extension_regex}$")).unwrap());
    }

    // Archive with strict platform spec and relaxed name spec
    matchers.push(Regex::new(&archive_regex).unwrap());

    for name_regex in &name_regexes {
        // Binary with strict name spec and relaxed platform spec (example: a Linux-only tool)
        matchers.push(Regex::new(&format!(
            r"^{name_regex}{separator_regex}{arch_regex}{optional_compression_extension_regex}$")).unwrap());

        // Binary with strict name spec and with no platform spec (example: a Linux-only tool for a single architecture)
        matchers.push(Regex::new(&format!(
            r"^{name_regex}{optional_compression_extension_regex}$")).unwrap());
    }

    Some(matchers.into_iter().map(Matcher::Regex).collect())
}

pub fn generate_binary_matcher(binary_name: &str, release: &Release) -> Matcher {
    generate_binary_matcher_inner(binary_name, &release.project.name)
}

fn generate_binary_matcher_inner(binary_name: &str, project_name: &str) -> Matcher {
    let binary_name_matcher = get_name_matcher(binary_name);
    let project_name_matcher = get_name_matcher(project_name);

    let matcher = if binary_name_matcher == project_name_matcher {
        binary_name_matcher
    } else {
        format!("(?:{binary_name_matcher}|{project_name_matcher})")
    };

    Matcher::Regex(Regex::new(&format!("(?:^|/){matcher}$")).unwrap())
}

fn get_name_matcher(name: &str) -> String {
    let hyphen_name = name.replace('_', "-");
    let underscore_name = hyphen_name.replace('-', "_");

    if hyphen_name == underscore_name {
        regex::escape(&hyphen_name)
    } else {
        format!("(?:{}|{})", regex::escape(&hyphen_name), regex::escape(&underscore_name))
    }
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
            generate_release_matchers("", "", os, arch).is_some(),
            "Unsupported OS/architecture: {os}/{arch}",
        );
    }

    #[rstest(binary_name, project_name, assets, matches, matcher_index,
        // https://github.com/KonishchevDmitry/binup
        case("binup", "binup", &[
            "binup-v1.1.0-linux-x64.tar.bz2",
            "binup-v1.1.0-macos-arm64.tar.bz2",
            "binup-v1.1.0-macos-x64.tar.bz2",
        ], &[
            (OS::Linux, Arch::X86_64, "binup-v1.1.0-linux-x64.tar.bz2"),
            (OS::MacOS, Arch::X86_64, "binup-v1.1.0-macos-x64.tar.bz2"),
            (OS::MacOS, Arch::AArch64, "binup-v1.1.0-macos-arm64.tar.bz2"),
        ], 0),

        // https://github.com/DNSCrypt/dnscrypt-proxy
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

            // TODO(konishchev): Support zip archives?
            // (OS::MacOS, Arch::X86_64, "dnscrypt-proxy-macos_x86_64-2.1.5.zip"),
            // (OS::MacOS, Arch::AArch64, "dnscrypt-proxy-macos_arm64-2.1.5.zip"),
        ], 0),

        // https://github.com/martin-helmich/prometheus-nginxlog-exporter
        case("prometheus-nginxlog-exporter", "prometheus-nginxlog-exporter", &[
            "checksums.txt",
            "prometheus-nginxlog-exporter_1.11.0_darwin_amd64.tar.gz",
            "prometheus-nginxlog-exporter_1.11.0_darwin_arm64.tar.gz",
            "prometheus-nginxlog-exporter_1.11.0_linux_amd64.deb",
            "prometheus-nginxlog-exporter_1.11.0_linux_amd64.rpm",
            "prometheus-nginxlog-exporter_1.11.0_linux_amd64.tar.gz",
            "prometheus-nginxlog-exporter_1.11.0_linux_arm64.deb",
            "prometheus-nginxlog-exporter_1.11.0_linux_arm64.rpm",
            "prometheus-nginxlog-exporter_1.11.0_linux_arm64.tar.gz",
        ], &[
            (OS::Linux, Arch::X86_64, "prometheus-nginxlog-exporter_1.11.0_linux_amd64.tar.gz"),
            (OS::MacOS, Arch::X86_64, "prometheus-nginxlog-exporter_1.11.0_darwin_amd64.tar.gz"),
            (OS::MacOS, Arch::AArch64, "prometheus-nginxlog-exporter_1.11.0_darwin_arm64.tar.gz"),
        ], 0),

        // https://github.com/prometheus/node_exporter
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
        ], 2),

        // https://github.com/shadowsocks/shadowsocks-rust
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
            // TODO(konishchev): Always automatically select glibc variant?
            // (OS::Linux, Arch::X86_64, "shadowsocks-v1.20.3.x86_64-unknown-linux-gnu.tar.xz"),
            (OS::MacOS, Arch::X86_64, "shadowsocks-v1.20.3.x86_64-apple-darwin.tar.xz"),
            (OS::MacOS, Arch::AArch64, "shadowsocks-v1.20.3.aarch64-apple-darwin.tar.xz"),
        ], 4),

        // https://github.com/tsl0922/ttyd/releases
        case("ttyd", "ttyd", &[
            "SHA256SUMS",
            "ttyd.aarch64",
            "ttyd.arm",
            "ttyd.armhf",
            "ttyd.i686",
            "ttyd.mips",
            "ttyd.mips64",
            "ttyd.mips64el",
            "ttyd.mipsel",
            "ttyd.s390x",
            "ttyd.win32.exe",
            "ttyd.x86_64",
        ], &[
            (OS::Linux, Arch::X86_64, "ttyd.x86_64"),
            (OS::MacOS, Arch::X86_64, "ttyd.x86_64"),
            (OS::MacOS, Arch::AArch64, "ttyd.aarch64"),
        ], 3),

        // https://github.com/KonishchevDmitry/binup/issues/2#issuecomment-3222682495
        case("rapidgzip", "indexed_bzip2", &["rapidgzip"], &[
            (OS::Linux, Arch::X86_64, "rapidgzip"),
            (OS::MacOS, Arch::X86_64, "rapidgzip"),
            (OS::MacOS, Arch::AArch64, "rapidgzip"),
        ], 6),
    )]
    fn release_matcher(binary_name: &str, project_name: &str, assets: &[&str], matches: &[(OS, Arch, &str)], matcher_index: usize) {
        for (os, arch, expected) in matches {
            let matchers = generate_release_matchers(binary_name, project_name, os.as_str(), arch.as_str()).unwrap();

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

        case("b-b-b", "p-p-p", "b-b-b"),
        case("b-b-b", "p-p-p", "b_b_b"),
        case("b_b_b", "p-p-p", "b-b-b"),

        case("b-b-b", "p-p-p", "p-p-p"),
        case("b-b-b", "p-p-p", "p_p_p"),
        case("b-b-b", "p_p_p", "p-p-p"),
    )]
    fn binary_matcher(binary_name: &str, project_name: &str, file: &str) {
        let matcher = generate_binary_matcher_inner(binary_name, project_name);
        assert!(matcher.matches(file), "{matcher} vs {file}");
    }
}