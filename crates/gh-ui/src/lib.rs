//! `ratatui` widgets and screen composition for `gh-tui`. Consumes `&State`
//! from `gh-core` and the layout helpers from `gh-render`. **Never** calls
//! `gh-api` directly — all side effects flow through `Cmd` dispatch.

pub mod screens;
