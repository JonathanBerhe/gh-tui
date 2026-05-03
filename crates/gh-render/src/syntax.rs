//! Tree-sitter syntax highlighting for code displayed in the TUI.
//!
//! Two public entry points:
//! - [`detect`] maps a path's extension to a [`Lang`] (currently `Rust` /
//!   `Plain`; more languages add as users hit them).
//! - [`highlight`] takes a `Lang` and source text, returns one
//!   `Vec<Span<'static>>` per source line. Plaintext fallback is silent and
//!   never panics — `Lang::Plain`, an unknown extension, an unparseable
//!   query, or an internal tree-sitter error all degrade to dim text.
//!
//! `HighlightConfiguration` is loaded once per language via `OnceLock`, so
//! repeated highlights pay only the per-call `Highlighter` cost.

use std::sync::OnceLock;

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

/// Language tag returned by [`detect`] and consumed by [`highlight`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Plain,
}

/// The highlight names we ask `HighlightConfiguration::configure` to surface.
/// Order matters — each `Highlight(idx)` event indexes into this slice.
const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "label",
    "number",
    "operator",
    "property",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

/// Detect a [`Lang`] from a file path's extension. Returns [`Lang::Plain`]
/// for unknown or extensionless paths.
#[must_use]
pub fn detect(path: &str) -> Lang {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    match ext {
        "rs" => Lang::Rust,
        _ => Lang::Plain,
    }
}

/// Highlight `source` and return one `Vec<Span<'static>>` per line. Lines
/// are split on `\n`; the trailing empty line is preserved if `source` ends
/// with a newline (so callers can index by 0-based line number).
///
/// This function never panics. On any tree-sitter or query failure it falls
/// back to a single dim-text span per line.
#[must_use]
pub fn highlight(lang: Lang, source: &str) -> Vec<Vec<Span<'static>>> {
    if matches!(lang, Lang::Plain) {
        return plain_lines(source);
    }
    let Some(config) = config_for(lang) else {
        return plain_lines(source);
    };

    let mut highlighter = Highlighter::new();
    let Ok(events) = highlighter.highlight(config, source.as_bytes(), None, |_| None) else {
        return plain_lines(source);
    };

    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = Vec::new();

    for event in events {
        let Ok(event) = event else { continue };
        match event {
            HighlightEvent::Source { start, end } => {
                let style = style_stack.last().copied().unwrap_or_default();
                let Some(text) = source.get(start..end) else {
                    continue;
                };
                let mut chunks = text.split('\n');
                if let Some(first) = chunks.next() {
                    if !first.is_empty() {
                        current.push(Span::styled(first.to_string(), style));
                    }
                    for chunk in chunks {
                        lines.push(std::mem::take(&mut current));
                        if !chunk.is_empty() {
                            current.push(Span::styled(chunk.to_string(), style));
                        }
                    }
                }
            }
            HighlightEvent::HighlightStart(Highlight(idx)) => {
                let name = HIGHLIGHT_NAMES.get(idx).copied().unwrap_or("");
                style_stack.push(style_for(name));
            }
            HighlightEvent::HighlightEnd => {
                style_stack.pop();
            }
        }
    }
    lines.push(current);
    lines
}

fn plain_lines(source: &str) -> Vec<Vec<Span<'static>>> {
    let style = Style::default().fg(Color::Gray);
    source
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                Vec::new()
            } else {
                vec![Span::styled(line.to_string(), style)]
            }
        })
        .collect()
}

/// Map a highlight-name token to a ratatui `Style`. Names not in the table
/// inherit a plain `Color::Gray` style. Conservative palette — easy to swap
/// once we ship themes.
fn style_for(name: &str) -> Style {
    let base = Style::default().fg(Color::Gray);
    match name {
        "comment" => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
        n if n.starts_with("keyword") => Style::default().fg(Color::Magenta),
        n if n.starts_with("string") => Style::default().fg(Color::Yellow),
        n if n.starts_with("type") => Style::default().fg(Color::Blue),
        n if n.starts_with("function") => Style::default().fg(Color::Cyan),
        n if n.starts_with("constant") => Style::default().fg(Color::LightYellow),
        "number" => Style::default().fg(Color::LightYellow),
        "attribute" | "tag" => Style::default().fg(Color::LightBlue),
        "label" => Style::default().fg(Color::Magenta),
        _ => base,
    }
}

fn config_for(lang: Lang) -> Option<&'static HighlightConfiguration> {
    static RUST: OnceLock<Option<HighlightConfiguration>> = OnceLock::new();
    match lang {
        Lang::Plain => None,
        Lang::Rust => RUST
            .get_or_init(|| {
                let mut cfg = HighlightConfiguration::new(
                    tree_sitter_rust::LANGUAGE.into(),
                    "rust",
                    tree_sitter_rust::HIGHLIGHTS_QUERY,
                    tree_sitter_rust::INJECTIONS_QUERY,
                    "",
                )
                .ok()?;
                cfg.configure(HIGHLIGHT_NAMES);
                Some(cfg)
            })
            .as_ref(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn detect_extension_to_lang() {
        assert_eq!(detect("src/foo.rs"), Lang::Rust);
        assert_eq!(detect("Cargo.toml"), Lang::Plain);
        assert_eq!(detect("README"), Lang::Plain);
        assert_eq!(detect(""), Lang::Plain);
    }

    #[test]
    fn highlight_unknown_falls_back_to_plain() {
        let out = highlight(Lang::Plain, "hello world\nsecond");
        assert_eq!(out.len(), 2);
        // single span per line, plain styling
        assert_eq!(out[0].len(), 1);
        assert_eq!(out[0][0].content, "hello world");
        assert_eq!(out[1][0].content, "second");
    }

    #[test]
    fn highlight_empty_source_yields_one_empty_line() {
        let out = highlight(Lang::Rust, "");
        assert_eq!(out.len(), 1);
        assert!(out[0].is_empty());
    }

    #[test]
    fn highlight_rust_basic_tokens() {
        let src = "fn main() { let x = 42; }";
        let out = highlight(Lang::Rust, src);
        assert_eq!(out.len(), 1, "single-line input");
        // join all spans on the line; should equal source
        let joined: String = out[0]
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(joined, src);
        // Spans must include at least one styled keyword (magenta).
        let has_keyword = out[0].iter().any(|s| s.style.fg == Some(Color::Magenta));
        assert!(has_keyword, "expected at least one magenta keyword span");
    }

    #[test]
    fn highlight_preserves_blank_lines_in_multiline_source() {
        let src = "fn a() {}\n\nfn b() {}";
        let out = highlight(Lang::Rust, src);
        assert_eq!(out.len(), 3, "expected three lines including blank");
        assert!(out[1].is_empty(), "middle line is blank");
    }
}
