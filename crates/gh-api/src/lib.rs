//! GitHub API client for `gh-tui`.
//!
//! REST and GraphQL paths share one [`Client`] over `reqwest`:
//!
//! - REST GETs go through `Client::get_json` with ETag-conditional caching
//!   and `x-ratelimit-*` header surfacing.
//! - GraphQL POSTs go through `Client::graphql<Q, V>` (cynic + reqwest).
//!   No GraphQL caching for now (GitHub doesn't ETag GraphQL responses).
//!
//! Depends on `gh-core` only for shared types. Schema is vendored at
//! `crates/gh-api/schema.graphql` and registered at build time via
//! `build.rs`.

pub mod auth;
pub mod cache;
pub mod cache_path;
pub mod client;
pub mod graphql;
pub mod pr_files;
pub mod pulls;
pub mod repo;

pub use cache::{CacheError, CachedEntry, EtagCache};
pub use cache_path::cache_db_path;
pub use client::{ApiError, Client, ClientError, Page};
pub use graphql::{fetch_pr_detail, pr_detail::PrDetailError};
pub use pr_files::{fetch_pr_files, PrFilesError};
pub use pulls::{fetch_open_prs_page, PrPage, PullsError};
pub use repo::{resolve_from_cwd, RepoResolveError};
