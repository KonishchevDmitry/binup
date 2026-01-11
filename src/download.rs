use std::io::Read;
use std::path::Path;

use log::debug;
use reqwest::blocking::ClientBuilder;
use tar::{Archive, EntryType};
use url::Url;

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

pub fn download(url: &Url, name: &str, installer: &mut dyn Installer) -> EmptyResult {
    let (stripped_name, decoder_builder) = get_decoder_builder(name)?;
    let archive_type = stripped_name.rsplit_once('.').map(|(_, extension)| extension);

    let reader_builder: ReaderBuilder = match archive_type.unwrap_or_default() {
        "tar" => Box::new(|| Box::new(TarReader::new())),
        "7z" | "apk" | "deb" | "dmg" | "msi" | "pkg" | "rar" | "rpm" | "zip" => {
            return Err!("Unsupported file type: {name:?}")
        },
        _ => {
            if installer.has_binary_matcher() {
                return Err!(concat!(
                    "The release file {:?} looks like a simple binary (not an archive), ",
                    "but a binary matcher is specified",
                ), name);
            }

            let name = name.to_owned();
            Box::new(move || Box::new(BinaryReader::new(&name)))
        },
    };

    debug!("Downloading {url}...");
    let client = ClientBuilder::new().user_agent(util::USER_AGENT).build()?;

    let response = client.get(url.to_owned()).send()?;
    if !response.status().is_success() {
        return Err!("The server returned an error: {}", response.status())
    }

    let decoder = decoder_builder(Box::new(response));
    let reader = reader_builder();

    reader.read(decoder, installer)
}

type DecoderBuilder = Box<dyn FnOnce(Box<dyn Read>) -> Box<dyn Read>>;

fn get_decoder_builder(name: &str) -> GenericResult<(&str, DecoderBuilder)> {
    let Some((stripped_name, extension)) = name.rsplit_once('.') else {
        return Ok((name, Box::new(|reader| reader)));
    };

    let builder: DecoderBuilder = match extension {
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