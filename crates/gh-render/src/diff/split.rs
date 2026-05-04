//! Side-by-side renderer: emits the diff as `(left, right)` row pairs.
//!
//! Layout rules:
//! - File headers, stats, hunk headers, no-newline markers, context lines:
//!   identical content on both sides (orientation cue).
//! - Balanced paired `-`/`+` blocks: word-level highlighting; left holds the
//!   `-` spans, right the `+` spans (no prefix character — column position
//!   is the indicator).
//! - Unbalanced blocks: each `-` line emits `(line, filler)`, each `+` line
//!   emits `(filler, line)`.
//!
//! Like the unified path, syntax highlighting is applied to the post-image
//! (right column for context and unpaired adds). Pre-image / paired pairs
//! use the word-level palette.

use gh_core::{FilePatch, ReviewThread};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::{
    diff_omitted, file_header, file_stats, hunk_header_line, no_newline_line, reconstruct_after,
    threads, word,
};
use crate::syntax::{self, Lang};

/// Render a sequence of file patches into split-view row pairs. Inline
/// review threads (when supplied) are injected as `(filler, pseudo_line)`
/// pairs beneath the matching anchor row.
#[must_use]
pub fn render(
    files: &[FilePatch],
    review_threads: &[ReviewThread],
) -> Vec<(Line<'static>, Line<'static>)> {
    let mut rows: Vec<(Line<'static>, Line<'static>)> = Vec::new();
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            rows.push((Line::raw(""), Line::raw("")));
        }
        render_file(file, &mut rows);
    }
    threads::inject_inline_split(&mut rows, files, review_threads);
    rows
}

fn render_file(file: &FilePatch, rows: &mut Vec<(Line<'static>, Line<'static>)>) {
    let header = file_header(file);
    rows.push((header.clone(), header));
    let stats = file_stats(file);
    rows.push((stats.clone(), stats));

    let raw_patch = match &file.patch {
        Some(p) if !p.is_empty() => p,
        _ => {
            let placeholder = diff_omitted();
            rows.push((placeholder.clone(), placeholder));
            return;
        }
    };
    // Same tab-normalisation as the unified path.
    let patch = super::expand_tabs(raw_patch);

    let lang = syntax::detect(&file.path);
    let highlighted = if matches!(lang, Lang::Plain) {
        None
    } else {
        let after = reconstruct_after(&patch);
        Some(syntax::highlight(lang, &after))
    };

    let mut walker = SplitWalker::default();
    for raw in patch.lines() {
        walker.consume(raw, highlighted.as_ref(), rows);
    }
    walker.flush(highlighted.as_ref(), rows);
}

#[derive(Default)]
struct SplitWalker {
    minus: Vec<String>,
    plus: Vec<String>,
    after_idx: usize,
    plus_run_start: usize,
}

impl SplitWalker {
    fn consume(
        &mut self,
        raw: &str,
        highlighted: Option<&Vec<Vec<Span<'static>>>>,
        rows: &mut Vec<(Line<'static>, Line<'static>)>,
    ) {
        if raw.starts_with("@@") {
            self.flush(highlighted, rows);
            let header = hunk_header_line(raw);
            rows.push((header.clone(), header));
            return;
        }
        match raw.chars().next() {
            Some('+') => {
                if self.plus.is_empty() {
                    self.plus_run_start = self.after_idx;
                }
                self.plus.push(raw.to_string());
                self.after_idx += 1;
            }
            Some('-') => {
                if !self.plus.is_empty() {
                    self.flush(highlighted, rows);
                }
                self.minus.push(raw.to_string());
            }
            Some('\\') => {
                self.flush(highlighted, rows);
                let n = no_newline_line(raw);
                rows.push((n.clone(), n));
            }
            _ => {
                self.flush(highlighted, rows);
                let line = context_split_line(raw, highlighted, self.after_idx);
                rows.push((line.clone(), line));
                self.after_idx += 1;
            }
        }
    }

    fn flush(
        &mut self,
        highlighted: Option<&Vec<Vec<Span<'static>>>>,
        rows: &mut Vec<(Line<'static>, Line<'static>)>,
    ) {
        let minus = std::mem::take(&mut self.minus);
        let plus = std::mem::take(&mut self.plus);
        let plus_start = self.plus_run_start;
        self.plus_run_start = 0;

        // Balanced paired block — pair 1:1 with word-level highlighting.
        if !minus.is_empty() && minus.len() == plus.len() {
            for (m, p) in minus.iter().zip(plus.iter()) {
                let old = m.get(1..).unwrap_or("");
                let new = p.get(1..).unwrap_or("");
                let (m_spans, p_spans) = word::intra_line_highlight(old, new);
                rows.push((Line::from(m_spans), Line::from(p_spans)));
            }
            return;
        }

        // Unbalanced — minuses on left only, plusses on right only.
        for raw in &minus {
            let body = raw.get(1..).unwrap_or("");
            let line = Line::from(Span::styled(
                body.to_string(),
                Style::default().fg(Color::Red),
            ));
            rows.push((line, filler()));
        }
        for (i, raw) in plus.iter().enumerate() {
            let line = addition_split_line(raw, highlighted, plus_start + i);
            rows.push((filler(), line));
        }
    }
}

