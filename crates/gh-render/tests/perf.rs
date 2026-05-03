//! Coarse-grained performance test for `render_diff`. Mirrors the
//! `benches/diff.rs` synthetic input but runs as a regular `#[test]` so CI
//! catches regressions without needing the criterion harness.
//!
//! Target: 5k-line diff < 300 ms first paint on a 2020-era laptop. We
//! give the test a 1 s ceiling — generous enough to survive CI noise and
//! slower runners while still flagging > 3× regressions.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use gh_core::{FilePatch, PatchStatus};
use gh_render::render_diff;

fn synthetic_patch(target_lines: usize, hunks: usize) -> Vec<FilePatch> {
    let lines_per_hunk = target_lines / hunks;
    let mut body = String::new();
    for h in 0..hunks {
        body.push_str(&format!(
            "@@ -{0},{1} +{0},{1} @@\n",
            h * lines_per_hunk + 1,
            lines_per_hunk
        ));
        let half = lines_per_hunk / 2;
        for i in 0..half {
            body.push_str(&format!(" let unchanged_{h}_{i} = compute({i});\n"));
        }
        body.push_str(&format!("-let changed_{h} = old_value(42);\n"));
        body.push_str(&format!("+let changed_{h} = new_value(43);\n"));
        for i in 0..half {
            body.push_str(&format!(" let trailing_{h}_{i} = consume({i});\n"));
        }
    }
    vec![FilePatch {
        path: "src/synthetic.rs".into(),
        previous_path: None,
        status: PatchStatus::Modified,
        additions: u32::try_from(hunks).unwrap_or(u32::MAX),
        deletions: u32::try_from(hunks).unwrap_or(u32::MAX),
        patch: Some(body),
        blob_sha: "synth".into(),
    }]
}

#[test]
fn renders_5k_lines_under_one_second() {
    let files = synthetic_patch(5_000, 100);
    // Warm caches: tree-sitter HighlightConfiguration in the OnceLock,
    // CPU caches, allocator, etc. We measure only the steady-state cost.
    let _warmup = render_diff(&files);

    let start = Instant::now();
    let lines = render_diff(&files);
    let elapsed = start.elapsed();

    // Sanity: output should be roughly the input line count plus headers.
    assert!(
        lines.len() >= 5_000,
        "expected ≥5000 rendered lines, got {}",
        lines.len()
    );
    assert!(
        elapsed < Duration::from_millis(1_000),
        "render_diff(5k lines) took {elapsed:?}; budget is 1s (target <300ms)"
    );
    eprintln!("render_diff(5k lines) took {elapsed:?}");
}
