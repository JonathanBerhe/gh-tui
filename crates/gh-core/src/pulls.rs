//! Pull-request domain types: a repo identifier and a renderable summary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// `owner/name` reference to a GitHub repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ParseRepoRefError {
    #[error("expected `owner/name`, got `{0}`")]
    BadShape(String),
    #[error("empty owner or name in `{0}`")]
    EmptySegment(String),
    #[error("invalid character in `{0}`")]
    InvalidChar(String),
}

impl RepoRef {
    /// Parse a `owner/name` slug. Rejects empty segments, embedded slashes,
    /// and `.` / `..` segments.
    pub fn parse(s: &str) -> Result<Self, ParseRepoRefError> {
        let s = s.trim();
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(ParseRepoRefError::BadShape(s.to_string()));
        }
        let (owner, name) = (parts[0].trim(), parts[1].trim());
        if owner.is_empty() || name.is_empty() {
            return Err(ParseRepoRefError::EmptySegment(s.to_string()));
        }
        if owner == "." || owner == ".." || name == "." || name == ".." {
            return Err(ParseRepoRefError::InvalidChar(s.to_string()));
        }
        if owner.contains(char::is_whitespace) || name.contains(char::is_whitespace) {
            return Err(ParseRepoRefError::InvalidChar(s.to_string()));
        }
        Ok(Self {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    /// Reverse of [`Self::parse`]: produces `owner/name`.
    #[must_use]
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// A pull request as it appears in the list view. Lossy projection of
/// GitHub's full PR — the binary maps `octocrab::PullRequest -> PrSummary`
/// at the edge of `gh-api`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub draft: bool,
    pub head_ref: String,
    pub base_ref: String,
    pub comments: u32,
    pub created_at: DateTime<Utc>,
    pub additions: u32,
    pub deletions: u32,
}

/// Open / closed / merged. Maps from GitHub's `PullRequestState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

impl PrState {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Closed => "CLOSED",
            Self::Merged => "MERGED",
        }
    }
}

/// Whether GitHub thinks the PR can be merged. `Unknown` is what we get when
/// GitHub is still computing the answer; the UI treats it as a transient
/// "checking…" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mergeable {
    Yes,
    No,
    Unknown,
}

/// Aggregated review status for a PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
    /// No required-review configuration, or no decision yet.
    None,
}

impl ReviewDecision {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Approved => "APPROVED",
            Self::ChangesRequested => "CHANGES_REQUESTED",
            Self::ReviewRequired => "REVIEW_REQUIRED",
            Self::None => "",
        }
    }
}

/// Compact view of a single review for the PR detail screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSummary {
    pub author: String,
    pub state: ReviewState,
    pub body_excerpt: String,
    pub submitted_at: DateTime<Utc>,
}

/// Per-review state. Maps from `PullRequestReviewState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewState {
    Pending,
    Commented,
    Approved,
    ChangesRequested,
    Dismissed,
}

impl ReviewState {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Commented => "COMMENTED",
            Self::Approved => "APPROVED",
            Self::ChangesRequested => "CHANGES_REQUESTED",
            Self::Dismissed => "DISMISSED",
        }
    }
}

/// Aggregate status of CI checks for the head commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksSummary {
    pub state: ChecksState,
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
}

impl ChecksSummary {
    #[must_use]
    pub fn total(&self) -> u32 {
        self.passing + self.failing + self.pending
    }

    /// `true` when the PR has no checks configured at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.total() == 0 && matches!(self.state, ChecksState::Unknown)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChecksState {
    Success,
    Failure,
    Pending,
    /// No status checks have run / no rollup yet.
    Unknown,
}

/// Full PR detail. Lossy projection of GitHub's GraphQL `PullRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrDetail {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: PrState,
    pub draft: bool,
    pub mergeable: Mergeable,
    pub author: String,
    pub head_ref: String,
    pub base_ref: String,
    pub additions: u32,
    pub deletions: u32,
    pub review_decision: ReviewDecision,
    pub reviews: Vec<ReviewSummary>,
    pub checks: ChecksSummary,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parse_valid() {
        let r = RepoRef::parse("JonathanBerhe/gh-tui").unwrap();
        assert_eq!(r.owner, "JonathanBerhe");
        assert_eq!(r.name, "gh-tui");
    }

    #[test]
    fn parse_trims_whitespace() {
        let r = RepoRef::parse("  cli/cli  ").unwrap();
        assert_eq!(r.slug(), "cli/cli");
    }

    #[test]
    fn parse_rejects_no_slash() {
        assert!(matches!(
            RepoRef::parse("foobar"),
            Err(ParseRepoRefError::BadShape(_))
        ));
    }

    #[test]
    fn parse_rejects_too_many_slashes() {
        assert!(matches!(
            RepoRef::parse("a/b/c"),
            Err(ParseRepoRefError::BadShape(_))
        ));
    }

    #[test]
    fn parse_rejects_empty_owner() {
        assert!(matches!(
            RepoRef::parse("/name"),
            Err(ParseRepoRefError::EmptySegment(_))
        ));
    }

    #[test]
    fn parse_rejects_empty_name() {
        assert!(matches!(
            RepoRef::parse("owner/"),
            Err(ParseRepoRefError::EmptySegment(_))
        ));
    }

    #[test]
    fn parse_rejects_dot_segments() {
        assert!(matches!(
            RepoRef::parse("./.."),
            Err(ParseRepoRefError::InvalidChar(_))
        ));
        assert!(matches!(
            RepoRef::parse("owner/.."),
            Err(ParseRepoRefError::InvalidChar(_))
        ));
    }

    #[test]
    fn parse_rejects_whitespace_in_segments() {
        assert!(matches!(
            RepoRef::parse("own er/name"),
            Err(ParseRepoRefError::InvalidChar(_))
        ));
    }

    #[test]
    fn slug_round_trip() {
        let r = RepoRef::parse("a/b").unwrap();
        assert_eq!(RepoRef::parse(&r.slug()).unwrap(), r);
    }
}
