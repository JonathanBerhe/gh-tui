//! Pure rendering helpers for `gh-tui`: markdown (`pulldown-cmark`),
//! diff (`similar`), syntax highlighting (`tree-sitter`), image protocol
//! handling (`ratatui-image`), and Mermaid (shell-out to `mmdc`).
//!
//! Phase 1 is an empty placeholder — real renderers arrive in Phases 4–6.
//! This crate takes `&State` slices from `gh-core` and produces
//! `ratatui::text::Line` / widget placements; it does not own widget state.
