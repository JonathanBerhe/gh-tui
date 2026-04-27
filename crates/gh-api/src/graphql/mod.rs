//! GraphQL queries built with [`cynic`].
//!
//! The schema is vendored at `crates/gh-api/schema.graphql` and loaded inline
//! at macro-expansion time. Queries are Rust structs with
//! `#[derive(QueryFragment)]` referencing the `schema` module's types.

pub mod pr_detail;

pub use pr_detail::fetch_pr_detail;

/// Schema bindings used by every query in this module's siblings. The path
/// is relative to the crate root.
pub mod schema {
    cynic::use_schema!("schema.graphql");
}
