use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use serde_derive::Deserialize;
use validator::Validate;

use crate::core::GenericResult;

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Config {
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
    pub name: String,
    pub path: Option<String>,
}