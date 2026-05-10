//! Pure rendering helpers for `gh-tui`: markdown (`pulldown-cmark`),
//! diff (`similar` — Phase 5), syntax highlighting (`tree-sitter` — Phase 5),
//! image protocol handling (`ratatui-image` — Phase 6), and Mermaid (shell-out
//! to `mmdc` — Phase 6).
//!
//! Phase 4 ships markdown only.

pub mod diff;
pub mod markdown;
pub mod syntax;

pub use diff::{
    file_line_offsets, render as render_diff, split::render as render_diff_split, total_diff_lines,
};
pub use markdown::{
    image_urls as markdown_image_urls, render as render_markdown,
    render_chunks as render_markdown_chunks, BodyChunk, IMAGE_HEIGHT_ROWS, MERMAID_HEIGHT_ROWS,
};
pub use syntax::{detect as detect_lang, highlight, Lang};
