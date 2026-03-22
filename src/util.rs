use std::env::{self, consts};
use std::fmt::Display;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use const_format::formatcp;
use itertools::Itertools;
use log::debug;
use platforms::OS;
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

pub fn confirm<S: Display>(message: S) -> bool {
    loop {
        if let Err(err) = write!(io::stderr(), "{} (y/n): ", message)
            .and_then(|_| io::stderr().flush()) {
            debug!("Failed to question the user: {err}. Assume no.");
            return false;
        }

        let mut answer = String::new();

        match io::stdin().read_line(&mut answer) {
            Ok(size) => if size == 0 {
                let _ = writeln!(io::stderr());
                debug!("Failed to question the user: stdin is closed. Assume no.");
                return false;
            },
            Err(err) => {
                let _ = writeln!(io::stderr());
                debug!("Failed to question the user: {err}. Assume no.");
                return false;
            }
        }

        match answer.trim() {
            "y" | "yes" => return true,
            "n" | "no" => return false,
            _ => {},
        }
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

pub fn temp_dir() -> PathBuf {
    let mut temp_dir = env::temp_dir();

    // On modern distributives /tmp is a tmpfs mount, so use /var/tmp instead of it
    if let Ok(os) = OS::from_str(consts::OS) && os == OS::Linux && temp_dir == Path::new("/tmp") {
        temp_dir = PathBuf::from("/var/tmp");
    }

    temp_dir
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