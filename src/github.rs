use std::error::Error as _;

use http::StatusCode;
use log::{debug, trace};
use octocrab::{self, Error};
use octocrab::models::repos::Release as ReleaseModel;
use tokio::runtime::Runtime;

use crate::core::GenericResult;

pub struct Release {

}

pub fn get_release(project: &str) -> GenericResult<Release> {
    create_runtime()?.block_on(get_release_async(project))
}

async fn get_release_async(project: &str) -> GenericResult<Release> {
    let (owner, repository) = parse_project_name(project)?;

    let github = octocrab::instance();
    let repository = github.repos(owner, repository);

    debug!("Getting {project} release info...");

    let release = repository.releases().get_latest().await
        .map(|release| Some(release))
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

    Ok(Release {})
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