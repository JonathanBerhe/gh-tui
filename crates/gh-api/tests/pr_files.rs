//! Wiremock coverage for `fetch_pr_files` (REST `/pulls/{n}/files`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gh_api::{Client, EtagCache, PrFilesError};
use gh_core::{PatchStatus, RepoRef};
use serde_json::json;
use wiremock::{
    matchers::{method, path_regex, query_param},
    Mock, MockServer, ResponseTemplate,
};

fn one_file_payload() -> serde_json::Value {
    json!([
        {
            "sha": "deadbeef",
            "filename": "src/lib.rs",
            "status": "modified",
            "additions": 4,
            "deletions": 1,
            "changes": 5,
            "patch": "@@ -1,3 +1,4 @@\n a\n-b\n+B\n c"
        }
    ])
}

#[tokio::test]
async fn happy_path_maps_response_to_file_patch() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls/42/files$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(one_file_payload()))
        .expect(1)
        .mount(&server)
        .await;

    let files = gh_api::fetch_pr_files(&client, &RepoRef::parse("foo/bar").unwrap(), 42)
        .await
        .unwrap();
    server.verify().await;

    assert_eq!(files.len(), 1);
    let f = &files[0];
    assert_eq!(f.path, "src/lib.rs");
    assert_eq!(f.previous_path, None);
    assert_eq!(f.status, PatchStatus::Modified);
    assert_eq!(f.additions, 4);
    assert_eq!(f.deletions, 1);
    assert_eq!(f.blob_sha, "deadbeef");
    assert!(f.patch.as_ref().unwrap().contains("@@ -1,3 +1,4 @@"));
}

#[tokio::test]
async fn renamed_file_keeps_previous_filename() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls/7/files$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "sha": "abc",
                "filename": "src/new.rs",
                "previous_filename": "src/old.rs",
                "status": "renamed",
                "additions": 0,
                "deletions": 0,
                "changes": 0
            }
        ])))
        .mount(&server)
        .await;

    let files = gh_api::fetch_pr_files(&client, &RepoRef::parse("foo/bar").unwrap(), 7)
        .await
        .unwrap();
    let f = &files[0];
    assert_eq!(f.path, "src/new.rs");
    assert_eq!(f.previous_path.as_deref(), Some("src/old.rs"));
    assert_eq!(f.status, PatchStatus::Renamed);
    // GitHub omits `patch` for pure renames (no content change).
    assert!(f.patch.is_none());
}

#[tokio::test]
async fn missing_patch_field_yields_none() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls/9/files$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "sha": "abc",
                "filename": "vendor/big.bin",
                "status": "modified",
                "additions": 0,
                "deletions": 0,
                "changes": 0
            }
        ])))
        .mount(&server)
        .await;

    let files = gh_api::fetch_pr_files(&client, &RepoRef::parse("foo/bar").unwrap(), 9)
        .await
        .unwrap();
    assert_eq!(files[0].patch, None);
}

#[tokio::test]
async fn paginated_response_concatenates_all_pages() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    // Page 1: one file, advertise next page via Link header.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls/3/files$"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    r#"<https://api.github.com/repos/foo/bar/pulls/3/files?page=2>; rel="next""#,
                )
                .set_body_json(json!([
                    {"sha":"a","filename":"a.rs","status":"modified","additions":1,"deletions":0,"changes":1}
                ])),
        )
        .expect(1)
        .mount(&server)
        .await;

    // Page 2: one file, no Link header → loop terminates.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls/3/files$"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"sha":"b","filename":"b.rs","status":"modified","additions":1,"deletions":0,"changes":1}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let files = gh_api::fetch_pr_files(&client, &RepoRef::parse("foo/bar").unwrap(), 3)
        .await
        .unwrap();
    server.verify().await;

    assert_eq!(files.len(), 2, "both pages concatenated");
    assert_eq!(files[0].path, "a.rs");
    assert_eq!(files[1].path, "b.rs");
}

#[tokio::test]
async fn not_found_maps_to_pr_files_error() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls/404/files$"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;

    let err = gh_api::fetch_pr_files(&client, &RepoRef::parse("foo/bar").unwrap(), 404)
        .await
        .unwrap_err();
    assert!(matches!(err, PrFilesError::NotFound(404)));
}
