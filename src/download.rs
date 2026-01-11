use std::io::Read;
use std::path::Path;

use log::debug;
use reqwest::blocking::ClientBuilder;
use tar::{Archive, EntryType};
use url::Url;

use crate::core::EmptyResult;
use crate::util;

pub enum FileType {
    #[allow(dead_code)] // FIXME(konishchev): Implement it
    Single,
    Archived {mode: u32},
}

pub trait Installer {
    fn has_binary_matcher(&self) -> bool;
    fn on_file(&mut self, path: &Path, file_type: FileType, data: &mut dyn Read) -> EmptyResult;
}

pub fn download(url: &Url, name: &str, installer: &mut dyn Installer) -> EmptyResult {
    let mut stripped_name = name;

    let mut decoder_builder: DecoderBuilder = Box::new(|reader| Box::new(reader));
    if let Some((prefix, extension)) = stripped_name.rsplit_once('.')
        && let Some(builder) = get_decoder_builder(extension) {
        decoder_builder = builder;
        stripped_name = prefix;
    }

    let reader_builder: ReaderBuilder = match stripped_name.rsplit_once('.') {
        Some((_, "tar")) => Box::new(|| Box::new(TarReader::new())),
        // XXX(konishchev): + installer.has_binary_matcher()
        _ => return Err!("Unsupported file type: {name:?}")
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

fn get_decoder_builder(extension: &str) -> Option<DecoderBuilder> {
    Some(match extension {
        "bz2" => Box::new(|reader| Box::new(bzip2::read::BzDecoder::new(reader))),
        "gz" => Box::new(|reader| Box::new(flate2::read::GzDecoder::new(reader))),
        "xz" => Box::new(|reader| Box::new(xz2::read::XzDecoder::new(reader))),
        _ => return None,
    })
}

type ReaderBuilder = Box<dyn FnOnce() -> Box<dyn ReleaseReader>>;

trait ReleaseReader {
    fn read(self: Box<Self>, reader: Box<dyn Read>, installer: &mut dyn Installer) -> EmptyResult;
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