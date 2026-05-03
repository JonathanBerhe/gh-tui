//! Snapshot tests for the split-view renderer. Each row pair is flattened
//! to plain "left | right" text so insta can diff the layout shape across
//! changes. Token-level styling is exercised by the unit tests in
//! `src/diff/split.rs`; this file focuses on row-level alignment.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use gh_core::{FilePatch, PatchStatus};
use gh_render::render_diff_split;
use ratatui::text::Line;

fn flatten_pair((left, right): &(Line<'static>, Line<'static>)) -> String {
    let l: String = left.spans.iter().map(|s| s.content.as_ref()).collect();
    let r: String = right.spans.iter().map(|s| s.content.as_ref()).collect();
    format!("{l:60} │ {r}")
}

fn flatten(files: &[FilePatch]) -> String {
    render_diff_split(files)
        .iter()
        .map(flatten_pair)
        .collect::<Vec<_>>()
        .join("\n")
}

fn fixture(name: &str) -> String {
    let path = format!("tests/fixtures/diff/{name}.patch");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

#[test]
fn split_simple_balanced() {
    // word_simple.patch is a 1:1 paired block — split lays it as
    //   ctx          | ctx
    //   old_line     | new_line
    //   trailing     | trailing
    let files = vec![FilePatch {
        path: "src/word_simple.rs".into(),
        previous_path: None,
        status: PatchStatus::Modified,
        additions: 1,
        deletions: 1,
        patch: Some(fixture("word_simple")),
        blob_sha: "x".into(),
    }];
    insta::assert_snapshot!(flatten(&files));
}

#[test]
fn split_unbalanced_uses_filler() {
    // word_unbalanced_block.patch is 3 - + 1 + → minuses pair with filler
    // on the right, plus pairs with filler on the left.
    let files = vec![FilePatch {
        path: "src/word_unbalanced.rs".into(),
        previous_path: None,
        status: PatchStatus::Modified,
        additions: 1,
        deletions: 3,
        patch: Some(fixture("word_unbalanced_block")),
        blob_sha: "x".into(),
    }];
    insta::assert_snapshot!(flatten(&files));
}

#[test]
fn split_pure_add_uses_filler_left() {
    // word_pure_add.patch is context + 2 unpaired adds.
    let files = vec![FilePatch {
        path: "src/word_pure_add.rs".into(),
        previous_path: None,
        status: PatchStatus::Modified,
        additions: 2,
        deletions: 0,
        patch: Some(fixture("word_pure_add")),
        blob_sha: "x".into(),
    }];
    insta::assert_snapshot!(flatten(&files));
}
