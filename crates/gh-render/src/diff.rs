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

/// Render a sequence of file patches into displayable lines.
#[must_use]
pub fn render(files: &[FilePatch]) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            lines.push(Line::raw(""));
        }
        lines.push(file_header(file));
        lines.push(file_stats(file));
        match &file.patch {
            Some(patch) if !patch.is_empty() => {
                for raw in patch.lines() {
                    lines.push(patch_line(raw));
                }
            }
            _ => lines.push(diff_omitted()),
        }
    }
    lines
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

fn patch_line(raw: &str) -> Line<'static> {
    if raw.starts_with("@@") {
        return Line::from(Span::styled(
            raw.to_string(),
            Style::default().fg(Color::Blue),
        ));
    }
    let style = match raw.chars().next() {
        Some('+') => Style::default().fg(Color::Green),
        Some('-') => Style::default().fg(Color::Red),
        Some('\\') => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
        _ => Style::default().fg(Color::Gray),
    };
    Line::from(Span::styled(raw.to_string(), style))
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
