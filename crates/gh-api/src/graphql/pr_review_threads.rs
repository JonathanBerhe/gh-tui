//! PR review-threads query: read-only fetch of inline review comments.
//!
//! GitHub's `pullRequest.reviewThreads` connection groups comments by
//! file/line so the diff renderer can inject pseudo-lines beneath the
//! exact row they're anchored to. Write side (composing comments) is
//! Phase 7 work.

use chrono::{DateTime as ChronoDateTime, Utc};
use gh_core::pulls::{RepoRef, ReviewComment, ReviewThread};
use thiserror::Error;
use tracing::{debug, instrument};

use super::schema;
use crate::client::{ApiError, Client};

// ── Query types ────────────────────────────────────────────────────────────

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct PrReviewThreadsVariables {
    pub owner: String,
    pub name: String,
    pub number: i32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "Query",
    variables = "PrReviewThreadsVariables"
)]
pub struct PrReviewThreadsQuery {
    #[arguments(owner: $owner, name: $name)]
    pub repository: Option<Repository>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "Repository",
    variables = "PrReviewThreadsVariables"
)]
pub struct Repository {
    #[arguments(number: $number)]
    pub pull_request: Option<PullRequest>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "PullRequest")]
pub struct PullRequest {
    #[arguments(first: 100)]
    pub review_threads: PullRequestReviewThreadConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "PullRequestReviewThreadConnection"
)]
pub struct PullRequestReviewThreadConnection {
    pub nodes: Option<Vec<Option<PullRequestReviewThread>>>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "PullRequestReviewThread"
)]
pub struct PullRequestReviewThread {
    pub path: String,
    pub line: Option<i32>,
    pub original_line: Option<i32>,
    #[arguments(first: 50)]
    pub comments: PullRequestReviewCommentConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "PullRequestReviewCommentConnection"
)]
pub struct PullRequestReviewCommentConnection {
    pub nodes: Option<Vec<Option<PullRequestReviewComment>>>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "PullRequestReviewComment"
)]
pub struct PullRequestReviewComment {
    pub author: Option<Actor>,
    pub body: String,
    pub created_at: DateTime,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "Actor")]
pub struct Actor {
    pub login: String,
}

#[derive(cynic::Scalar, Debug, Clone)]
#[cynic(schema_module = "schema", graphql_type = "DateTime")]
pub struct DateTime(pub String);

// ── Fetcher ────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum PrReviewThreadsError {
    #[error("repo `{0}` not found")]
    RepoNotFound(String),
    #[error("PR #{number} not found in `{repo}`")]
    PrNotFound { repo: String, number: u64 },
    #[error("api error: {0}")]
    Api(#[from] ApiError),
}

#[instrument(skip(client))]
pub async fn fetch_pr_review_threads(
    client: &Client,
    repo: &RepoRef,
    number: u64,
) -> Result<Vec<ReviewThread>, PrReviewThreadsError> {
    let vars = PrReviewThreadsVariables {
        owner: repo.owner.clone(),
        name: repo.name.clone(),
        number: i32::try_from(number).unwrap_or(i32::MAX),
    };

    let resp: PrReviewThreadsQuery = client.graphql::<PrReviewThreadsQuery, _>(vars).await?;

    let pr = resp
        .repository
        .ok_or_else(|| PrReviewThreadsError::RepoNotFound(repo.slug()))?
        .pull_request
        .ok_or_else(|| PrReviewThreadsError::PrNotFound {
            repo: repo.slug(),
            number,
        })?;

    let threads = pr
        .review_threads
        .nodes
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .map(map_thread)
        .collect::<Vec<_>>();

    debug!(count = threads.len(), %number, "got review threads");
    Ok(threads)
}

fn map_thread(t: PullRequestReviewThread) -> ReviewThread {
    let comments = t
        .comments
        .nodes
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .map(map_comment)
        .collect();
    ReviewThread {
        path: t.path,
        line: t.line.and_then(|n| u32::try_from(n).ok()),
        original_line: t.original_line.and_then(|n| u32::try_from(n).ok()),
        comments,
    }
}

fn map_comment(c: PullRequestReviewComment) -> ReviewComment {
    ReviewComment {
        author: c
            .author
            .map(|a| a.login)
            .unwrap_or_else(|| "<ghost>".into()),
        body: c.body,
        created_at: c
            .created_at
            .0
            .parse::<ChronoDateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now()),
    }
}
