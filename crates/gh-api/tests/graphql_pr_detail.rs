//! Wiremock-backed integration test for the PR detail GraphQL fetcher.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gh_api::{Client, EtagCache};
use gh_core::{Mergeable, PrState, RepoRef, ReviewDecision};
use serde_json::json;
use wiremock::{
    matchers::{header, method, path_regex},
    Mock, MockServer, ResponseTemplate,
};

fn pr_detail_body(number: i64) -> serde_json::Value {
    json!({
        "data": {
            "repository": {
                "pullRequest": {
                    "number": number,
                    "title": "feat: example PR",
                    "body": "This is the description.",
                    "state": "OPEN",
                    "isDraft": false,
                    "mergeable": "MERGEABLE",
                    "author": { "__typename": "User", "login": "alice" },
                    "headRefName": "feat/example",
                    "baseRefName": "main",
                    "additions": 42,
                    "deletions": 7,
                    "reviewDecision": "APPROVED"
                }
            }
        }
    })
}

#[tokio::test]
async fn pr_detail_happy_path() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .and(header("authorization", "Bearer fake-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pr_detail_body(123)))
        .mount(&server)
        .await;

    let detail = gh_api::fetch_pr_detail(&client, &RepoRef::parse("foo/bar").unwrap(), 123)
        .await
        .unwrap();

    assert_eq!(detail.number, 123);
    assert_eq!(detail.title, "feat: example PR");
    assert_eq!(detail.body, "This is the description.");
    assert_eq!(detail.state, PrState::Open);
    assert!(!detail.draft);
    assert_eq!(detail.mergeable, Mergeable::Yes);
    assert_eq!(detail.author, "alice");
    assert_eq!(detail.head_ref, "feat/example");
    assert_eq!(detail.base_ref, "main");
    assert_eq!(detail.additions, 42);
    assert_eq!(detail.deletions, 7);
    assert_eq!(detail.review_decision, ReviewDecision::Approved);
}

#[tokio::test]
async fn pr_detail_repo_not_found() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    // GitHub returns 200 + null repository when the repo can't be found.
    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "repository": null }
        })))
        .mount(&server)
        .await;

    let err = gh_api::fetch_pr_detail(&client, &RepoRef::parse("foo/missing").unwrap(), 1)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("foo/missing"), "got: {msg}");
}

#[tokio::test]
async fn pr_detail_pr_not_found() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    // Repo present, PR null.
    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "repository": { "pullRequest": null }
            }
        })))
        .mount(&server)
        .await;

    let err = gh_api::fetch_pr_detail(&client, &RepoRef::parse("foo/bar").unwrap(), 999)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("999"), "got: {msg}");
    assert!(msg.contains("foo/bar"), "got: {msg}");
}

#[tokio::test]
async fn pr_detail_graphql_errors_array() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("POST"))
        .and(path_regex(r"^/graphql$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "errors": [{ "message": "rate limit exceeded", "type": "RATE_LIMITED" }]
        })))
        .mount(&server)
        .await;

    let err = gh_api::fetch_pr_detail(&client, &RepoRef::parse("foo/bar").unwrap(), 1)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("rate limit"), "got: {msg}");
}
