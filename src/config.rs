use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use log::error;
use nondestructive::yaml::{self, Document, Separator};
use serde::Deserialize;
use validator::Validate;

use crate::core::{EmptyResult, GenericResult};
use crate::github::GithubConfig;
use crate::tool::ToolSpec;
use crate::util;

#[derive(Clone, Deserialize, PartialEq, Validate)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(skip)]
    source: Option<ConfigSource>,

    #[serde(rename = "path", default = "default_install_path", deserialize_with = "util::deserialize_path")]
    pub path: PathBuf,

    #[serde(default)]
    #[validate(nested)]
    pub tools: BTreeMap<String, ToolSpec>,

    #[serde(default)]
    pub github: GithubConfig,
}

impl Config {
    pub fn load(path: &Path, custom: bool) -> GenericResult<Config> {
        let (mut reader, exists) = match File::open(path) {
            Ok(file) => (ConfigReader::new(file), true),
            Err(err) => {
                if custom || err.kind() != ErrorKind::NotFound {
                    return Err(err.into());
                }
                (ConfigReader::new("".as_bytes()), false)
            },
        };

        let mut config = Config::read(&mut reader)?;
        config.source.replace(ConfigSource {
            path: path.to_owned(),
            data: reader.consume(),
            exists,
        });

        Ok(config)
    }

    pub fn edit<E, P, R>(&mut self, edit: E, process: P) -> GenericResult<R>
        where
            E: FnOnce(&mut Config, &mut Document) -> EmptyResult,
            P: FnOnce(&Config) -> GenericResult<R>
    {
        let error_prefix = "Failed to edit the configuration file: its current format is not supported by the underlaying library.";

        let mut expected_config = self.clone();
        let mut source = expected_config.source.take().unwrap();

        let mut raw = yaml::from_slice(&source.data).map_err(|e| format!(
            "{error_prefix} Got an error: {e}"))?;

        edit(&mut expected_config, &mut raw)?;

        let result = raw.to_string();
        let mut config = Config::read(result.as_bytes()).map_err(|e| format!(
            "{error_prefix} Got the following invalid config ({e}):\n{result}"))?;

        if config != expected_config {
            return Err!("{error_prefix} Got the following unexpected config:\n{result}");
        }

        source.data = result.into_bytes();
        config.source.replace(source);
        let result = process(&config)?;

        let source = config.source.as_mut().unwrap();

        if !source.exists {
            if let Some(path) = source.path.parent() {
                fs::create_dir_all(path).map_err(|e| format!(
                    "Unable to create {path:?}: {e}"))?;
            }
            source.exists = true;
        }

        Config::write(&source.path, &source.data)?;
        *self = config;

        Ok(result)
    }

    pub fn get_tool_path(&self, name: &str, spec: &ToolSpec) -> PathBuf {
        spec.path.as_ref().unwrap_or(&self.path).join(name)
    }

    pub fn update_tool(&mut self, raw: &mut Document, name: &str, spec: &ToolSpec) -> EmptyResult {
        let mut root = raw.as_mut().make_mapping();

        let mut tools = match root.get_mut("tools") {
            Some(tools) => tools,
            None => root.insert("tools", Separator::Auto),
        }.make_mapping();

        let mut tool = match tools.get_mut(name) {
            Some(tool) => tool,
            None => tools.insert(name, Separator::Auto),
        }.make_mapping();

        spec.serialize(&mut tool)?;
        self.tools.insert(name.to_owned(), spec.clone());

        Ok(())
    }

    pub fn remove_tool(&mut self, raw: &mut Document, name: &str) -> EmptyResult {
        let removed = || -> Option<bool> {
            Some(raw.as_mut().as_mapping_mut()?
                .get_mut("tools")?.as_mapping_mut()?
                .remove(name))
        }().unwrap_or_default();

        if !removed || self.tools.remove(name).is_none() {
            return Err!("Unable to find the tool in the configuration file")
        }

        Ok(())
    }

    fn read<R: Read>(reader: R) -> GenericResult<Config> {
        let config: Config = serde_yaml::from_reader(reader)?;
        config.validate()?;
        Ok(config)
    }

    fn write(path: &Path, data: &[u8]) -> EmptyResult {
        let temp_path = {
            let mut path = path.as_os_str().to_owned();
            path.push(".new");
            PathBuf::from(path)
        };

        if let Err(err) = fs::remove_file(&temp_path) && err.kind() != ErrorKind::NotFound {
            return Err!("Unable to delete {temp_path:?}: {err}");
        }

        let mut open_options = OpenOptions::new();
        open_options.create_new(true).write(true);

        match fs::metadata(path) {
            Ok(metadata) => {
                open_options.mode(metadata.mode());
            },
            Err(err) => if err.kind() != ErrorKind::NotFound {
                return Err!("Unable to stat() {path:?}: {err}");
            }
        }

        open_options.open(&temp_path)
            .and_then(|mut file| {
                file.write_all(data).inspect_err(|_| {
                    if let Err(err) = fs::remove_file(&temp_path) {
                        error!("Failed to delete {temp_path:?}: {err}.");
                    }
                })
            })
            .and_then(|_| fs::rename(&temp_path, path))
            .map_err(|e| format!("Failed to write {path:?}: {e}"))?;

        Ok(())
    }
}

fn default_install_path() -> PathBuf {
    PathBuf::from(shellexpand::tilde("~/.local/bin").to_string())
}

#[derive(Clone, PartialEq)]
struct ConfigSource {
    path: PathBuf,
    data: Vec<u8>,
    exists: bool,
}

struct ConfigReader {
    reader: Box<dyn Read>,
    data: Vec<u8>,
}

impl ConfigReader {
    fn new<R: Read + 'static>(reader: R) -> ConfigReader {
        ConfigReader {
            reader: Box::new(reader),
            data: Vec::new(),
        }
    }

    fn consume(self) -> Vec<u8> {
        self.data
    }
}

impl Read for ConfigReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf).inspect(|&size| {
            self.data.extend_from_slice(&buf[..size]);
        })
    }
}