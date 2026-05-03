//! Inline review-comment injection.
//!
//! After the unified or split renderer has produced its row layout, this
//! module walks the patches a second time to map `(path, new_file_line)`
//! anchors to row indices, then inserts pseudo-lines for matching
//! [`ReviewThread`]s. Outdated threads (`line == None`) are silently
//! skipped — anchoring an outdated discussion would need different logic
//! and isn't required by the read-only v1 surface.
//!
//! Pseudo-line layout per comment:
//!
//! ```text
//! ▏ @author: first body line
//! ▏          subsequent body lines
//! ```

use gh_core::{FilePatch, ReviewComment, ReviewThread};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Inject thread pseudo-lines into a unified rendering. The walk over
/// `files` mirrors [`super::render`]'s structure exactly so anchor
/// indices line up with the rendered output.
pub(super) fn inject_inline(
    lines: &mut Vec<Line<'static>>,
    files: &[FilePatch],
    threads: &[ReviewThread],
) {
    let anchors = collect_anchors(files);
    apply_inserts(lines, anchors, threads, build_pseudo_line);
}

/// Same idea but emits `(filler, pseudo_line)` row pairs for split mode.
pub(super) fn inject_inline_split(
    rows: &mut Vec<(Line<'static>, Line<'static>)>,
    files: &[FilePatch],
    threads: &[ReviewThread],
) {
    let anchors = collect_anchors(files);
    apply_inserts(rows, anchors, threads, build_pseudo_row_pair);
}

/// Number of pseudo-lines a single thread will emit. Used by the cheap
/// `file_line_offsets` path so offsets match post-injection layout.
pub(super) fn pseudo_line_count(thread: &ReviewThread) -> usize {
    if thread.line.is_none() {
        return 0;
    }
    thread
        .comments
        .iter()
        .map(|c| c.body.lines().count().max(1))
        .sum()
}

// ── shared anchor walker ───────────────────────────────────────────────────

/// `(path, new_file_line, row_index_after_emit)` for every context/`+` row
/// in the rendered output.
///
/// Mirrors `ChangeBlockWalker`'s buffer-and-flush behaviour exactly so
/// anchor indices line up with the rendered output, including the
/// **interleaved** emission used for balanced multi-pair `-`/`+` blocks
/// (the walker emits `-a, +A, -b, +B`, not `-a, -b, +A, +B`).
fn collect_anchors(files: &[FilePatch]) -> Vec<(String, u32, usize)> {
    let mut anchors = Vec::new();
    let mut idx: usize = 0;
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            idx += 1; // inter-file blank
        }
        idx += 2; // header + stats
        let patch = match &file.patch {
            Some(p) if !p.is_empty() => p,
            _ => {
                idx += 1; // placeholder
                continue;
            }
        };
        let mut new_line: u32 = 0;
        // Pending counters mirror ChangeBlockWalker's buffered state.
        let mut pending_minus: usize = 0;
        // The new-file line number assigned to each buffered `+`.
        let mut pending_plus: Vec<u32> = Vec::new();

        for raw in patch.lines() {
            if raw.starts_with("@@") {
                flush_anchors(
                    &file.path,
                    &mut idx,
                    &mut pending_minus,
                    &mut pending_plus,
                    &mut anchors,
                );
                if let Some(start) = parse_hunk_new_start(raw) {
                    new_line = start;
                }
                idx += 1;
                continue;
            }
            match raw.chars().next() {
                Some('+') => {
                    pending_plus.push(new_line);
                    new_line = new_line.saturating_add(1);
                }
                Some('-') => {
                    // `-` after `+` ends the change block — flush first.
                    if !pending_plus.is_empty() {
                        flush_anchors(
                            &file.path,
                            &mut idx,
                            &mut pending_minus,
                            &mut pending_plus,
                            &mut anchors,
                        );
                    }
                    pending_minus += 1;
                }
                Some('\\') => {
                    flush_anchors(
                        &file.path,
                        &mut idx,
                        &mut pending_minus,
                        &mut pending_plus,
                        &mut anchors,
                    );
                    idx += 1;
                }
                _ => {
                    flush_anchors(
                        &file.path,
                        &mut idx,
                        &mut pending_minus,
                        &mut pending_plus,
                        &mut anchors,
                    );
                    anchors.push((file.path.clone(), new_line, idx));
                    new_line = new_line.saturating_add(1);
                    idx += 1;
                }
            }
        }
        flush_anchors(
            &file.path,
            &mut idx,
            &mut pending_minus,
            &mut pending_plus,
            &mut anchors,
        );
    }
    anchors
}

