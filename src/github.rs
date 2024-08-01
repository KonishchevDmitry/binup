use std::env::{self, VarError};
use std::error::Error as _;

use chrono::{DateTime, Utc};
use http::StatusCode;
use log::{debug, trace};
use octocrab::{OctocrabBuilder, Error};
use octocrab::models::repos::Release as ReleaseModel;
use tokio::runtime::Runtime;
use url::Url;

use crate::core::GenericResult;

pub struct Release {
    pub tag: String,
    pub assets: Vec<Asset>,
}

pub struct Asset {
    pub name: String,
    pub time: DateTime<Utc>,
    pub url: Url,
}

pub fn get_release(project: &str) -> GenericResult<Release> {
    create_runtime()?.block_on(get_release_async(project))
}

async fn get_release_async(project: &str) -> GenericResult<Release> {
    let (owner, repository) = parse_project_name(project)?;

    let mut builder = OctocrabBuilder::new();
    if let Some(token) = get_token()? {
        builder = builder.user_access_token(token);
    }

    let github = builder.build()?;
    let repository = github.repos(owner, repository);

    debug!("Getting {project} release info...");

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
                    Error::GitHub {source, ..} if source.status_code == StatusCode::NOT_FOUND => "The project doesn't exist".into(),
                    _ => humanize_error(err),
                }
            })?;
            return Err!("The project has no releases");
        },
    };

    trace!("The latest {project} release:\n{release:#?}");

    Ok(Release {
        tag: release.tag_name,
        assets: release.assets.into_iter().map(|asset| {
            Asset {
                name: asset.name,
                time: asset.updated_at,
                url: asset.browser_download_url,
            }
        }).collect(),
    })
}

fn parse_project_name(name: &str) -> GenericResult<(&str, &str)> {
    let mut parts = name.split('/');

    let owner = parts.next();
    let repository = parts.next();
    let extra = parts.next();

    Ok(match (owner, repository, extra) {
        (Some(owner), Some(repository), None) => (owner, repository),
        _ => {
            return Err!("Invalid GitHub project name");
        },
    })
}

fn create_runtime() -> GenericResult<Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!(
        "Failed to create tokio runtime: {e}"))?)
}

fn get_token() -> GenericResult<Option<String>> {
    const VAR_NAME: &str = "GITHUB_TOKEN";

    Ok(match env::var(VAR_NAME) {
        Ok(token) => {
            debug!("Using GitHub token from {VAR_NAME} environment variable.");
            Some(token)
        },
        Err(VarError::NotPresent) => None,
        Err(err) => return Err!("{VAR_NAME} environment variable has an invalid value: {err}"),
    })
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