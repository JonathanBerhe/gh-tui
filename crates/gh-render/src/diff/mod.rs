//! Renderer for unified-diff patches as returned by GitHub's
//! `GET /pulls/{n}/files` REST endpoint.
//!
//! Each [`FilePatch`] is laid out as:
//!
//! ```text
//! path/to/file.rs (modified)        ← bold cyan header
//! +12 -3                            ← dim stats
//! @@ -1,3 +1,4 @@                   ← blue hunk header
//!  context line                     ← dim
//! -removed line                     ← red
//! +added line                       ← green
//! ```
//!
//! Files with `patch == None` (too large / binary) render as a single dim
//! italic placeholder. A blank line separates consecutive files.

pub mod split;
pub mod threads;
pub mod word;

use gh_core::{FilePatch, PatchStatus, ReviewThread};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::syntax::{self, Lang};

/// Number of spaces a `\t` expands to in the diff viewer. Hardcoded for
/// now; lifts to the theme/keymap config when that lands. Most Go and
/// Rust toolchains pick 4; matches `git`'s default.
const TAB_WIDTH: usize = 4;

/// Replace every `\t` with [`TAB_WIDTH`] spaces. Terminals render tabs at
/// inconsistent widths (8 by default, sometimes terminal-configured),
/// which breaks visual alignment in a code-heavy view. Normalising once
/// at the renderer entry means every downstream path — tree-sitter
/// highlighter, `similar` word diff, ratatui spans — sees the same flat
/// whitespace.
pub(super) fn expand_tabs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '\t' {
            for _ in 0..TAB_WIDTH {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Render a sequence of file patches into displayable lines.
///
/// Per file, if [`syntax::detect`] returns a non-`Plain` language, the
/// renderer reconstructs the post-image (context + `+` lines) and runs
/// tree-sitter highlighting once. Context and `+` lines then use the
/// per-token styled spans from that lookup. After the body is laid out,
/// matching review threads inject pseudo-lines beneath the anchor row
/// (path + new-file line number).
#[must_use]
pub fn render(files: &[FilePatch], threads: &[ReviewThread]) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            lines.push(Line::raw(""));
        }
        render_file(file, &mut lines);
    }
    threads::inject_inline(&mut lines, files, threads);
    lines
}

fn render_file(file: &FilePatch, lines: &mut Vec<Line<'static>>) {
    lines.push(file_header(file));
    lines.push(file_stats(file));

    let raw_patch = match &file.patch {
        Some(p) if !p.is_empty() => p,
        _ => {
            lines.push(diff_omitted());
            return;
        }
    };
    // Normalise tabs once so tree-sitter, similar, and ratatui all agree.
    let patch = expand_tabs(raw_patch);

    let lang = syntax::detect(&file.path);
    let highlighted = if matches!(lang, Lang::Plain) {
        None
    } else {
        let after = reconstruct_after(&patch);
        Some(syntax::highlight(lang, &after))
    };

    let mut walker = ChangeBlockWalker::default();
    for raw in patch.lines() {
        walker.consume(raw, highlighted.as_ref(), lines);
    }
    walker.flush(highlighted.as_ref(), lines);
}

/// Stateful walker that buffers consecutive `-`/`+` runs and pairs them
/// 1:1 for word-level highlighting when the runs are balanced. Other line
/// kinds (context, hunk header, no-newline marker) flush the pending run
/// and emit normally.
#[derive(Default)]
struct ChangeBlockWalker {
    /// Pending `-` lines, raw (with the `-` prefix).
    minus: Vec<String>,
    /// Pending `+` lines, raw (with the `+` prefix).
    plus: Vec<String>,
    /// Current 0-based index into the post-image (for syntax span lookup).
    after_idx: usize,
    /// `after_idx` at the start of the current `+` run (so we can index
    /// per paired line on flush).
    plus_run_start: usize,
}

