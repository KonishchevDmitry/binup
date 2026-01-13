use std::io::{Read, Seek, SeekFrom};
use std::env::consts;
use std::str::FromStr;

use file_format::{FileFormat, Kind};
use platforms::OS;

use crate::core::GenericResult;

pub fn is_executable<R: Read + Seek>(mut reader: R) -> GenericResult<(String, bool)> {
    let format = {
        reader.seek(SeekFrom::Start(0))?;
        FileFormat::from_reader(reader)?
    };

    let description = format!(
        "{full_name}{short_name} ({kind:?}, {media_type})",
        full_name=format.name(), short_name=format.short_name().map(|name| format!(" / {name}")).unwrap_or_default(),
        kind=format.kind(), media_type=format.media_type(),
    );

    let executable = get_os_specific_executable_types().unwrap_or_default().contains(&format)
        || format.kind() == Kind::Other && format.name().ends_with(" Script");

    Ok((description, executable))
}

fn get_os_specific_executable_types() -> Option<Vec<FileFormat>> {
    Some(match OS::from_str(consts::OS).ok()? {
        OS::Linux => vec![FileFormat::ExecutableAndLinkableFormat],
        OS::MacOS => vec![FileFormat::MachO],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_support() {
        assert!(
            get_os_specific_executable_types().is_some(),
            "Unsupported OS: {}", consts::OS,
        );
    }
}