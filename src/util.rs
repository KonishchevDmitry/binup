use std::fmt::Display;
use std::path::PathBuf;

use const_format::formatcp;
use itertools::Itertools;
use serde::Deserialize;
use serde::de::{Deserializer, Error};

pub static USER_AGENT: &str = formatcp!(
    "{name}/{version} ({homepage})",
    name=env!("CARGO_PKG_NAME"), version=env!("CARGO_PKG_VERSION"), homepage=env!("CARGO_PKG_REPOSITORY"),
);

pub fn format_list<T: Display, I: Iterator<Item = T>>(mut iter: I) -> String {
    "\n* ".to_owned() + &iter.join("\n* ")
}

pub fn format_multiline(text: &str) -> String {
    let text = text.trim_end();

    if text.find('\n').is_some() {
        format!("\n{text}")
    } else {
        format!(" {text}")
    }
}

pub fn deserialize_path<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
    where D: Deserializer<'de>
{
    let path: String = Deserialize::deserialize(deserializer)?;
    parse_path::<D>(&path)
}

pub fn deserialize_optional_path<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error>
    where D: Deserializer<'de>
{
    let path: Option<String> = Deserialize::deserialize(deserializer)?;
    path.as_deref().map(parse_path::<D>).transpose()
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