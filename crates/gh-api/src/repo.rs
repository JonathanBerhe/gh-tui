//! Resolve the active GitHub repo for a `gh tui` invocation.
//!
//! Argv parsing is in `gh_core::RepoRef::parse`. This module covers the
//! cwd-fallback path that shells out to `gh repo view`.

use gh_core::RepoRef;
use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, instrument};

#[derive(Debug, Error)]
pub enum RepoResolveError {
    #[error("`gh` is not installed")]
    GhMissing,
    #[error("not in a GitHub repo (cwd has no GitHub remote)")]
    NotInRepo,
    #[error("`gh repo view` failed: {0}")]
    GhFailed(String),
    #[error("could not parse `{0}` as `owner/name`")]
    ParseFailed(String),
}

/// Shell out to `gh repo view --json owner,name --jq '.owner.login + "/" + .name'`
/// to discover the active repo from cwd. Returns `RepoResolveError::NotInRepo`
/// if cwd has no GitHub remote.
#[instrument]
pub async fn resolve_from_cwd() -> Result<RepoRef, RepoResolveError> {
    let output = Command::new("gh")
        .args([
            "repo",
            "view",
            "--json",
            "owner,name",
            "--jq",
            r#".owner.login + "/" + .name"#,
        ])
        .output()
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => RepoResolveError::GhMissing,
            other => RepoResolveError::GhFailed(format!("spawn: {other}")),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // gh's "no GitHub remote found" message is the canonical not-in-repo signal.
        if stderr.contains("no git remotes") || stderr.contains("not a github") {
            return Err(RepoResolveError::NotInRepo);
        }
        return Err(RepoResolveError::GhFailed(if stderr.is_empty() {
            format!("exit {}", output.status)
        } else {
            stderr
        }));
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!(raw = %raw, "gh repo view returned");

    RepoRef::parse(&raw).map_err(|_| RepoResolveError::ParseFailed(raw))
}
