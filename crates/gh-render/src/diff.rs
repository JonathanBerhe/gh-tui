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

use gh_core::{FilePatch, PatchStatus};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::syntax::{self, Lang};

/// Render a sequence of file patches into displayable lines.
///
/// Per file, if [`syntax::detect`] returns a non-`Plain` language, the
/// renderer reconstructs the post-image (context + `+` lines) and runs
/// tree-sitter highlighting once. Context and `+` lines then use the
/// per-token styled spans from that lookup; `-` lines stay solid red until
/// PR #3 introduces word-level pairing. Plain-language files take a fast
/// path that skips the highlighter entirely.
#[must_use]
pub fn render(files: &[FilePatch]) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            lines.push(Line::raw(""));
        }
        render_file(file, &mut lines);
    }
    lines
}

fn render_file(file: &FilePatch, lines: &mut Vec<Line<'static>>) {
    lines.push(file_header(file));
    lines.push(file_stats(file));

    let patch = match &file.patch {
        Some(p) if !p.is_empty() => p,
        _ => {
            lines.push(diff_omitted());
            return;
        }
    };

    let lang = syntax::detect(&file.path);
    let highlighted = if matches!(lang, Lang::Plain) {
        None
    } else {
        let after = reconstruct_after(patch);
        Some(syntax::highlight(lang, &after))
    };

    let mut after_idx: usize = 0;
    for raw in patch.lines() {
        if raw.starts_with("@@") {
            lines.push(hunk_header_line(raw));
            continue;
        }
        match raw.chars().next() {
            Some('+') => {
                lines.push(addition_line(raw, highlighted.as_ref(), after_idx));
                after_idx += 1;
            }
            Some('-') => {
                lines.push(deletion_line(raw));
            }
            Some('\\') => {
                lines.push(no_newline_line(raw));
            }
            // Context line (leading space) or truly blank line — both go in
            // the after-image, so increment the index in lockstep.
            _ => {
                lines.push(context_line(raw, highlighted.as_ref(), after_idx));
                after_idx += 1;
            }
        }
    }
}

/// Pull just the post-image lines (context + additions) out of a patch,
/// stripping the leading prefix character. The result is what tree-sitter
/// sees.
fn reconstruct_after(patch: &str) -> String {
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
///
/// Cheap: walks `patch.lines()` once per file to count, never allocates a
/// `Line` or `Span`. Workers call this on the message-passing path so the
/// hot render path runs only inside the UI loop.
///
/// Saturates at `u16::MAX`; jumps in diffs longer than 65535 lines will land
/// at the final saturated offset rather than the file's true position.
#[must_use]
pub fn file_line_offsets(files: &[FilePatch]) -> Vec<u16> {
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
    }
    offsets
}

fn file_header(file: &FilePatch) -> Line<'static> {
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

fn file_stats(file: &FilePatch) -> Line<'static> {
    Line::from(Span::styled(
        format!("+{} -{}", file.additions, file.deletions),
        Style::default().fg(Color::DarkGray),
    ))
}

fn diff_omitted() -> Line<'static> {
    Line::from(Span::styled(
        "[diff omitted: file too large or binary]",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    ))
}

fn hunk_header_line(raw: &str) -> Line<'static> {
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

fn no_newline_line(raw: &str) -> Line<'static> {
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
        let lines = render(&[]);
        assert!(lines.is_empty());
        assert!(file_line_offsets(&[]).is_empty());
    }

    #[test]
    fn missing_patch_renders_placeholder() {
        let files = vec![fp("a.rs", None, PatchStatus::Modified)];
        let lines = render(&files);
        // header + stats + placeholder
        assert_eq!(lines.len(), 3);
        let placeholder = &lines[2].spans[0].content;
        assert!(placeholder.contains("diff omitted"));
    }

    #[test]
    fn simple_patch_renders_header_stats_and_hunk_lines() {
        let patch = "@@ -1,2 +1,2 @@\n one\n-two\n+TWO";
        let files = vec![fp("a.rs", Some(patch), PatchStatus::Modified)];
        let lines = render(&files);
        // header, stats, @@, " one", "-two", "+TWO" → 6
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn renamed_file_header_shows_arrow() {
        let mut file = fp("new.rs", None, PatchStatus::Renamed);
        file.previous_path = Some("old.rs".into());
        let lines = render(&[file]);
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
        let offsets = file_line_offsets(&files);
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0], 0, "first file starts at line 0");
        // first file: header(1) + stats(1) + @@(1) + +x(1) = 4 lines, then
        // the inter-file blank brings us to 5, second file's header at 5.
        assert_eq!(offsets[1], 5);
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
        let lines = render(&files);
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
        let lines = render(&files);
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
    fn deletion_lines_stay_solid_red_even_with_syntax() {
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
        let lines = render(&files);
        // Layout: header, stats, @@, "-...", "+..."
        let minus_line = &lines[3];
        assert_eq!(minus_line.spans.len(), 1, "deletion stays as one span");
        assert_eq!(minus_line.spans[0].style.fg, Some(Color::Red));
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
        let lines = render(&files);
        let offsets = file_line_offsets(&files);
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
