use std::io::Read;
use std::path::Path;

use const_format::formatcp;
use log::debug;
use reqwest::blocking::ClientBuilder;
use tar::{Archive, EntryType};
use url::Url;

use crate::core::{EmptyResult, GenericResult};

static USER_AGENT: &str = formatcp!(
    "{name}/{version} ({homepage})",
    name=env!("CARGO_PKG_NAME"), version=env!("CARGO_PKG_VERSION"), homepage=env!("CARGO_PKG_REPOSITORY"),
);

pub trait Installer {
    fn on_file(&mut self, path: &Path, data: &mut dyn Read) -> EmptyResult;
}

pub fn download(url: &Url, name: &str, installer: &mut dyn Installer) -> EmptyResult {
    let reader = ReleaseReaderBuilder::new(name)?;
    let client = ClientBuilder::new().user_agent(USER_AGENT).build()?;

    debug!("Downloading {url}...");

    let response = client.get(url.to_owned()).send()?;
    if !response.status().is_success() {
        return Err!("The server returned and error: {}", response.status())
    }

    let mut archive = reader.build(response);

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
            let mode = header.mode();
            installer.on_file(&path, &mut entry)?;
        }
    }

    Ok(())
}

struct ReleaseReaderBuilder {
    decoder_builder: Box<dyn FnOnce(Box<dyn Read>) -> Box<dyn Read>>,
}

impl ReleaseReaderBuilder {
    fn new(name: &str) -> GenericResult<ReleaseReaderBuilder> {
        let decoder_builder = name.rsplit_once('.').and_then(|(name, extension)| {
            let decoder: Box<dyn FnOnce(Box<dyn Read>) -> Box<dyn Read>> = match extension {
                "bz2" => Box::new(|reader| Box::new(bzip2::read::BzDecoder::new(reader))),
                "gz" => Box::new(|reader| Box::new(flate2::read::GzDecoder::new(reader))),
                _ => return None,
            };

            if name.rsplit_once('.')?.1 != "tar" {
                return None;
            }

            Some(decoder)
        }).ok_or_else(|| format!("Unsupported file type: {name:?}"))?;

        Ok(ReleaseReaderBuilder {decoder_builder})
    }

    fn build<R: Read + 'static>(self, reader: R) -> Archive<impl Read> {
        let reader = (self.decoder_builder)(Box::new(reader));
        Archive::new(reader)
    }
}