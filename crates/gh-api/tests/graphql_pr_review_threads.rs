//! Wiremock-backed integration tests for `fetch_pr_review_threads`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gh_api::{Client, EtagCache, PrReviewThreadsError};
use gh_core::RepoRef;
use serde_json::json;
use wiremock::{
    matchers::{method, path_regex},
    Mock, MockServer, ResponseTemplate,
};

fn body(threads: serde_json::Value) -> serde_json::Value {
    json!({
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": { "nodes": threads }
                }
            }
        }
    })
}

#[tokio::test]
async fn happy_path_maps_threads_and_comments() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body(json!([
            {
                "path": "src/lib.rs",
                "line": 42,
                "originalLine": 42,
                "comments": { "nodes": [
                    {
                        "author": { "__typename": "User", "login": "alice" },
                        "body": "looks good",
                        "createdAt": "2026-04-30T10:00:00Z"
                    },
                    {
                        "author": { "__typename": "User", "login": "bob" },
                        "body": "second the above",
                        "createdAt": "2026-04-30T11:00:00Z"
                    }
                ]}
            }
        ]))))
        .expect(1)
        .mount(&server)
        .await;

    let threads = gh_api::fetch_pr_review_threads(&client, &RepoRef::parse("foo/bar").unwrap(), 7)
        .await
        .unwrap();
    server.verify().await;

    assert_eq!(threads.len(), 1);
    let t = &threads[0];
    assert_eq!(t.path, "src/lib.rs");
    assert_eq!(t.line, Some(42));
    assert_eq!(t.original_line, Some(42));
    assert_eq!(t.comments.len(), 2);
    assert_eq!(t.comments[0].author, "alice");
    assert_eq!(t.comments[0].body, "looks good");
    assert_eq!(t.comments[1].author, "bob");
}

#[tokio::test]
async fn outdated_thread_keeps_none_line() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body(json!([
            {
                "path": "src/old.rs",
                "line": null,
                "originalLine": 99,
                "comments": { "nodes": [
                    {
                        "author": { "__typename": "User", "login": "alice" },
                        "body": "stale",
                        "createdAt": "2026-04-30T10:00:00Z"
                    }
                ]}
            }
        ]))))
        .mount(&server)
        .await;

    let threads = gh_api::fetch_pr_review_threads(&client, &RepoRef::parse("foo/bar").unwrap(), 7)
        .await
        .unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].line, None);
    assert_eq!(threads[0].original_line, Some(99));
}

#[tokio::test]
async fn zero_threads_returns_empty_vec() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body(json!([]))))
        .mount(&server)
        .await;

    let threads = gh_api::fetch_pr_review_threads(&client, &RepoRef::parse("foo/bar").unwrap(), 7)
        .await
        .unwrap();
    assert!(threads.is_empty());
}

#[tokio::test]
async fn null_author_falls_back_to_ghost() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body(json!([
            {
                "path": "src/lib.rs",
                "line": 1,
                "originalLine": 1,
                "comments": { "nodes": [
                    {
                        "author": null,
                        "body": "from a deleted account",
                        "createdAt": "2026-04-30T10:00:00Z"
                    }
                ]}
            }
        ]))))
        .mount(&server)
        .await;

    let threads = gh_api::fetch_pr_review_threads(&client, &RepoRef::parse("foo/bar").unwrap(), 7)
        .await
        .unwrap();
    assert_eq!(threads[0].comments[0].author, "<ghost>");
}

#[tokio::test]
async fn missing_repo_maps_to_repo_not_found() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "repository": null }
        })))
        .mount(&server)
        .await;

    let err = gh_api::fetch_pr_review_threads(&client, &RepoRef::parse("missing/repo").unwrap(), 7)
        .await
        .unwrap_err();
    assert!(matches!(err, PrReviewThreadsError::RepoNotFound(_)));
}
