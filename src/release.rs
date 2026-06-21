use std::borrow::Borrow;
use std::env::consts;
use std::fmt::Write;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use itertools::Itertools;
use log::debug;
use platforms::{Arch, OS};
use regex::{self, Regex};
use url::Url;

use crate::core::GenericResult;
use crate::download::{ARCHIVE_EXTENSION_REGEX, COMPRESSION_EXTENSION_REGEX};
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

        if let Some(matchers) = generate_release_matchers(binary_name, &self.project.name, consts::OS, consts::ARCH) {
            if let Some((_matcher, asset)) = match_assets(&self.assets, &matchers, consts::OS) {
                return Ok(asset);
            }
        }

        Err!(concat!(
            "Unable to automatically choose the proper release from the following assets:{}\n\n",
            "Release matcher should be specified.",
        ), format_assets(&self.assets))
    }
}

pub struct Asset {
    pub name: String,
    pub time: DateTime<Utc>,
    pub url: Url,
}

fn format_assets<A: Borrow<Asset>>(assets: &[A]) -> String {
    util::format_list(assets.iter().map(|asset| {
        &asset.borrow().name
    }))
}

const SEPARATOR_REGEX: &str = "[-._]";

fn generate_release_matchers(binary_name: &str, project_name: &str, os: &str, arch: &str) -> Option<Vec<Matcher>> {
    let os = OS::from_str(os).ok()?;
    let arch = Arch::from_str(arch).ok()?;

    // vendor + os
    let os_regex = match os {
        OS::Linux => "(?:unknown-)?linux",
        OS::MacOS => "(?:apple-)?(?:darwin|macos)",
        _ => return None,
    };

    let arch_regex = match arch {
        Arch::AArch64 => "(?:aarch64|arm64)",
        Arch::X86_64 => "(?:amd64|x64|x86_64)",
        _ => return None,
    };

    let any_fields_regex = format!("(?:{SEPARATOR_REGEX}[^/]+)?");
    let platform_regex = format!("(?:{os_regex}[-_]{arch_regex}|{arch_regex}[-_]{os_regex})");
    let optional_compression_extension_regex = format!(r"(?:{COMPRESSION_EXTENSION_REGEX})?");
    let archive_regex = format!(r"{SEPARATOR_REGEX}{platform_regex}{any_fields_regex}{ARCHIVE_EXTENSION_REGEX}$");

    // Prioritized list of matchers
    let mut matchers = Vec::new();
    let name_regexes = [binary_name, project_name].into_iter().dedup().map(get_name_matcher).collect_vec();

    for name_regex in &name_regexes {
        // Archive with strict name and platform spec
        matchers.push(Regex::new(&format!(
            r"^{name_regex}{any_fields_regex}{archive_regex}")).unwrap());

        // Binary with strict name and platform spec
        matchers.push(Regex::new(&format!(
            r"^{name_regex}{SEPARATOR_REGEX}{platform_regex}{optional_compression_extension_regex}$")).unwrap());
    }

    // Archive with strict platform spec and relaxed name spec
    matchers.push(Regex::new(&archive_regex).unwrap());

    for name_regex in &name_regexes {
        // Binary with strict name spec and relaxed platform spec (example: a Linux-only tool)
        matchers.push(Regex::new(&format!(
            r"^{name_regex}{SEPARATOR_REGEX}{arch_regex}{optional_compression_extension_regex}$")).unwrap());

        // Binary with strict name spec and with no platform spec (example: a Linux-only tool for a single architecture)
        matchers.push(Regex::new(&format!(
            r"^{name_regex}{optional_compression_extension_regex}$")).unwrap());
    }

    Some(matchers.into_iter().map(Matcher::Regex).collect())
}

fn match_assets<'a>(assets: &'a [Asset], matchers: &[Matcher], os: &str) -> Option<(usize, &'a Asset)> {
    for (index, matcher) in matchers.iter().enumerate() {
        let mut assets: Vec<_> = assets.into_iter()
            .filter(|asset| matcher.matches(&asset.name))
            .collect();

        // Linux tools are often compiled for multiple ABI and we want to have a sane default to automatically choose
        // between them. GNU libc looks as a good default here (https://konishchev.ru/posts/glibc-static-linking/).
        //
        // We do it as a separate hack to not overcomplicate matching logic.
        if assets.len() > 1 && OS::from_str(os) == Ok(OS::Linux) {
            if let Some(asset) = fold_assets_by_abi(&assets, &["linux-gnu", "linux-musl"]) {
                assets.truncate(0);
                assets.push(asset);
            }
        }

        if assets.is_empty() {
            debug!("No assets match `{matcher}` automatic release matcher.");
        } else {
            debug!("Matched assets for `{matcher}` automatic release matcher:{}", format_assets(&assets));
        }

        if assets.len() == 1 {
            return Some((index, assets[0]));
        }
    }

    None
}

