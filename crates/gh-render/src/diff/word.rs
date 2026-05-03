//! Word-level intra-line diff for paired `-`/`+` lines in a hunk.
//!
//! When a change block has the same number of `-` and `+` lines, we pair
//! them 1:1 and use [`similar::TextDiff::from_words`] to highlight only the
//! *changed* words inside each pair. Unchanged words keep the line's base
//! foreground colour; changed words flip to a high-contrast bg pair so
//! reviewers can scan a line and see what moved at a glance.
//!
//! v1 trades syntax highlighting for word-level highlighting on paired
//! lines: the foreground is just `Color::Red` / `Color::Green`, no
//! tree-sitter colours. PR #5 (or a follow-up) can revisit by overlaying
//! word-bg on syntax-fg.

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};
use similar::{ChangeTag, TextDiff};

/// Compute word-level intra-line styling for a paired `(old, new)` line.
/// Returns the spans for the `-` line and the `+` line. Both lines share
/// the same word boundaries: an unchanged word appears on both sides; a
/// changed run appears as a `Delete` on the left and an `Insert` on the
/// right.
#[must_use]
pub fn intra_line_highlight(old: &str, new: &str) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let diff = TextDiff::from_words(old, new);

    let red_fg = Style::default().fg(Color::Red);
    let green_fg = Style::default().fg(Color::Green);
    let red_bg = Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(96, 0, 0))
        .add_modifier(Modifier::BOLD);
    let green_bg = Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(0, 96, 0))
        .add_modifier(Modifier::BOLD);

    let mut minus: Vec<Span<'static>> = Vec::new();
    let mut plus: Vec<Span<'static>> = Vec::new();

    for change in diff.iter_all_changes() {
        let value = change.value().to_string();
        match change.tag() {
            ChangeTag::Equal => {
                minus.push(Span::styled(value.clone(), red_fg));
                plus.push(Span::styled(value, green_fg));
            }
            ChangeTag::Delete => {
                minus.push(Span::styled(value, red_bg));
            }
            ChangeTag::Insert => {
                plus.push(Span::styled(value, green_bg));
            }
        }
    }

    (minus, plus)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn join(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn intra_line_highlight_finds_changed_words() {
        let (m, p) = intra_line_highlight("let x = 1;", "let y = 1;");
        // Round-trip text content equals the inputs.
        assert_eq!(join(&m), "let x = 1;");
        assert_eq!(join(&p), "let y = 1;");
        // The `x` token should carry the bold red bg; `y` the bold green bg.
        let m_changed = m
            .iter()
            .any(|s| s.content == "x" && s.style.add_modifier.contains(Modifier::BOLD));
        let p_changed = p
            .iter()
            .any(|s| s.content == "y" && s.style.add_modifier.contains(Modifier::BOLD));
        assert!(m_changed, "old 'x' must be highlighted");
        assert!(p_changed, "new 'y' must be highlighted");
        // Unchanged words ('let', '=', '1', ';', spaces) must NOT carry bold.
        let m_let = m.iter().find(|s| s.content == "let").unwrap();
        assert!(!m_let.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn identical_inputs_produce_no_highlights() {
        let (m, p) = intra_line_highlight("let x = 1;", "let x = 1;");
        assert!(m
            .iter()
            .all(|s| !s.style.add_modifier.contains(Modifier::BOLD)));
        assert!(p
            .iter()
            .all(|s| !s.style.add_modifier.contains(Modifier::BOLD)));
    }

    #[test]
    fn pure_addition_marks_only_plus_side() {
        let (m, p) = intra_line_highlight("", "new content");
        assert!(m.is_empty(), "no spans on an empty old side");
        assert_eq!(join(&p), "new content");
        // Every span on the plus side is a bold insert.
        assert!(p
            .iter()
            .all(|s| s.style.add_modifier.contains(Modifier::BOLD)));
    }
}
