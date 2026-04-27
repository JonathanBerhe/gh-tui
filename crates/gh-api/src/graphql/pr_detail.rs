//! PR detail query: one round-trip for the metadata the detail screen needs.
//!
//! Maps GitHub's GraphQL types to `gh_core::PrDetail` at the seam.

use gh_core::pulls::{Mergeable, PrDetail, PrState, RepoRef, ReviewDecision};
use thiserror::Error;
use tracing::{debug, instrument};

use super::schema;
use crate::client::{ApiError, Client};

// `super::schema` (a sibling module) hosts cynic's schema bindings via
// `cynic::use_schema!`. The derives reference `schema::*` types in their
// generated code; the `use super::schema;` above puts the module in scope.

// ── Query types (cynic-derived) ────────────────────────────────────────────

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct PrDetailVariables {
    pub owner: String,
    pub name: String,
    pub number: i32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "Query",
    variables = "PrDetailVariables"
)]
pub struct PrDetailQuery {
    #[arguments(owner: $owner, name: $name)]
    pub repository: Option<Repository>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "Repository",
    variables = "PrDetailVariables"
)]
pub struct Repository {
    #[arguments(number: $number)]
    pub pull_request: Option<PullRequest>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "PullRequest")]
pub struct PullRequest {
    pub number: i32,
    pub title: String,
    pub body: String,
    pub state: PullRequestState,
    pub is_draft: bool,
    pub mergeable: MergeableState,
    pub author: Option<Actor>,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub additions: i32,
    pub deletions: i32,
    pub review_decision: Option<PullRequestReviewDecision>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "Actor")]
pub struct Actor {
    pub login: String,
}

#[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
#[cynic(schema_path = "schema.graphql", graphql_type = "PullRequestState")]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
#[cynic(schema_path = "schema.graphql", graphql_type = "MergeableState")]
pub enum MergeableState {
    Mergeable,
    Conflicting,
    Unknown,
}

#[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "PullRequestReviewDecision"
)]
pub enum PullRequestReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

// ── Fetcher ────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum PrDetailError {
    #[error("repo `{0}` not found")]
    RepoNotFound(String),
    #[error("PR #{number} not found in {repo}")]
    PrNotFound { repo: String, number: u64 },
    #[error("api error: {0}")]
    Api(#[from] ApiError),
}

#[instrument(skip(client))]
pub async fn fetch_pr_detail(
    client: &Client,
    repo: &RepoRef,
    number: u64,
) -> Result<PrDetail, PrDetailError> {
    let vars = PrDetailVariables {
        owner: repo.owner.clone(),
        name: repo.name.clone(),
        number: i32::try_from(number).unwrap_or(i32::MAX),
    };

    let resp: PrDetailQuery = client.graphql::<PrDetailQuery, _>(vars).await?;

    let pr = resp
        .repository
        .ok_or_else(|| PrDetailError::RepoNotFound(repo.slug()))?
        .pull_request
        .ok_or_else(|| PrDetailError::PrNotFound {
            repo: repo.slug(),
            number,
        })?;

    debug!(number = pr.number, title = %pr.title, "got pr detail");
    Ok(map_pr(pr))
}

fn map_pr(p: PullRequest) -> PrDetail {
    PrDetail {
        number: u64::try_from(p.number).unwrap_or(0),
        title: p.title,
        body: p.body,
        state: match p.state {
            PullRequestState::Open => PrState::Open,
            PullRequestState::Closed => PrState::Closed,
            PullRequestState::Merged => PrState::Merged,
        },
        draft: p.is_draft,
        mergeable: match p.mergeable {
            MergeableState::Mergeable => Mergeable::Yes,
            MergeableState::Conflicting => Mergeable::No,
            MergeableState::Unknown => Mergeable::Unknown,
        },
        author: p
            .author
            .map(|a| a.login)
            .unwrap_or_else(|| "<unknown>".to_string()),
        head_ref: p.head_ref_name,
        base_ref: p.base_ref_name,
        additions: u32::try_from(p.additions).unwrap_or(0),
        deletions: u32::try_from(p.deletions).unwrap_or(0),
        review_decision: match p.review_decision {
            Some(PullRequestReviewDecision::Approved) => ReviewDecision::Approved,
            Some(PullRequestReviewDecision::ChangesRequested) => ReviewDecision::ChangesRequested,
            Some(PullRequestReviewDecision::ReviewRequired) => ReviewDecision::ReviewRequired,
            None => ReviewDecision::None,
        },
    }
}