// An asset is selected only when ABI is the only difference with other assets
fn fold_assets_by_abi<'a>(assets: &[&'a Asset], prioritized_abi: &[&str]) -> Option<&'a Asset> {
    let abi_regex = {
        let mut abi_regex = format!(r"{SEPARATOR_REGEX}(?<abi>");

        for (index, abi) in prioritized_abi.into_iter().enumerate() {
            if index != 0 {
                abi_regex.push('|');
            }
            abi_regex.push_str(&regex::escape(abi));
        }

        write!(&mut abi_regex, ")(?:{SEPARATOR_REGEX}|$)").unwrap();
        Regex::new(&abi_regex).unwrap()
    };

    struct SelectedAsset<'a> {
        abi: &'a str,
        asset: &'a Asset,
        stripped_name: String,
    }

    let mut selected: Option<SelectedAsset> = None;

    for &asset in assets {
        let name = &asset.name;
        let captures = abi_regex.captures(name)?;

        let abi_capture = captures.name("abi").unwrap();
        if abi_regex.find_at(name, abi_capture.end()).is_some() {
            return None;
        }

        let current_abi = abi_capture.as_str();
        let stripped_name = format!("{}{}",
            &name[..captures.get_match().start()],
            &name[abi_capture.end()..]);

        if let Some(selected) = selected.as_ref() {
            if current_abi == selected.abi || stripped_name != selected.stripped_name {
                return None;
            }

            let current_priority = prioritized_abi.iter().position(|&abi| abi == current_abi).unwrap();
            let selected_priority = prioritized_abi.iter().position(|&abi| abi == selected.abi).unwrap();
            if current_priority >= selected_priority {
                continue;
            }
        }

        selected = Some(SelectedAsset {
            abi: current_abi,
            asset,
            stripped_name,
        });
    }

    selected.map(|selected| selected.asset)
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

    #[test_log::test]
    #[rstest(binary_name, project_name, assets, matches, matcher_index,
        // https://github.com/KonishchevDmitry/binup
        case("binup", "binup", &[
            "binup-v2.0.0-linux-arm64.tar.bz2",
            "binup-v1.1.0-linux-x64.tar.bz2",
            "binup-v1.1.0-macos-arm64.tar.bz2",
            "binup-v1.1.0-macos-x64.tar.bz2",
        ], &[
            (OS::Linux, Arch::X86_64, Some("binup-v1.1.0-linux-x64.tar.bz2")),
            (OS::Linux, Arch::AArch64, Some("binup-v2.0.0-linux-arm64.tar.bz2")),
            (OS::MacOS, Arch::X86_64, Some("binup-v1.1.0-macos-x64.tar.bz2")),
            (OS::MacOS, Arch::AArch64, Some("binup-v1.1.0-macos-arm64.tar.bz2")),
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
            (OS::Linux, Arch::X86_64, Some("dnscrypt-proxy-linux_x86_64-2.1.5.tar.gz")),
            (OS::Linux, Arch::AArch64, Some("dnscrypt-proxy-linux_arm64-2.1.5.tar.gz")),
            (OS::MacOS, Arch::X86_64, Some("dnscrypt-proxy-macos_x86_64-2.1.5.zip")),
            (OS::MacOS, Arch::AArch64, Some("dnscrypt-proxy-macos_arm64-2.1.5.zip")),
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
            (OS::Linux, Arch::X86_64, Some("prometheus-nginxlog-exporter_1.11.0_linux_amd64.tar.gz")),
            (OS::Linux, Arch::AArch64, Some("prometheus-nginxlog-exporter_1.11.0_linux_arm64.tar.gz")),
            (OS::MacOS, Arch::X86_64, Some("prometheus-nginxlog-exporter_1.11.0_darwin_amd64.tar.gz")),
            (OS::MacOS, Arch::AArch64, Some("prometheus-nginxlog-exporter_1.11.0_darwin_arm64.tar.gz")),
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
            (OS::Linux, Arch::X86_64, Some("node_exporter-1.8.2.linux-amd64.tar.gz")),
            (OS::Linux, Arch::AArch64, Some("node_exporter-1.8.2.linux-arm64.tar.gz")),
            (OS::MacOS, Arch::X86_64, Some("node_exporter-1.8.2.darwin-amd64.tar.gz")),
            (OS::MacOS, Arch::AArch64, Some("node_exporter-1.8.2.darwin-arm64.tar.gz")),
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
            (OS::Linux, Arch::X86_64, Some("shadowsocks-v1.20.3.x86_64-unknown-linux-gnu.tar.xz")),
            (OS::Linux, Arch::AArch64, Some("shadowsocks-v1.20.3.aarch64-unknown-linux-gnu.tar.xz")),
            (OS::MacOS, Arch::X86_64, Some("shadowsocks-v1.20.3.x86_64-apple-darwin.tar.xz")),
            (OS::MacOS, Arch::AArch64, Some("shadowsocks-v1.20.3.aarch64-apple-darwin.tar.xz")),
        ], 4),

        // https://github.com/telemt/telemt
        case("telemt", "telemt", &[
            "telemt",
            "telemt-aarch64-linux-gnu.tar.gz",
            "telemt-aarch64-linux-gnu.tar.gz.sha256",
            "telemt-aarch64-linux-musl.tar.gz",
            "telemt-aarch64-linux-musl.tar.gz.sha256",
            "telemt-x86_64-linux-gnu.tar.gz",
            "telemt-x86_64-linux-gnu.tar.gz.sha256",
            "telemt-x86_64-linux-musl.tar.gz",
            "telemt-x86_64-linux-musl.tar.gz.sha256",
            "telemt-x86_64-v3-linux-gnu.tar.gz",
            "telemt-x86_64-v3-linux-gnu.tar.gz.sha256",
            "telemt-x86_64-v3-linux-musl.tar.gz",
            "telemt-x86_64-v3-linux-musl.tar.gz.sha256",
        ], &[
            (OS::Linux, Arch::X86_64, Some("telemt-x86_64-linux-gnu.tar.gz")),
            (OS::Linux, Arch::AArch64, Some("telemt-aarch64-linux-gnu.tar.gz")),
        ], 0),
        case("telemt", "telemt", &[
            "telemt",
            "telemt-aarch64-linux-gnu.tar.gz",
            "telemt-aarch64-linux-gnu.tar.gz.sha256",
            "telemt-aarch64-linux-musl.tar.gz",
            "telemt-aarch64-linux-musl.tar.gz.sha256",
            "telemt-x86_64-linux-gnu.tar.gz",
            "telemt-x86_64-linux-gnu.tar.gz.sha256",
            "telemt-x86_64-linux-musl.tar.gz",
            "telemt-x86_64-linux-musl.tar.gz.sha256",
            "telemt-x86_64-v3-linux-gnu.tar.gz",
            "telemt-x86_64-v3-linux-gnu.tar.gz.sha256",
            "telemt-x86_64-v3-linux-musl.tar.gz",
            "telemt-x86_64-v3-linux-musl.tar.gz.sha256",
        ], &[
            // We don't want this decision actually, but it's an artifact of current release matcher
            (OS::MacOS, Arch::AArch64, Some("telemt")),
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
            // There is no OS in the assets, so we match it to any OS, assuming that it's available for user's platform only
            (OS::Linux, Arch::X86_64, Some("ttyd.x86_64")),
            (OS::Linux, Arch::AArch64, Some("ttyd.aarch64")),
            (OS::MacOS, Arch::X86_64, Some("ttyd.x86_64")),
            (OS::MacOS, Arch::AArch64, Some("ttyd.aarch64")),
        ], 3),

        // https://github.com/KonishchevDmitry/binup/issues/2#issuecomment-3222682495
        case("rapidgzip", "indexed_bzip2", &["rapidgzip"], &[
            (OS::Linux, Arch::X86_64, Some("rapidgzip")),
            (OS::MacOS, Arch::X86_64, Some("rapidgzip")),
            (OS::MacOS, Arch::AArch64, Some("rapidgzip")),
        ], 6),
    )]
    fn release_matcher(binary_name: &str, project_name: &str, assets: &[&str], matches: &[(OS, Arch, Option<&str>)], matcher_index: usize) {
        for (os, arch, expected) in matches {
            let assets = assets.iter().map(|&name| Asset {
                name: name.to_owned(),
                time: Utc::now(),
                url: Url::parse("https://github.com/").unwrap(),
            }).collect_vec();

            let matchers = generate_release_matchers(binary_name, project_name, os.as_str(), arch.as_str()).unwrap();

            match match_assets(&assets, &matchers, os.as_str()) {
                Some((index, asset)) => assert_eq!(
                    (index, Some(asset.name.as_str())),
                    (matcher_index, *expected),
                ),
                None => assert_eq!(*expected, None),
            }
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