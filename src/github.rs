use std::error::Error as _;

use chrono::{DateTime, Utc};
use http::StatusCode;
use log::{debug, trace};
use octocrab::{OctocrabBuilder, Error};
use octocrab::models::repos::Release as ReleaseModel;
use serde::Deserialize;
use tokio::runtime::Runtime;
use url::Url;

use crate::core::GenericResult;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    token: Option<String>,
}

pub struct Release {
    pub tag: String,
    pub assets: Vec<Asset>,
    pub changelog: Url,
}

pub struct Asset {
    pub name: String,
    pub time: DateTime<Utc>,
    pub url: Url,
}

pub fn get_release(config: &GithubConfig, project: &str) -> GenericResult<Release> {
    create_runtime()?.block_on(get_release_async(config, project))
}

async fn get_release_async(config: &GithubConfig, full_name: &str) -> GenericResult<Release> {
    let project = parse_project_name(full_name)?;

    let mut builder = OctocrabBuilder::new();
    if let Some(token) = config.token.as_ref() {
        builder = builder.user_access_token(token.to_owned());
    }

    let github = builder.build()?;
    let repository = github.repos(project.owner, project.name);

    debug!("Getting {full_name} release info...");

    let release = repository.releases().get_latest().await
        .map(Some)
        .or_else(|err| -> GenericResult<Option<ReleaseModel>> {
            match err {
                Error::GitHub {source, ..} if source.status_code == StatusCode::NOT_FOUND => Ok(None),
                _ => Err!("{}", humanize_error(err)),
            }
        })?;

    let release = match release {
        Some(release) => release,
        None => {
            repository.get().await.map_err(|err| {
                match err {
                    Error::GitHub {source, ..} if source.status_code == StatusCode::NOT_FOUND => {
                        "The project doesn't exist".into()
                    },
                    _ => humanize_error(err),
                }
            })?;
            return Err!("The project has no releases");
        },
    };

    trace!("The latest {full_name} release:\n{release:#?}");

    Ok(Release {
        tag: release.tag_name,
        assets: release.assets.into_iter().map(|asset| {
            Asset {
                name: asset.name,
                time: asset.updated_at,
                url: asset.browser_download_url,
            }
        }).collect(),
        changelog: project.changelog,
    })
}

struct Project {
    name: String,
    owner: String,
    changelog: Url,
}

fn parse_project_name(full_name: &str) -> GenericResult<Project> {
    let mut parts = full_name.split('/');

    let owner = parts.next();
    let name = parts.next();
    let extra = parts.next();
    let changelog = Url::parse(&format!("https://github.com/{}/releases", full_name)).ok();

    Ok(match (owner, name, extra, changelog) {
        (Some(owner), Some(name), None, Some(changelog)) => Project {
            name: name.to_owned(),
            owner: owner.to_owned(),
            changelog,
        },
        _ => {
            return Err!("Invalid GitHub project name");
        },
    })
}

fn create_runtime() -> GenericResult<Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!(
        "Failed to create tokio runtime: {e}"))?)
}

// octocrab errors are very human-unfriendly
fn humanize_error(err: Error) -> String {
    let mut message = String::new();
    let mut source = err.source();

    while let Some(inner) = source {
        if message.is_empty() {
            message = inner.to_string();
        } else {
            let inner_message = inner.to_string();
            if message.ends_with(&inner_message) {
                break;
            }
            message = format!("{message}: {inner_message}");
        }
        source = inner.source();
    }

    if message.is_empty() {
        message = err.to_string();
    }

    message
}