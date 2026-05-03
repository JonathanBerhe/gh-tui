//! Pure rendering helpers for `gh-tui`: markdown (`pulldown-cmark`),
//! diff (`similar` — Phase 5), syntax highlighting (`tree-sitter` — Phase 5),
//! image protocol handling (`ratatui-image` — Phase 6), and Mermaid (shell-out
//! to `mmdc` — Phase 6).
//!
//! Phase 4 ships markdown only.

pub mod diff;
pub mod markdown;
pub mod syntax;

pub use diff::{file_line_offsets, render as render_diff};
pub use markdown::render as render_markdown;
pub use syntax::{detect as detect_lang, highlight, Lang};