/// Mirror of [`super::ChangeBlockWalker::flush`]. Walks the buffered
/// `-`/`+` runs in the same order the renderer emits them (interleaved
/// for balanced blocks; stacked for unbalanced) and pushes anchors at
/// the right post-emit indices.
fn flush_anchors(
    path: &str,
    idx: &mut usize,
    pending_minus: &mut usize,
    pending_plus: &mut Vec<u32>,
    anchors: &mut Vec<(String, u32, usize)>,
) {
    let m = *pending_minus;
    let p = pending_plus.len();
    if m > 0 && m == p {
        // Balanced — interleaved (`-, +, -, +, ...`).
        for &line in pending_plus.iter() {
            *idx += 1; // `-` row
            anchors.push((path.to_string(), line, *idx));
            *idx += 1; // `+` row at this index
        }
    } else {
        // Unbalanced or one-sided — minuses first, then plusses.
        *idx += m;
        for &line in pending_plus.iter() {
            anchors.push((path.to_string(), line, *idx));
            *idx += 1;
        }
    }
    *pending_minus = 0;
    pending_plus.clear();
}

fn parse_hunk_new_start(line: &str) -> Option<u32> {
    let after_at = line.strip_prefix("@@")?.trim_start();
    let plus_idx = after_at.find('+')?;
    let after_plus = &after_at[plus_idx + 1..];
    let end = after_plus
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_plus.len());
    after_plus[..end].parse().ok()
}

// ── insert orchestration ──────────────────────────────────────────────────

fn apply_inserts<T, F>(
    out: &mut Vec<T>,
    anchors: Vec<(String, u32, usize)>,
    threads: &[ReviewThread],
    build_one: F,
) where
    F: Fn(&ReviewComment, bool) -> T,
{
    // Find each thread's anchor (first matching path+line). Build the
    // pseudo-rows it will produce, with their target insertion index.
    let mut inserts: Vec<(usize, Vec<T>)> = Vec::new();
    for thread in threads {
        let Some(line_no) = thread.line else { continue };
        let Some((_, _, anchor_idx)) = anchors
            .iter()
            .find(|(p, l, _)| p == &thread.path && *l == line_no)
        else {
            continue;
        };
        let rows: Vec<T> = thread
            .comments
            .iter()
            .flat_map(|c| build_comment_rows(c, &build_one))
            .collect();
        if !rows.is_empty() {
            inserts.push((anchor_idx + 1, rows));
        }
    }
    // Stable sort by anchor index so multiple threads on the same line stack
    // in walk order. Apply forward with a running shift.
    inserts.sort_by_key(|(idx, _)| *idx);
    let mut shift = 0usize;
    for (orig_at, rows) in inserts {
        let count = rows.len();
        for (off, row) in rows.into_iter().enumerate() {
            // Defensive clamp: in the (debug-asserted) absence of bugs the
            // walker-mirroring `collect_anchors` always yields valid indices,
            // but a future change to the renderer must not crash the binary.
            let target = orig_at.saturating_add(shift).saturating_add(off);
            let safe = target.min(out.len());
            debug_assert_eq!(
                target,
                safe,
                "thread anchor {target} > rendered output length {} — bug in collect_anchors",
                out.len(),
            );
            out.insert(safe, row);
        }
        shift += count;
    }
}

fn build_comment_rows<T, F>(comment: &ReviewComment, build_one: &F) -> Vec<T>
where
    F: Fn(&ReviewComment, bool) -> T,
{
    let body_lines: Vec<&str> = if comment.body.is_empty() {
        vec![""]
    } else {
        comment.body.lines().collect()
    };
    body_lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let synthetic = ReviewComment {
                author: comment.author.clone(),
                body: line.to_string(),
                created_at: comment.created_at,
            };
            build_one(&synthetic, i == 0)
        })
        .collect()
}

// ── per-row builders for unified vs split ─────────────────────────────────

fn build_pseudo_line(comment: &ReviewComment, is_first: bool) -> Line<'static> {
    let prefix = if is_first {
        format!("▏ @{}: ", comment.author)
    } else {
        "▏        ".to_string()
    };
    Line::from(vec![
        Span::styled(
            prefix,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
        Span::styled(
            comment.body.clone(),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
        ),
    ])
}

