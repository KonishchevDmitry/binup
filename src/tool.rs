use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use log::debug;
use nondestructive::yaml::MappingMut;
use serde::Deserialize;
use url::Url;
use validator::Validate;

use crate::core::{EmptyResult, GenericResult};
use crate::matcher::Matcher;
use crate::util;
use crate::version::VersionSource;

#[derive(Deserialize, Validate, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    #[validate(length(min = 1))]
    pub project: String,
    #[serde(default)]
    pub prerelease: bool,
    pub changelog: Option<Url>,

    pub release_matcher: Option<Matcher>,
    pub binary_matcher: Option<Matcher>,
    pub version_source: Option<VersionSource>,

    #[serde(default, deserialize_with = "util::deserialize_optional_path")]
    pub path: Option<PathBuf>,
    pub post: Option<String>,
}

impl ToolSpec {
    pub fn serialize(&self, map: &mut MappingMut) -> EmptyResult {
        map.clear();
        map.insert_str("project", &self.project);

        if self.prerelease {
            map.insert_bool("prerelease", true);
        }
        if let Some(ref changelog) = self.changelog {
            map.insert_str("changelog", changelog.as_str());
        }
        if let Some(ref release_matcher) = self.release_matcher {
            map.insert_str("release_matcher", release_matcher.to_string());
        }
        if let Some(ref binary_matcher) = self.binary_matcher {
            map.insert_str("binary_matcher", binary_matcher.to_string());
        }
        if let Some(ref version_source) = self.version_source {
            map.insert_str("version_source", Into::<&str>::into(version_source));
        }
        if let Some(ref path) = self.path {
            let path = path.to_str().ok_or_else(|| format!("Invalid path: {path:?}"))?;
            map.insert_str("path", path);
        }
        if let Some(ref post) = self.post {
            map.insert_str("post", post);
        }

        Ok(())
    }
}

pub struct ToolState {
    pub modify_time: SystemTime,
}

pub fn check(path: &Path) -> GenericResult<Option<ToolState>> {
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