impl ChangeBlockWalker {
    fn consume(
        &mut self,
        raw: &str,
        highlighted: Option<&Vec<Vec<Span<'static>>>>,
        lines: &mut Vec<Line<'static>>,
    ) {
        if raw.starts_with("@@") {
            self.flush(highlighted, lines);
            lines.push(hunk_header_line(raw));
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
                // `-` after `+` ends a block; flush before starting a new run.
                if !self.plus.is_empty() {
                    self.flush(highlighted, lines);
                }
                self.minus.push(raw.to_string());
            }
            Some('\\') => {
                self.flush(highlighted, lines);
                lines.push(no_newline_line(raw));
            }
            // Context line (leading space) or truly blank line — flush
            // pending run, then emit normally.
            _ => {
                self.flush(highlighted, lines);
                lines.push(context_line(raw, highlighted, self.after_idx));
                self.after_idx += 1;
            }
        }
    }

    fn flush(
        &mut self,
        highlighted: Option<&Vec<Vec<Span<'static>>>>,
        lines: &mut Vec<Line<'static>>,
    ) {
        let minus = std::mem::take(&mut self.minus);
        let plus = std::mem::take(&mut self.plus);
        let plus_start = self.plus_run_start;
        self.plus_run_start = 0;

        // Balanced 1:1 paired block — word-level intra-line highlighting.
        if !minus.is_empty() && minus.len() == plus.len() {
            for (m, p) in minus.iter().zip(plus.iter()) {
                let old = m.get(1..).unwrap_or("");
                let new = p.get(1..).unwrap_or("");
                let (m_spans, p_spans) = word::intra_line_highlight(old, new);
                lines.push(paired_minus_line(m_spans));
                lines.push(paired_plus_line(p_spans));
            }
            return;
        }

        // Unbalanced or one-sided — fall back to whole-line styling.
        for raw in &minus {
            lines.push(deletion_line(raw));
        }
        for (i, raw) in plus.iter().enumerate() {
            lines.push(addition_line(raw, highlighted, plus_start + i));
        }
    }
}

fn paired_minus_line(spans: Vec<Span<'static>>) -> Line<'static> {
    let mut out = vec![Span::styled("-", Style::default().fg(Color::Red))];
    out.extend(spans);
    Line::from(out)
}

fn paired_plus_line(spans: Vec<Span<'static>>) -> Line<'static> {
    let mut out = vec![Span::styled("+", Style::default().fg(Color::Green))];
    out.extend(spans);
    Line::from(out)
}

/// Pull just the post-image lines (context + additions) out of a patch,
/// stripping the leading prefix character. The result is what tree-sitter
/// sees.
pub(super) fn reconstruct_after(patch: &str) -> String {
    let mut out = String::new();
    let mut first = true;
    for raw in patch.lines() {
        if raw.starts_with("@@") {
            continue;
        }
        let body = match raw.chars().next() {
            Some('+') | Some(' ') => raw.get(1..).unwrap_or(""),
            // Truly blank line in the patch — still a context line in the
            // post-image; treat as empty.
            None => "",
            // Skip deletions and no-newline markers.
            _ => continue,
        };
        if !first {
            out.push('\n');
        }
        out.push_str(body);
        first = false;
    }
    out
}

/// Cumulative line offsets pointing at each file's header in the rendered
/// output. Used by the reducer to serve `{`/`}` jumps between files.
#[must_use]
pub fn file_line_offsets(files: &[FilePatch], threads: &[ReviewThread]) -> Vec<u16> {
    file_line_layout(files, threads).0
}

/// Total rendered line count for the diff view. Used by the reducer to
/// clamp `scroll` so `G` (jump-to-end) doesn't park the scroll value far
/// past actual content length, leaving subsequent `k` presses no-ops.
#[must_use]
pub fn total_diff_lines(files: &[FilePatch], threads: &[ReviewThread]) -> u16 {
    file_line_layout(files, threads).1
}

/// Cheap single-pass walker that produces both the per-file offset table
/// and the total rendered line count. Walks `patch.lines()` once per
/// file, never allocates a `Line` or `Span`. Workers call this on the
/// message-passing path so the hot render path runs only inside the UI
/// loop. Thread pseudo-lines are counted into both outputs so the
/// reducer's section jumps and end-clamp remain accurate.
///
/// Saturates at `u16::MAX`; diffs longer than 65535 lines land at the
/// saturated offset rather than the true position.
#[must_use]
pub fn file_line_layout(files: &[FilePatch], threads: &[ReviewThread]) -> (Vec<u16>, u16) {
    let mut offsets: Vec<u16> = Vec::with_capacity(files.len());
    let mut total: u32 = 0;
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            // Inter-file blank separator.
            total = total.saturating_add(1);
        }
        offsets.push(u16::try_from(total).unwrap_or(u16::MAX));
        // Header + stats lines.
        total = total.saturating_add(2);
        // Patch body — `lines()` count, or 1 for the placeholder.
        let body_lines = match &file.patch {
            Some(patch) if !patch.is_empty() => {
                u32::try_from(patch.lines().count()).unwrap_or(u32::MAX)
            }
            _ => 1,
        };
        total = total.saturating_add(body_lines);
        // Thread pseudo-lines anchored to lines in this file.
        let thread_lines: u32 = threads
            .iter()
            .filter(|t| t.path == file.path && t.line.is_some())
            .map(|t| u32::try_from(threads::pseudo_line_count(t)).unwrap_or(u32::MAX))
            .sum();
        total = total.saturating_add(thread_lines);
    }
    (offsets, u16::try_from(total).unwrap_or(u16::MAX))
}