fn build_pseudo_row_pair(
    comment: &ReviewComment,
    is_first: bool,
) -> (Line<'static>, Line<'static>) {
    let pseudo = build_pseudo_line(comment, is_first);
    let filler = Line::from(Span::styled(
        "  ~  ",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    ));
    (filler, pseudo)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Utc;
    use gh_core::PatchStatus;

    fn fp(path: &str, patch: &str) -> FilePatch {
        FilePatch {
            path: path.into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 1,
            deletions: 1,
            patch: Some(patch.into()),
            blob_sha: "x".into(),
        }
    }

    fn thread(path: &str, line: u32, author: &str, body: &str) -> ReviewThread {
        ReviewThread {
            path: path.into(),
            line: Some(line),
            original_line: Some(line),
            comments: vec![ReviewComment {
                author: author.into(),
                body: body.into(),
                created_at: Utc::now(),
            }],
        }
    }

    #[test]
    fn parse_hunk_new_start_extracts_c() {
        assert_eq!(parse_hunk_new_start("@@ -1,3 +5,7 @@"), Some(5));
        assert_eq!(parse_hunk_new_start("@@ -1,3 +1 @@ fn foo()"), Some(1));
        assert_eq!(parse_hunk_new_start("@@ -0,0 +1,3 @@"), Some(1));
        assert_eq!(parse_hunk_new_start("@@ broken"), None);
    }

    #[test]
    fn pseudo_line_count_zero_for_outdated() {
        let mut t = thread("a.rs", 1, "alice", "x");
        t.line = None;
        assert_eq!(pseudo_line_count(&t), 0);
    }

    #[test]
    fn pseudo_line_count_one_per_body_line() {
        let t = thread("a.rs", 1, "alice", "first\nsecond\nthird");
        assert_eq!(pseudo_line_count(&t), 3);
    }

    #[test]
    fn pseudo_line_count_treats_empty_body_as_one_line() {
        let t = thread("a.rs", 1, "alice", "");
        assert_eq!(pseudo_line_count(&t), 1);
    }

    #[test]
    fn collect_anchors_records_new_file_line_for_context_and_plus() {
        let files = vec![fp("a.rs", "@@ -1,2 +1,2 @@\n one\n+two")];
        let anchors = collect_anchors(&files);
        // Layout indices:
        //   0: header
        //   1: stats
        //   2: @@
        //   3: " one"   ← new_line 1
        //   4: "+two"   ← new_line 2
        assert_eq!(anchors.len(), 2);
        assert_eq!(anchors[0], ("a.rs".to_string(), 1, 3));
        assert_eq!(anchors[1], ("a.rs".to_string(), 2, 4));
    }

    #[test]
    fn collect_anchors_balanced_multi_pair_uses_interleaved_indices() {
        // 2-2 balanced block. `super::ChangeBlockWalker` emits
        //   -a, +A, -b, +B  (interleaved)
        // not patch order
        //   -a, -b, +A, +B  (stacked)
        // So the anchor for `+A` (new line 1) must point at idx 4 and
        // the anchor for `+B` (new line 2) at idx 6.
        let files = vec![fp("a.rs", "@@ -1,2 +1,2 @@\n-a\n-b\n+A\n+B")];
        let anchors = collect_anchors(&files);
        // Indices:
        //   0: header, 1: stats, 2: @@,
        //   3: -a, 4: +A, 5: -b, 6: +B
        assert_eq!(anchors.len(), 2);
        assert_eq!(anchors[0], ("a.rs".to_string(), 1, 4));
        assert_eq!(anchors[1], ("a.rs".to_string(), 2, 6));
    }

    #[test]
    fn collect_anchors_unbalanced_block_keeps_stacked_indices() {
        // 2 - + 1 + is unbalanced → walker emits stacked:
        //   -a, -b, +C
        let files = vec![fp("a.rs", "@@ -1,2 +1,1 @@\n-a\n-b\n+C")];
        let anchors = collect_anchors(&files);
        // 0: header, 1: stats, 2: @@, 3: -a, 4: -b, 5: +C
        assert_eq!(anchors, vec![("a.rs".to_string(), 1, 5)]);
    }

    #[test]
    fn collect_anchors_match_render_output_for_balanced_multi_pair() {
        // End-to-end check: the index recorded for `+A` must point at
        // the rendered `+A` line in the actual `render` output.
        let patch = "@@ -1,2 +1,2 @@\n-a\n-b\n+A\n+B";
        let files = vec![fp("a.rs", patch)];
        let lines = super::super::render(&files, &[]);
        let anchors = collect_anchors(&files);
        for (path, line_no, idx) in anchors {
            // Round-trip: the rendered line at idx should contain content
            // that matches the post-image line. For our test patch:
            //   line 1 → "A", line 2 → "B"
            let rendered: String = lines[idx]
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect();
            let expected = match line_no {
                1 => "A",
                2 => "B",
                _ => panic!("unexpected anchor line_no {line_no}"),
            };
            assert!(
                rendered.contains(expected),
                "anchor for {path}:{line_no} (idx {idx}) should point at {expected:?}; got {rendered:?}",
            );
        }
    }

    #[test]
    fn render_with_thread_on_balanced_multi_pair_does_not_panic() {
        // This was the user-reported smoke-test panic: a thread anchored
        // to a non-final `+` in a balanced multi-pair block crashed
        // `out.insert` with an out-of-bounds index because the old
        // `collect_anchors` walked patch order, not interleaved order.
        use chrono::Utc;
        let patch = "@@ -1,2 +1,2 @@\n-a\n-b\n+A\n+B";
        let files = vec![fp("src/foo.rs", patch)];
        let threads = vec![ReviewThread {
            path: "src/foo.rs".into(),
            line: Some(1), // anchored to +A, the first plus of the pair
            original_line: Some(1),
            comments: vec![ReviewComment {
                author: "alice".into(),
                body: "comment on +A".into(),
                created_at: Utc::now(),
            }],
        }];
        // Should NOT panic. Should inject 1 pseudo-line after the +A row.
        let lines = super::super::render(&files, &threads);
        // 4 base patch rows + header + stats + @@ + 1 pseudo = 8
        assert_eq!(lines.len(), 8);
    }
}