fn context_split_line(
    raw: &str,
    highlighted: Option<&Vec<Vec<Span<'static>>>>,
    after_idx: usize,
) -> Line<'static> {
    let body = if raw.is_empty() {
        ""
    } else {
        raw.get(1..).unwrap_or("")
    };
    match highlighted.and_then(|h| h.get(after_idx)) {
        Some(line_spans) => Line::from(line_spans.clone()),
        None => Line::from(Span::styled(
            body.to_string(),
            Style::default().fg(Color::Gray),
        )),
    }
}

fn addition_split_line(
    raw: &str,
    highlighted: Option<&Vec<Vec<Span<'static>>>>,
    after_idx: usize,
) -> Line<'static> {
    let body = raw.get(1..).unwrap_or("");
    match highlighted.and_then(|h| h.get(after_idx)) {
        Some(line_spans) => Line::from(line_spans.clone()),
        None => Line::from(Span::styled(
            body.to_string(),
            Style::default().fg(Color::Green),
        )),
    }
}

fn filler() -> Line<'static> {
    Line::from(Span::styled(
        "  ~  ",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use gh_core::PatchStatus;

    fn fp(path: &str, patch: Option<&str>) -> FilePatch {
        FilePatch {
            path: path.into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 1,
            deletions: 1,
            patch: patch.map(str::to_string),
            blob_sha: "x".into(),
        }
    }

    #[test]
    fn empty_input_yields_no_rows() {
        let rows = render(&[], &[]);
        assert!(rows.is_empty());
    }

    #[test]
    fn balanced_block_pairs_minus_left_plus_right() {
        let patch = "@@ -1,3 +1,3 @@\n ctx\n-old\n+new\n trailing";
        let files = vec![fp("src/foo.rs", Some(patch))];
        let rows = render(&files, &[]);
        // Layout: header, stats, @@, ctx (both sides), pair (- on left, + on right), trailing.
        // Find the row where left and right differ.
        let pair = rows.iter().find(|(l, r)| {
            let lj: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            let rj: String = r.spans.iter().map(|s| s.content.as_ref()).collect();
            lj == "old" && rj == "new"
        });
        assert!(pair.is_some(), "expected paired (old, new) row");
    }

    #[test]
    fn unbalanced_block_uses_filler_on_missing_side() {
        // 2 dels + 1 add → unbalanced; left gets a filler when only `+`
        // is present and right gets a filler when only `-` is.
        let patch = "@@ -1,3 +1,2 @@\n-a\n-b\n+c";
        let files = vec![fp("src/foo.rs", Some(patch))];
        let rows = render(&files, &[]);
        // Find the `+c` row: right side contains "c", left side is the filler.
        let plus_row = rows
            .iter()
            .find(|(_, r)| r.spans.iter().any(|s| s.content == "c"));
        let plus_row = plus_row.expect("expected a row whose right side carries `c`");
        let left_text: String = plus_row
            .0
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            left_text.contains('~'),
            "left side of pure-add row should be filler, got {left_text:?}"
        );
        // Find a `-a` row: left side carries "a", right side is filler.
        let minus_row = rows
            .iter()
            .find(|(l, _)| l.spans.iter().any(|s| s.content == "a"));
        let minus_row = minus_row.expect("expected a row whose left side carries `a`");
        let right_text: String = minus_row
            .1
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            right_text.contains('~'),
            "right side of pure-del row should be filler, got {right_text:?}"
        );
    }

    #[test]
    fn omitted_patch_renders_placeholder_on_both_sides() {
        let files = vec![fp("vendor/big.bin", None)];
        let rows = render(&files, &[]);
        // Layout: header (×2 columns), stats (×2), placeholder (×2).
        let placeholder_row = rows.last().unwrap();
        let left: String = placeholder_row
            .0
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let right: String = placeholder_row
            .1
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(left, right);
        assert!(left.contains("diff omitted"));
    }

    #[test]
    fn context_lines_appear_identically_on_both_sides() {
        let patch = "@@ -1 +1 @@\n unchanged context";
        let files = vec![fp("src/foo.rs", Some(patch))];
        let rows = render(&files, &[]);
        let ctx_row = rows
            .iter()
            .find(|(l, _)| {
                let s: String = l.spans.iter().map(|x| x.content.as_ref()).collect();
                s.contains("unchanged context")
            })
            .expect("expected a context row");
        let left: String = ctx_row.0.spans.iter().map(|s| s.content.as_ref()).collect();
        let right: String = ctx_row.1.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(left, right);
    }
}
