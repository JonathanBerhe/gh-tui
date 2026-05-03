//! Snapshot tests for the diff renderer. Each fixture is a raw patch chunk
//! wrapped into a single `FilePatch`; the snapshot is the lossy plain-text
//! rendering. Review with `cargo insta review`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use gh_core::{FilePatch, PatchStatus};
use gh_render::render_diff;

fn flatten(files: &[FilePatch]) -> String {
    let lines = render_diff(files);
    lines
        .into_iter()
        .map(|l| {
            l.spans
                .into_iter()
                .map(|s| s.content.into_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fixture(name: &str) -> String {
    let path = format!("tests/fixtures/diff/{name}.patch");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn one_file(name: &str, status: PatchStatus, additions: u32, deletions: u32) -> Vec<FilePatch> {
    vec![FilePatch {
        path: format!("src/{name}.rs"),
        previous_path: None,
        status,
        additions,
        deletions,
        patch: Some(fixture(name)),
        blob_sha: "deadbeef".into(),
    }]
}

fn one_file_at(
    name: &str,
    path: &str,
    status: PatchStatus,
    additions: u32,
    deletions: u32,
) -> Vec<FilePatch> {
    vec![FilePatch {
        path: path.into(),
        previous_path: None,
        status,
        additions,
        deletions,
        patch: Some(fixture(name)),
        blob_sha: "deadbeef".into(),
    }]
}

#[test]
fn simple() {
    insta::assert_snapshot!(flatten(&one_file("simple", PatchStatus::Modified, 1, 1)));
}

#[test]
fn added() {
    insta::assert_snapshot!(flatten(&one_file("added", PatchStatus::Added, 3, 0)));
}

#[test]
fn multi_hunk() {
    insta::assert_snapshot!(flatten(&one_file(
        "multi_hunk",
        PatchStatus::Modified,
        2,
        1
    )));
}

#[test]
fn omitted_patch_renders_placeholder() {
    let files = vec![FilePatch {
        path: "vendor/big_blob.bin".into(),
        previous_path: None,
        status: PatchStatus::Modified,
        additions: 0,
        deletions: 0,
        patch: None,
        blob_sha: "deadbeef".into(),
    }];
    insta::assert_snapshot!(flatten(&files));
}

#[test]
fn rust_modified() {
    insta::assert_snapshot!(flatten(&one_file_at(
        "rust_modified",
        "src/count.rs",
        PatchStatus::Modified,
        4,
        2,
    )));
}

#[test]
fn unknown_ext() {
    insta::assert_snapshot!(flatten(&one_file_at(
        "unknown_ext",
        "Cargo.toml",
        PatchStatus::Modified,
        2,
        1,
    )));
}

#[test]
fn mixed_lang() {
    let files = vec![
        FilePatch {
            path: "src/count.rs".into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 4,
            deletions: 2,
            patch: Some(fixture("rust_modified")),
            blob_sha: "rs".into(),
        },
        FilePatch {
            path: "Cargo.toml".into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 2,
            deletions: 1,
            patch: Some(fixture("unknown_ext")),
            blob_sha: "toml".into(),
        },
    ];
    insta::assert_snapshot!(flatten(&files));
}

#[test]
fn word_simple() {
    insta::assert_snapshot!(flatten(&one_file_at(
        "word_simple",
        "src/word_simple.rs",
        PatchStatus::Modified,
        1,
        1,
    )));
}

#[test]
fn word_pure_add() {
    insta::assert_snapshot!(flatten(&one_file_at(
        "word_pure_add",
        "src/word_pure_add.rs",
        PatchStatus::Modified,
        2,
        0,
    )));
}

#[test]
fn word_unbalanced_block() {
    insta::assert_snapshot!(flatten(&one_file_at(
        "word_unbalanced_block",
        "src/word_unbalanced.rs",
        PatchStatus::Modified,
        1,
        3,
    )));
}

#[test]
fn multiple_files_separated_by_blank_line() {
    let files = vec![
        FilePatch {
            path: "src/a.rs".into(),
            previous_path: None,
            status: PatchStatus::Modified,
            additions: 1,
            deletions: 1,
            patch: Some(fixture("simple")),
            blob_sha: "aa".into(),
        },
        FilePatch {
            path: "src/b.rs".into(),
            previous_path: None,
            status: PatchStatus::Added,
            additions: 3,
            deletions: 0,
            patch: Some(fixture("added")),
            blob_sha: "bb".into(),
        },
    ];
    insta::assert_snapshot!(flatten(&files));
}
