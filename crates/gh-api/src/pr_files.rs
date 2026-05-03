//! REST: list the changed files for a single pull request.
//!
//! Wraps `GET /repos/{owner}/{repo}/pulls/{n}/files`. The response includes
//! the raw unified-diff `patch` text per file, which `gh-render::diff` later
//! parses for display. ETag-cached via `Client::get_json` (the resource is
//! mutable while the PR is open, so a time-bounded ETag fits — same model as
//! the PR list).

use gh_core::{FilePatch, PatchStatus, RepoRef};
use serde::Deserialize;
use thiserror::Error;
use tracing::{debug, instrument, warn};

use crate::client::{ApiError, Client};

/// Per-request page size. GitHub allows up to 100; smaller values would just
/// mean more round-trips on PRs with many files.
const PER_PAGE: u32 = 100;
/// Hard cap on the number of file pages we will fetch for a single PR.
/// 10 × 100 = 1000 files; PRs larger than this are pathological and we'd
/// rather render a truncated view than block the UI on minute-long fetches.
const MAX_PAGES: u32 = 10;

#[derive(Debug, Error)]
pub enum PrFilesError {
    #[error("PR #{0} not found or no access")]
    NotFound(u64),
    #[error("api error: {0}")]
    Api(#[from] ApiError),
}

/// Fetch the per-file diff for a single PR. Loops over pages of `PER_PAGE`
/// files (GitHub's max) until `Link: rel="next"` is absent, capped at
/// `MAX_PAGES` to bound worst-case work. PRs that exceed the cap render a
/// truncated view and emit a `tracing::warn!`.
#[instrument(skip(client))]
pub async fn fetch_pr_files(
    client: &Client,
    repo: &RepoRef,
    number: u64,
) -> Result<Vec<FilePatch>, PrFilesError> {
    let mut all: Vec<FilePatch> = Vec::new();
    for page_n in 1..=MAX_PAGES {
        let path = format!(
            "/repos/{}/{}/pulls/{number}/files?per_page={PER_PAGE}&page={page_n}",
            repo.owner, repo.name
        );
        let page = client
            .get_json::<Vec<RawPrFile>>(&path)
            .await
            .map_err(|e| match e {
                ApiError::NotFound => PrFilesError::NotFound(number),
                other => PrFilesError::Api(other),
            })?;

        let count = page.body.len();
        all.extend(page.body.into_iter().map(into_file_patch));
        debug!(page = page_n, count, total = all.len(), %number, "got pr files page");

        if !page.has_next {
            return Ok(all);
        }
        if page_n == MAX_PAGES {
            warn!(
                cap = MAX_PAGES,
                so_far = all.len(),
                %number,
                "PR has more than {} pages of files; truncating diff",
                MAX_PAGES
            );
            return Ok(all);
        }
    }
    Ok(all)
}

#[derive(Debug, Deserialize)]
struct RawPrFile {
    sha: String,
    filename: String,
    #[serde(default)]
    previous_filename: Option<String>,
    status: String,
    #[serde(default)]
    additions: u32,
    #[serde(default)]
    deletions: u32,
    #[serde(default)]
    patch: Option<String>,
}

fn into_file_patch(raw: RawPrFile) -> FilePatch {
    FilePatch {
        path: raw.filename,
        previous_path: raw.previous_filename,
        status: parse_status(&raw.status),
        additions: raw.additions,
        deletions: raw.deletions,
        patch: raw.patch.filter(|p| !p.is_empty()),
        blob_sha: raw.sha,
    }
}

fn parse_status(s: &str) -> PatchStatus {
    match s {
        "added" => PatchStatus::Added,
        "modified" => PatchStatus::Modified,
        "removed" => PatchStatus::Removed,
        "renamed" => PatchStatus::Renamed,
        "copied" => PatchStatus::Copied,
        "changed" => PatchStatus::Changed,
        "unchanged" => PatchStatus::Unchanged,
        // GitHub's docs list the seven values above. Any other string is
        // forward-compat — log the surprise and treat as Modified so the
        // UI still renders. Useful signal if GitHub ever extends the enum.
        other => {
            warn!(value = %other, "unknown PullRequestFile.status; treating as modified");
            PatchStatus::Modified
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn status_parser_covers_known_values() {
        assert_eq!(parse_status("added"), PatchStatus::Added);
        assert_eq!(parse_status("modified"), PatchStatus::Modified);
        assert_eq!(parse_status("removed"), PatchStatus::Removed);
        assert_eq!(parse_status("renamed"), PatchStatus::Renamed);
        assert_eq!(parse_status("copied"), PatchStatus::Copied);
        assert_eq!(parse_status("changed"), PatchStatus::Changed);
        assert_eq!(parse_status("unchanged"), PatchStatus::Unchanged);
    }

    #[test]
    fn status_parser_falls_back_to_modified() {
        assert_eq!(parse_status("bogus"), PatchStatus::Modified);
    }

    #[test]
    fn empty_patch_string_normalises_to_none() {
        let raw = RawPrFile {
            sha: "abc".into(),
            filename: "f".into(),
            previous_filename: None,
            status: "modified".into(),
            additions: 0,
            deletions: 0,
            patch: Some(String::new()),
        };
        let fp = into_file_patch(raw);
        assert_eq!(fp.patch, None);
    }
}