pub(super) fn file_header(file: &FilePatch) -> Line<'static> {
    let path_text = match (&file.previous_path, file.status) {
        (Some(prev), PatchStatus::Renamed | PatchStatus::Copied) => {
            format!("{prev} → {}", file.path)
        }
        _ => file.path.clone(),
    };
    Line::from(Span::styled(
        format!("{path_text} ({})", file.status.label()),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
}

pub(super) fn file_stats(file: &FilePatch) -> Line<'static> {
    Line::from(Span::styled(
        format!("+{} -{}", file.additions, file.deletions),
        Style::default().fg(Color::DarkGray),
    ))
}

pub(super) fn diff_omitted() -> Line<'static> {
    Line::from(Span::styled(
        "[diff omitted: file too large or binary]",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    ))
}

pub(super) fn hunk_header_line(raw: &str) -> Line<'static> {
    Line::from(Span::styled(
        raw.to_string(),
        Style::default().fg(Color::Blue),
    ))
}

fn deletion_line(raw: &str) -> Line<'static> {
    Line::from(Span::styled(
        raw.to_string(),
        Style::default().fg(Color::Red),
    ))
}

pub(super) fn no_newline_line(raw: &str) -> Line<'static> {
    Line::from(Span::styled(
        raw.to_string(),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    ))
}

/// Render a `+` line with a green prefix span and either syntax-styled
/// content (when highlighting is available) or a single solid-green span.
fn addition_line(
    raw: &str,
    highlighted: Option<&Vec<Vec<Span<'static>>>>,
    after_idx: usize,
) -> Line<'static> {
    let body = raw.get(1..).unwrap_or("");
    let prefix = Span::styled("+", Style::default().fg(Color::Green));
    let mut spans = vec![prefix];
    match highlighted.and_then(|h| h.get(after_idx)) {
        Some(line_spans) => spans.extend(line_spans.iter().cloned()),
        None => spans.push(Span::styled(
            body.to_string(),
            Style::default().fg(Color::Green),
        )),
    }
    Line::from(spans)
}

