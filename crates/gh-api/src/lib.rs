//! GitHub API client for `gh-tui`.
//!
//! Phase 1 only exposes the auth detector; REST (`octocrab`), GraphQL
//! (`cynic`), the request coalescer, ETag cache (`sqlx` + SQLite), and rate
//! limiting land in Phases 3–4. See the roadmap.
//!
//! Depends on `gh-core` only for shared types. GraphQL queries will live under
//! `src/graphql/queries/`, compiled at build time.

pub mod auth;
