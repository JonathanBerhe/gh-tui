//! GitHub API client for `gh-tui`.
//!
//! Phase 3 builds: `Client` (a reqwest-backed wrapper that does conditional
//! GETs against an ETag cache), `repo::resolve_from_cwd` (`gh repo view`
//! shell-out), and `pulls::list_open_prs` (eager first page; streaming in
//! the next PR).
//!
//! Depends on `gh-core` only for shared types. GraphQL queries will live
//! under `src/graphql/queries/`, compiled at build time.

pub mod auth;
pub mod cache;
pub mod cache_path;
pub mod client;
pub mod pulls;
pub mod repo;

pub use cache::{CacheError, CachedEntry, EtagCache};
pub use cache_path::cache_db_path;
pub use client::{ApiError, Client, ClientError, Page};
pub use pulls::{fetch_open_prs_page, PrPage, PullsError};
pub use repo::{resolve_from_cwd, RepoResolveError};
