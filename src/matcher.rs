use std::fmt::{self, Display, Formatter};
use std::path::Path;

use globset::{GlobBuilder, GlobMatcher};
use regex::Regex;
use serde::Deserialize;
use serde::de::{Deserializer, Error};

#[derive(Clone)]
pub enum Matcher {
    Glob(GlobMatcher),
    Regex(Regex),
}

impl Matcher {
    pub fn matches<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();

        match self {
            Matcher::Glob(glob) => glob.is_match(path),
            Matcher::Regex(regex) => path.to_str().map(|path| regex.is_match(path)).unwrap_or(false),
        }
    }
}

impl<'de> Deserialize<'de> for Matcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let pattern: String = Deserialize::deserialize(deserializer)?;

        Ok(if let Some(regex) = pattern.strip_prefix('~') {
            Matcher::Regex(
                Regex::new(regex).map_err(|e| D::Error::custom(e.to_string()))?
            )
        } else {
            Matcher::Glob(GlobBuilder::new(&pattern)
                .literal_separator(true)
                .backslash_escape(true)
                .build().map_err(|e| D::Error::custom(e.to_string()))?
                .compile_matcher()
            )
        })
    }
}

impl Display for Matcher {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Matcher::Glob(glob) => glob.glob().fmt(formatter),
            Matcher::Regex(regex) => write!(formatter, "~{regex}"),
        }
    }
}