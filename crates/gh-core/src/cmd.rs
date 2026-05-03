//! Commands emitted by reducers; the binary's worker layer runs them.

use crate::pulls::RepoRef;

#[derive(Debug, Clone)]
pub enum Cmd {
    /// Detect the active GitHub auth context (token + host + user).
    AuthenticateFromGh,
    /// Shell out to `gh repo view` to figure out the repo from cwd.
    ResolveRepoFromCwd,
    /// Fetch a single page of open PRs (1-based page index).
    FetchPrPage { repo: RepoRef, page: u32 },
    /// Fetch the detail for a single PR.
    FetchPrDetail { repo: RepoRef, number: u64 },
    /// Fetch the per-file diff for a single PR (REST `/pulls/{n}/files`).
    FetchPrDiff { repo: RepoRef, number: u64 },
}
