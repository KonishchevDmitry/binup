use std::error::Error as _;

use futures_util::TryStreamExt;
use http::{StatusCode, header};
use log::{debug, trace};
use octocrab::{Octocrab, OctocrabBuilder, Error};
use octocrab::models::repos::Release as ReleaseModel;
use serde::Deserialize;
use tokio::pin;
use tokio::runtime::Runtime;
use url::Url;

use crate::core::GenericResult;
use crate::project::Project;
use crate::release::{Release, Asset};
use crate::util;

#[derive(Clone, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    token: Option<String>,
}

pub struct Github {
    runtime: Runtime,
    client: Octocrab,
}

impl Github {
    pub fn new(config: &GithubConfig) -> GenericResult<Github> {
        let runtime = create_runtime()?;

        let client = runtime.block_on(async {
            let mut builder = OctocrabBuilder::new()
                .add_header(header::USER_AGENT, util::USER_AGENT.to_owned());

            if let Some(token) = config.token.as_ref() {
                builder = builder.user_access_token(token.to_owned());
            }

            builder.build()
        })?;

        Ok(Github {runtime, client})
    }

    pub fn get_release(&self, project: &str, allow_prerelease: bool) -> GenericResult<Option<Release>> {
        self.runtime.block_on(self.get_release_async(project, allow_prerelease))
    }

    async fn get_release_async(&self, project: &str, allow_prerelease: bool) -> GenericResult<Option<Release>> {
        let project = parse_project_name(project)?;
        debug!("Getting {} release info (allow prerelease: {allow_prerelease})...", project.full_name());

        let release = if allow_prerelease {
            self.get_latest_any_release(&project).await?
        } else {
            match self.get_latest_final_release(&project).await? {
                Some(release) => Some(release),
                None => self.get_latest_any_release(&project).await?,
            }
        };

        let Some(release) = release else {
            debug!("{} has no releases.", project.full_name());
            return Ok(None);
        };

        trace!("The latest {} release:\n{release:#?}", project.full_name());

        Ok(Some(Release::new(project, &release.tag_name, release.assets.into_iter().map(|asset| {
            Asset {
                name: asset.name,
                time: asset.updated_at,
                url: asset.browser_download_url,
            }
        }).collect())))
    }

    async fn get_latest_final_release(&self, project: &Project) -> GenericResult<Option<ReleaseModel>> {
        let repository = self.client.repos(&project.owner, &project.name);

        Ok(match repository.releases().get_latest().await {
            Ok(release) => Some(release),
            Err(Error::GitHub {source, ..}) if source.status_code == StatusCode::NOT_FOUND => {
                repository.get().await.map_err(map_project_error)?;
                None
            },
            Err(err) => return Err!("{}", humanize_error(err))
        })
    }

    async fn get_latest_any_release(&self, project: &Project) -> GenericResult<Option<ReleaseModel>> {
        let repository = self.client.repos(&project.owner, &project.name);

        let releases = repository.releases().list().send().await
            .map_err(map_project_error)?
            .into_stream(&self.client);
        pin!(releases);

        while let Some(release) = releases.try_next().await.map_err(humanize_error)? {
            if !release.draft {
                return Ok(Some(release))
            }
        }

        Ok(None)
    }
}

pub fn parse_project_name(full_name: &str) -> GenericResult<Project> {
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

fn map_project_error(err: Error) -> String {
    match err {
        Error::GitHub {source, ..} if source.status_code == StatusCode::NOT_FOUND => {
            "The project doesn't exist".to_owned()
        },
        _ => humanize_error(err),
    }
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