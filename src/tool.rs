use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::SystemTime;

use log::debug;

use crate::core::GenericResult;

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