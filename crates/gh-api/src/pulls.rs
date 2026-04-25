//! REST: list open PRs for a repository, one page at a time.
//!
//! Each page URL (`?page=N`) becomes its own ETag-cached entry, so a second
//! visit to the same repo with no PR changes serves every page from cache.

use chrono::{DateTime, Utc};
use gh_core::{PrSummary, RepoRef};
use thiserror::Error;
use tracing::{debug, instrument};

use crate::client::{ApiError, Client, Page};

#[derive(Debug, Error)]
pub enum PullsError {
    #[error("repo `{0}` not found or no access")]
    NotFound(String),
    #[error("api error: {0}")]
    Api(#[from] ApiError),
}

/// One page of the open-PR list. `has_more` is `true` iff GitHub's `Link`
/// header on this response advertised `rel="next"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrPage {
    pub page: u32,
    pub items: Vec<PrSummary>,
    pub has_more: bool,
}

/// Fetch a single page of open PRs (page numbers are 1-based, matching
/// GitHub's API).
#[instrument(skip(client))]
pub async fn fetch_open_prs_page(
    client: &Client,
    repo: &RepoRef,
    page: u32,
) -> Result<PrPage, PullsError> {
    let path = format!(
        "/repos/{}/{}/pulls?state=open&sort=created&direction=desc&per_page=30&page={page}",
        repo.owner, repo.name
    );

    let res: Page<Vec<octocrab::models::pulls::PullRequest>> =
        client.get_json(&path).await.map_err(|e| match e {
            ApiError::NotFound => PullsError::NotFound(repo.slug()),
            other => PullsError::Api(other),
        })?;

    debug!(count = res.body.len(), has_more = res.has_next, %page, "got pr page");

    Ok(PrPage {
        page,
        items: res.body.into_iter().map(from_octocrab).collect(),
        has_more: res.has_next,
    })
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
