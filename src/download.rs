use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use log::debug;
use reqwest::blocking::ClientBuilder;
use tar::{Archive, EntryType};
use url::Url;
use zip::ZipArchive;

use crate::core::{EmptyResult, GenericResult};
use crate::util;

pub enum FileType {
    Single,
    Archived {mode: u32},
}

pub trait Installer {
    fn has_binary_matcher(&self) -> bool;
    fn on_file(&mut self, path: &Path, file_type: FileType, data: &mut dyn Read) -> EmptyResult;
}

pub const ARCHIVE_EXTENSION_REGEX: &str = r"\.(?:tar\.[^/.]+|zip)";

pub fn download(url: &Url, name: &str, installer: &mut dyn Installer) -> EmptyResult {
    let (stripped_name, decompressor_builder) = get_decompressor_builder(name)?;
    let archive_type = stripped_name.rsplit_once('.').map(|(_, extension)| extension);

    let reader_builder: ReaderBuilder = match archive_type.unwrap_or_default() {
        "tar" => Box::new(|| Box::new(TarReader::new())),
        "zip" => Box::new(|| Box::new(ZipReader::new())),

        "7z" | "apk" | "deb" | "dmg" | "msi" | "pkg" | "rar" | "rpm" => {
            return Err!("Unsupported file type: {name:?}")
        },

        _ => {
            if installer.has_binary_matcher() {
                return Err!(concat!(
                    "The release file {:?} looks like a simple binary (not an archive), ",
                    "but a binary matcher is specified",
                ), name);
            }

            let binary_name = stripped_name.to_owned();
            Box::new(move || Box::new(BinaryReader::new(&binary_name)))
        },
    };

    debug!("Downloading {url}...");
    let client = ClientBuilder::new().user_agent(util::USER_AGENT).build()?;

    let response = client.get(url.to_owned()).send()?;
    if !response.status().is_success() {
        return Err!("The server returned an error: {}", response.status())
    }

    let decompressor = decompressor_builder(Box::new(response));
    let reader = reader_builder();

    reader.read(decompressor, installer)
}

type DecompressorBuilder = Box<dyn FnOnce(Box<dyn Read>) -> Box<dyn Read>>;
pub const COMPRESSION_EXTENSION_REGEX: &str = r"\.(?:bz2|gz|lz|lz4|lzma|lzo|xz|z|zst)";

fn get_decompressor_builder(name: &str) -> GenericResult<(&str, DecompressorBuilder)> {
    let Some((stripped_name, extension)) = name.rsplit_once('.') else {
        return Ok((name, Box::new(|reader| reader)));
    };

    let builder: DecompressorBuilder = match extension {
        "bz2" => Box::new(|reader| Box::new(bzip2::read::BzDecoder::new(reader))),
        "gz" => Box::new(|reader| Box::new(flate2::read::GzDecoder::new(reader))),
        "xz" => Box::new(|reader| Box::new(xz2::read::XzDecoder::new(reader))),
        "lz" | "lz4" | "lzma" | "lzo" | "z" | "zst" => return Err!("Unsupported file type: {name:?}"),
        _ => return Ok((name, Box::new(|reader| reader))),
    };

    Ok((stripped_name, builder))
}

type ReaderBuilder = Box<dyn FnOnce() -> Box<dyn ReleaseReader>>;

trait ReleaseReader {
    fn read(self: Box<Self>, reader: Box<dyn Read>, installer: &mut dyn Installer) -> EmptyResult;
}

struct BinaryReader {
    name: String,
}

impl BinaryReader {
    fn new(name: &str) -> BinaryReader {
        BinaryReader {
            name: name.to_owned(),
        }
    }
}

impl ReleaseReader for BinaryReader {
    fn read(self: Box<Self>, mut reader: Box<dyn Read>, installer: &mut dyn Installer) -> EmptyResult {
        installer.on_file(Path::new(self.name.as_str()), FileType::Single, &mut reader)
    }
}

struct TarReader {
}

impl TarReader {
    fn new() -> TarReader {
        TarReader {}
    }
}

impl ReleaseReader for TarReader {
    fn read(self: Box<Self>, reader: Box<dyn Read>, installer: &mut dyn Installer) -> EmptyResult {
        let mut archive = Archive::new(reader);

        for (index, entry) in archive.entries()?.enumerate() {
            let mut entry = entry?;

            let header = entry.header();
            let path = entry.path()?;
            let entry_type = header.entry_type();

            if index == 0 {
                debug!("Processing the archive:")
            }
            debug!("* {path:?} ({entry_type:?})");

            if matches!(entry_type, EntryType::Regular | EntryType::Continuous) {
                let path = path.to_path_buf();
                let file_type = FileType::Archived {mode: header.mode()?};
                installer.on_file(&path, file_type, &mut entry)?;
            }
        }

        Ok(())
    }
}

struct ZipReader {
}

impl ZipReader {
    fn new() -> ZipReader {
        ZipReader {}
    }
}

// Examples of real projects for testing:
// * https://github.com/sxyazi/yazi
// * https://github.com/tstack/lnav
impl ReleaseReader for ZipReader {
    fn read(self: Box<Self>, mut reader: Box<dyn Read>, installer: &mut dyn Installer) -> EmptyResult {
        let temp_dir = util::temp_dir();

        // Please note that tempfile::tempfile_in() creates unlinked file, which can't be done with tempfile::Builder
        let mut temp_file = tempfile::tempfile_in(&temp_dir).map_err(|e| format!(
            "Unable to create a temporary file in {temp_dir:?}: {e}"))?;

        io::copy(&mut reader, &mut temp_file)?;
        temp_file.flush()?;
        temp_file.seek(SeekFrom::Start(0))?;

        let mut archive = ZipArchive::new(temp_file)?;

        for index in 0..archive.len() {
            let mut file = archive.by_index(index)?;
            let path = PathBuf::from(file.name());

            if index == 0 {
                debug!("Processing the archive:")
            }
            debug!("* {path:?} ({})", match file {
                _ if file.is_symlink() => "symlink",
                _ if file.is_dir() => "directory",
                _ => "file",
            });

            if file.is_file() {
                let mode = file.unix_mode().unwrap_or_default();
                let file_type = FileType::Archived {mode};
                installer.on_file(&path, file_type, &mut file)?;
            }
        }

        Ok(())
    }
}