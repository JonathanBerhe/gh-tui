//! Pure rendering helpers for `gh-tui`: markdown (`pulldown-cmark`),
//! diff (`similar` — Phase 5), syntax highlighting (`tree-sitter` — Phase 5),
//! image protocol handling (`ratatui-image` — Phase 6), and Mermaid (shell-out
//! to `mmdc` — Phase 6).
//!
//! Phase 4 ships markdown only.

pub mod markdown;

pub use markdown::render as render_markdown;
