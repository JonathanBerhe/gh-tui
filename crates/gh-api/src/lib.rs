//! GitHub API client for `gh-tui`.
//!
//! Phase 3 adds the REST PR list path: a thin `Client` over `octocrab`,
//! repo resolution from cwd via `gh repo view`, and the open-PR list call.
//! ETag caching, rate-limit tracking, and stream pagination land in
//! subsequent PRs.
//!
//! Depends on `gh-core` only for shared types. GraphQL queries will live
//! under `src/graphql/queries/`, compiled at build time.

pub mod auth;
pub mod client;
pub mod pulls;
pub mod repo;

pub use client::{Client, ClientError};
pub use pulls::{list_open_prs, PullsError};
pub use repo::{resolve_from_cwd, RepoResolveError};
