//! PR detail query: one round-trip for the metadata the detail screen needs.
//!
//! Maps GitHub's GraphQL types to `gh_core::PrDetail` at the seam.

use chrono::{DateTime as ChronoDateTime, Utc};
use gh_core::pulls::{
    ChecksState, ChecksSummary, Mergeable, PrDetail, PrState, RepoRef, ReviewDecision, ReviewState,
    ReviewSummary,
};
use thiserror::Error;
use tracing::{debug, instrument, warn};

use super::schema;
use crate::client::{ApiError, Client};

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
    #[arguments(first: 20)]
    pub latest_reviews: Option<PullRequestReviewConnection>,
    #[arguments(last: 1)]
    pub commits: PullRequestCommitConnection,
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

// ── Reviews ────────────────────────────────────────────────────────────────

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "PullRequestReviewConnection"
)]
pub struct PullRequestReviewConnection {
    pub nodes: Option<Vec<Option<PullRequestReview>>>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "PullRequestReview")]
pub struct PullRequestReview {
    pub author: Option<Actor>,
    pub state: PullRequestReviewState,
    pub body: String,
    pub submitted_at: Option<DateTime>,
}

#[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "PullRequestReviewState"
)]
pub enum PullRequestReviewState {
    Pending,
    Commented,
    Approved,
    ChangesRequested,
    Dismissed,
}

// ── Status checks ──────────────────────────────────────────────────────────

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "PullRequestCommitConnection"
)]
pub struct PullRequestCommitConnection {
    pub nodes: Option<Vec<Option<PullRequestCommit>>>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "PullRequestCommit")]
pub struct PullRequestCommit {
    pub commit: Commit,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "Commit")]
pub struct Commit {
    pub status_check_rollup: Option<StatusCheckRollup>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "StatusCheckRollup")]
pub struct StatusCheckRollup {
    pub state: StatusState,
    #[arguments(first: 50)]
    pub contexts: StatusCheckRollupContextConnection,
}

#[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
#[cynic(schema_path = "schema.graphql", graphql_type = "StatusState")]
pub enum StatusState {
    Expected,
    Error,
    Failure,
    Pending,
    Success,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "StatusCheckRollupContextConnection"
)]
pub struct StatusCheckRollupContextConnection {
    pub nodes: Option<Vec<Option<StatusCheckRollupContext>>>,
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(
    schema_path = "schema.graphql",
    graphql_type = "StatusCheckRollupContext"
)]
pub enum StatusCheckRollupContext {
    CheckRun(CheckRun),
    StatusContext(StatusContext),
    #[cynic(fallback)]
    Unknown,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "CheckRun")]
pub struct CheckRun {
    pub conclusion: Option<CheckConclusionState>,
}

#[derive(cynic::Enum, Debug, Clone, Copy, PartialEq, Eq)]
#[cynic(schema_path = "schema.graphql", graphql_type = "CheckConclusionState")]
pub enum CheckConclusionState {
    ActionRequired,
    TimedOut,
    Cancelled,
    Failure,
    Success,
    Neutral,
    Skipped,
    Stale,
    StartupFailure,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "schema.graphql", graphql_type = "StatusContext")]
pub struct StatusContext {
    pub state: StatusState,
}

// ── Custom scalars ─────────────────────────────────────────────────────────

#[derive(cynic::Scalar, Debug, Clone)]
#[cynic(schema_module = "schema", graphql_type = "DateTime")]
pub struct DateTime(pub String);

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
    let reviews = map_reviews(p.latest_reviews);
    let checks = map_checks(&p.commits);

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
        reviews,
        checks,
    }
}

fn map_reviews(conn: Option<PullRequestReviewConnection>) -> Vec<ReviewSummary> {
    let Some(conn) = conn else { return Vec::new() };
    let Some(nodes) = conn.nodes else {
        return Vec::new();
    };
    nodes.into_iter().flatten().map(map_review).collect()
}

fn map_review(r: PullRequestReview) -> ReviewSummary {
    ReviewSummary {
        author: r
            .author
            .map(|a| a.login)
            .unwrap_or_else(|| "<unknown>".to_string()),
        state: match r.state {
            PullRequestReviewState::Pending => ReviewState::Pending,
            PullRequestReviewState::Commented => ReviewState::Commented,
            PullRequestReviewState::Approved => ReviewState::Approved,
            PullRequestReviewState::ChangesRequested => ReviewState::ChangesRequested,
            PullRequestReviewState::Dismissed => ReviewState::Dismissed,
        },
        body_excerpt: excerpt(&r.body, 200),
        submitted_at: r
            .submitted_at
            .and_then(|dt| dt.0.parse::<ChronoDateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now),
    }
}

fn excerpt(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(max).collect();
    format!("{head}…")
}

fn map_checks(conn: &PullRequestCommitConnection) -> ChecksSummary {
    let rollup = conn
        .nodes
        .as_ref()
        .and_then(|nodes| nodes.iter().flatten().next())
        .and_then(|c| c.commit.status_check_rollup.as_ref());

    let Some(rollup) = rollup else {
        return ChecksSummary {
            state: ChecksState::Unknown,
            passing: 0,
            failing: 0,
            pending: 0,
        };
    };

    let state = match rollup.state {
        StatusState::Success => ChecksState::Success,
        StatusState::Failure | StatusState::Error => ChecksState::Failure,
        StatusState::Pending | StatusState::Expected => ChecksState::Pending,
    };

    let mut passing = 0u32;
    let mut failing = 0u32;
    let mut pending = 0u32;

    let nodes = rollup
        .contexts
        .nodes
        .as_ref()
        .map(|v| v.iter().flatten().collect::<Vec<_>>())
        .unwrap_or_default();

    for ctx in nodes {
        match ctx {
            StatusCheckRollupContext::CheckRun(cr) => match cr.conclusion {
                Some(CheckConclusionState::Success | CheckConclusionState::Neutral) => {
                    passing += 1;
                }
                Some(
                    CheckConclusionState::Failure
                    | CheckConclusionState::TimedOut
                    | CheckConclusionState::Cancelled
                    | CheckConclusionState::ActionRequired
                    | CheckConclusionState::StartupFailure,
                ) => failing += 1,
                Some(CheckConclusionState::Skipped | CheckConclusionState::Stale) => {
                    // Don't count skipped/stale toward any bucket.
                }
                None => pending += 1,
            },
            StatusCheckRollupContext::StatusContext(sc) => match sc.state {
                StatusState::Success => passing += 1,
                StatusState::Failure | StatusState::Error => failing += 1,
                StatusState::Pending | StatusState::Expected => pending += 1,
            },
            StatusCheckRollupContext::Unknown => {
                warn!("unknown status check rollup context type");
            }
        }
    }

    ChecksSummary {
        state,
        passing,
        failing,
        pending,
    }
}
