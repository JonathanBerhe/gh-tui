//! Criterion benchmark: render a 5k-line synthetic diff. Target is
//! < 300 ms first paint on a 2020-era laptop.
//!
//! Run with `cargo bench -p gh-render`. A coarse-grained `#[test]`
//! mirror lives in `tests/perf.rs` so CI catches regressions without
//! needing the bench harness.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gh_core::{FilePatch, PatchStatus};
use gh_render::render_diff;

/// Build a single synthetic [`FilePatch`] containing roughly `target_lines`
/// rendered lines, split across `hunks` hunks. Each hunk is a tight
/// 1:1 modify (one `-`, one `+`) plus interleaved context to imitate a
/// realistic file diff.
fn synthetic_patch(target_lines: usize, hunks: usize) -> Vec<FilePatch> {
    let lines_per_hunk = target_lines / hunks;
    let mut body = String::new();
    for h in 0..hunks {
        body.push_str(&format!(
            "@@ -{0},{1} +{0},{1} @@\n",
            h * lines_per_hunk + 1,
            lines_per_hunk
        ));
        // Half context, then a paired -/+ pair, then more context.
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

fn bench_render_5k(c: &mut Criterion) {
    let files = synthetic_patch(5_000, 100);
    c.bench_function("render_diff_5k_lines", |b| {
        b.iter(|| {
            let lines = render_diff(black_box(&files));
            black_box(lines);
        });
    });
}

criterion_group!(benches, bench_render_5k);
criterion_main!(benches);
