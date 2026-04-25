//! REST: list open PRs for a repository (with ETag caching via [`Client`]).

use chrono::{DateTime, Utc};
use gh_core::{PrSummary, RepoRef};
use thiserror::Error;
use tracing::{debug, instrument};

use crate::client::{ApiError, Client};

#[derive(Debug, Error)]
pub enum PullsError {
    #[error("repo `{0}` not found or no access")]
    NotFound(String),
    #[error("api error: {0}")]
    Api(#[from] ApiError),
}

/// Fetch the first page of open PRs (up to 30 items) for `repo`.
///
/// Pagination via streaming arrives in PR #4; for the MVP we eagerly take
/// the first page so the screen has something to show immediately.
#[instrument(skip(client))]
pub async fn list_open_prs(client: &Client, repo: &RepoRef) -> Result<Vec<PrSummary>, PullsError> {
    let path = format!(
        "/repos/{}/{}/pulls?state=open&sort=created&direction=desc&per_page=30",
        repo.owner, repo.name
    );

    let raw: Vec<octocrab::models::pulls::PullRequest> =
        client.get_json(&path).await.map_err(|e| match e {
            ApiError::NotFound => PullsError::NotFound(repo.slug()),
            other => PullsError::Api(other),
        })?;

    debug!(count = raw.len(), "got pr page");
    Ok(raw.into_iter().map(from_octocrab).collect())
}

fn from_octocrab(p: octocrab::models::pulls::PullRequest) -> PrSummary {
    let title = p.title.unwrap_or_default();
    let author = p
        .user
        .map(|u| u.login)
        .unwrap_or_else(|| "<unknown>".to_string());
    let head_ref = p.head.ref_field;
    let base_ref = p.base.ref_field;
    let comments = u32::try_from(p.comments.unwrap_or(0)).unwrap_or(u32::MAX);
    let additions = u32::try_from(p.additions.unwrap_or(0)).unwrap_or(u32::MAX);
    let deletions = u32::try_from(p.deletions.unwrap_or(0)).unwrap_or(u32::MAX);
    let created_at: DateTime<Utc> = p.created_at.unwrap_or_else(Utc::now);

    PrSummary {
        number: p.number,
        title,
        author,
        draft: p.draft.unwrap_or(false),
        head_ref,
        base_ref,
        comments,
        created_at,
        additions,
        deletions,
    }
}
