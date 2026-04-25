//! Commands emitted by reducers; the binary's worker layer runs them.

use crate::pulls::RepoRef;

#[derive(Debug, Clone)]
pub enum Cmd {
    /// Detect the active GitHub auth context (token + host + user).
    AuthenticateFromGh,
    /// Shell out to `gh repo view` to figure out the repo from cwd.
    ResolveRepoFromCwd,
    /// Fetch the open-PR list for the given repo.
    FetchPrList { repo: RepoRef },
}