/// Render a context line with an unstyled prefix space and either
/// syntax-styled content or a single dim span.
fn context_line(
    raw: &str,
    highlighted: Option<&Vec<Vec<Span<'static>>>>,
    after_idx: usize,
) -> Line<'static> {
    let body = if raw.is_empty() {
        ""
    } else {
        raw.get(1..).unwrap_or("")
    };
    let prefix = Span::raw(" ");
    let mut spans = vec![prefix];
    match highlighted.and_then(|h| h.get(after_idx)) {
        Some(line_spans) => spans.extend(line_spans.iter().cloned()),
        None => spans.push(Span::styled(
            body.to_string(),
            Style::default().fg(Color::Gray),
        )),
    }
    Line::from(spans)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn fp(path: &str, patch: Option<&str>, status: PatchStatus) -> FilePatch {
        FilePatch {
            path: path.into(),
            previous_path: None,
            status,
            additions: 1,
            deletions: 1,
            patch: patch.map(str::to_string),
            blob_sha: "deadbeef".into(),
        }
    }

    #[test]
    fn empty_input_renders_no_lines() {
        let lines = render(&[], &[]);
        assert!(lines.is_empty());
        assert!(file_line_offsets(&[], &[]).is_empty());
    }

    #[test]
    fn missing_patch_renders_placeholder() {
        let files = vec![fp("a.rs", None, PatchStatus::Modified)];
        let lines = render(&files, &[]);
        // header + stats + placeholder
        assert_eq!(lines.len(), 3);
        let placeholder = &lines[2].spans[0].content;
        assert!(placeholder.contains("diff omitted"));
    }

    #[test]
    fn simple_patch_renders_header_stats_and_hunk_lines() {
        let patch = "@@ -1,2 +1,2 @@\n one\n-two\n+TWO";
        let files = vec![fp("a.rs", Some(patch), PatchStatus::Modified)];
        let lines = render(&files, &[]);
        // header, stats, @@, " one", "-two", "+TWO" → 6
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn renamed_file_header_shows_arrow() {
        let mut file = fp("new.rs", None, PatchStatus::Renamed);
        file.previous_path = Some("old.rs".into());
        let lines = render(&[file], &[]);
        let header = &lines[0].spans[0].content;
        assert!(header.contains("old.rs → new.rs"));
        assert!(header.contains("renamed"));
    }

    #[test]
    fn file_offsets_point_at_each_files_header() {
        let files = vec![
            fp("a.rs", Some("@@ -1 +1 @@\n+x"), PatchStatus::Modified),
            fp("b.rs", Some("@@ -1 +1 @@\n+y"), PatchStatus::Modified),
        ];
        let offsets = file_line_offsets(&files, &[]);
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0], 0, "first file starts at line 0");
        // first file: header(1) + stats(1) + @@(1) + +x(1) = 4 lines, then
        // the inter-file blank brings us to 5, second file's header at 5.
        assert_eq!(offsets[1], 5);
    }

    #[test]
    fn tabs_in_patch_expand_to_spaces() {
        // Go-style tab-indented context line and addition. Both should
        // render with leading spaces, not raw `\t` (terminals render tabs
        // at inconsistent widths and break alignment).
        let patch = "@@ -1,2 +1,2 @@\n\tcontext\n+\tnew";
        let files = vec![FilePatch {
            path: "main.go".into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 1,
            deletions: 0,
            patch: Some(patch.into()),
            blob_sha: "x".into(),
        }];
        let lines = render(&files, &[]);
        // No rendered Span content should contain a literal tab.
        for line in &lines {
            for span in &line.spans {
                assert!(
                    !span.content.contains('\t'),
                    "tab leaked into rendered span: {:?}",
                    span.content,
                );
            }
        }
        // The context line at idx 3 (after header/stats/@@) must contain
        // 4 leading spaces from the expanded tab, then "context".
        let ctx_text: String = lines[3].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            ctx_text.contains("    context"),
            "expected 4-space-indented context, got {ctx_text:?}",
        );
    }

    #[test]
    fn expand_tabs_basic() {
        assert_eq!(expand_tabs("\tfoo"), "    foo");
        assert_eq!(expand_tabs("a\tb\tc"), "a    b    c");
        assert_eq!(expand_tabs("no tabs here"), "no tabs here");
        assert_eq!(expand_tabs(""), "");
    }

    #[test]
    fn rust_file_addition_lines_are_syntax_highlighted() {
        let patch = "@@ -1,2 +1,2 @@\n fn old() {}\n+fn new() {}";
        let files = vec![FilePatch {
            path: "src/foo.rs".into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 1,
            deletions: 0,
            patch: Some(patch.into()),
            blob_sha: "x".into(),
        }];
        let lines = render(&files, &[]);
        // Layout: header, stats, @@, " fn old() {}", "+fn new() {}"
        let plus_line = &lines[4];
        // Prefix span is "+", then syntax-coloured tokens. Look for the
        // magenta "fn" keyword span anywhere in the line.
        let has_keyword = plus_line
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Magenta));
        assert!(
            has_keyword,
            "expected magenta keyword span in syntax-highlighted addition"
        );
        // First span is the green "+" prefix.
        assert_eq!(plus_line.spans[0].content, "+");
        assert_eq!(plus_line.spans[0].style.fg, Some(Color::Green));
    }

    #[test]
    fn non_rust_file_falls_back_to_plain_styling() {
        let patch = "@@ -1 +1 @@\n+fn this_is_not_actually_rust() {}";
        let files = vec![FilePatch {
            path: "config.toml".into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 1,
            deletions: 0,
            patch: Some(patch.into()),
            blob_sha: "x".into(),
        }];
        let lines = render(&files, &[]);
        // Layout: header, stats, @@, "+fn..."
        let plus_line = &lines[3];
        // Plain-language path emits prefix + single solid-green body span.
        assert_eq!(plus_line.spans.len(), 2);
        assert_eq!(plus_line.spans[0].content, "+");
        assert_eq!(plus_line.spans[1].style.fg, Some(Color::Green));
        // No magenta (would indicate the highlighter wrongly fired).
        let has_keyword = plus_line
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Magenta));
        assert!(!has_keyword);
    }

    #[test]
    fn unpaired_deletion_stays_solid_red() {
        // 2 - lines + 1 + line is unbalanced → fall back to whole-line red.
        let patch = "@@ -1,3 +1,2 @@\n-a\n-b\n+c";
        let files = vec![FilePatch {
            path: "src/foo.rs".into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 1,
            deletions: 2,
            patch: Some(patch.into()),
            blob_sha: "x".into(),
        }];
        let lines = render(&files, &[]);
        // Layout: header, stats, @@, "-a", "-b", "+c"
        for raw_line in &lines[3..=4] {
            assert_eq!(raw_line.spans.len(), 1, "unpaired deletion is one span");
            assert_eq!(raw_line.spans[0].style.fg, Some(Color::Red));
        }
    }

    #[test]
    fn paired_block_word_highlights_changed_tokens() {
        // 1 - line and 1 + line of equal count → paired, word-level.
        let patch = "@@ -1 +1 @@\n-fn old() {}\n+fn new() {}";
        let files = vec![FilePatch {
            path: "src/foo.rs".into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 1,
            deletions: 1,
            patch: Some(patch.into()),
            blob_sha: "x".into(),
        }];
        let lines = render(&files, &[]);
        // Paired `-` line is no longer one solid span; it's prefix + per-word
        // spans where `old` carries the bold red bg overlay.
        let minus_line = &lines[3];
        assert!(
            minus_line.spans.len() > 1,
            "paired deletion is split per word"
        );
        assert_eq!(minus_line.spans[0].content, "-");
        // Any span on the minus line must carry the bold word-bg highlight
        // for the `old` change. Span content varies with similar's word
        // tokenisation (punctuation may attach), so don't pin it to "old"
        // exactly — just assert *some* highlighted span exists and that
        // unhighlighted spans cover the unchanged portions.
        let any_bold = minus_line
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
        assert!(
            any_bold,
            "expected at least one bold word-highlight on - side"
        );
        // The plus line should have its own bold highlight for `new`.
        let plus_line = &lines[4];
        let any_bold_plus = plus_line
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
        assert!(
            any_bold_plus,
            "expected at least one bold word-highlight on + side"
        );
    }

    #[test]
    fn reconstruct_after_drops_deletions_and_strips_prefixes() {
        let patch = "@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three";
        let after = reconstruct_after(patch);
        assert_eq!(after, "one\nTWO\nthree");
    }

    /// Cheap `file_line_offsets` must agree with the full `render` walk —
    /// when offsets[i] points at line N, the rendered output's line N must
    /// be the matching file's header.
    #[test]
    fn cheap_offsets_match_full_render() {
        let files = vec![
            fp(
                "a.rs",
                Some("@@ -1,2 +1,2 @@\n one\n+two"),
                PatchStatus::Modified,
            ),
            fp("b.rs", None, PatchStatus::Modified),
            fp("c.rs", Some("@@ -1 +1 @@\n+x"), PatchStatus::Added),
        ];
        let lines = render(&files, &[]);
        let offsets = file_line_offsets(&files, &[]);
        for (i, file) in files.iter().enumerate() {
            let idx = usize::from(offsets[i]);
            let header_text = &lines[idx].spans[0].content;
            assert!(
                header_text.contains(&file.path),
                "offset[{i}] = {idx} should point at {} header, got {header_text:?}",
                file.path,
            );
        }
    }
}
