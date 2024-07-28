use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;
use serde::de::{Deserializer, Error};
use validator::Validate;

use crate::core::GenericResult;

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_install_path")]
    #[serde(deserialize_with = "deserialize_path")]
    pub path: PathBuf,

    #[validate(nested)]
    pub tools: BTreeMap<String, Tool>,
}

impl Config {
    pub fn load(path: &Path) -> GenericResult<Config> {
        let config: Config = serde_yaml::from_reader(File::open(path)?)?;
        config.validate()?;
        Ok(config)
    }
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    #[validate(length(min = 1))]
    pub project: String,

    #[serde(deserialize_with = "deserialize_glob")]
    pub release_matcher: GlobMatcher,

    #[serde(default, deserialize_with = "deserialize_optional_glob")]
    pub binary_matcher: Option<GlobMatcher>,

    #[serde(default, deserialize_with = "deserialize_optional_path")]
    pub path: Option<PathBuf>,
}

fn default_install_path() -> PathBuf {
    PathBuf::from(shellexpand::tilde("~/.local/bin").to_string())
}

fn deserialize_path<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
    where D: Deserializer<'de>
{
    let path: String = Deserialize::deserialize(deserializer)?;
    parse_path::<D>(&path)
}

fn deserialize_optional_path<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error>
    where D: Deserializer<'de>
{
    let path: Option<String> = Deserialize::deserialize(deserializer)?;
    path.as_deref().map(parse_path::<D>).transpose()
}

fn deserialize_glob<'de, D>(deserializer: D) -> Result<GlobMatcher, D::Error>
    where D: Deserializer<'de>
{
    let glob: String = Deserialize::deserialize(deserializer)?;
    parse_glob::<D>(&glob)
}

fn deserialize_optional_glob<'de, D>(deserializer: D) -> Result<Option<GlobMatcher>, D::Error>
    where D: Deserializer<'de>
{
    let glob: Option<String> = Deserialize::deserialize(deserializer)?;
    glob.as_deref().map(parse_glob::<D>).transpose()
}

fn parse_path<'de, D>(path: &str) -> Result<PathBuf, D::Error>
    where D: Deserializer<'de>
{
    let path = PathBuf::from(shellexpand::tilde(path).to_string());
    if !path.is_absolute() {
        return Err(D::Error::custom("The path must be absolute"));
    }
    Ok(path)
}

fn parse_glob<'de, D>(glob: &str) -> Result<GlobMatcher, D::Error>
    where D: Deserializer<'de>
{
    Ok(GlobBuilder::new(glob)
        .literal_separator(true)
        .backslash_escape(true)
        .build().map_err(|e| D::Error::custom(format!("Invalid glob ({glob:?}): {e}")))?
        .compile_matcher())
}