//! End-to-end test of the ETag cache flow against a wiremock server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gh_api::{Client, EtagCache};
use gh_core::RepoRef;
use serde_json::json;
use wiremock::{
    matchers::{header, method, path_regex, query_param},
    Mock, MockServer, ResponseTemplate,
};

/// Helper: builds a Client pointed at the wiremock server, sharing one
/// in-memory cache.
fn build_client(server: &MockServer, cache: Arc<EtagCache>) -> Client {
    Client::new("fake-token", &server.uri(), cache).unwrap()
}

fn pulls_body() -> serde_json::Value {
    json!([
        {
            "number": 42,
            "title": "test PR",
            "state": "open",
            "draft": false,
            "user": { "login": "alice", "id": 1, "type": "User", "node_id": "n", "url": "https://example.com/u", "avatar_url": "https://example.com/a", "gravatar_id": "", "html_url": "https://example.com/h", "followers_url": "https://example.com", "following_url": "https://example.com", "gists_url": "https://example.com", "starred_url": "https://example.com", "subscriptions_url": "https://example.com", "organizations_url": "https://example.com", "repos_url": "https://example.com", "events_url": "https://example.com", "received_events_url": "https://example.com", "site_admin": false },
            "head": { "ref": "feat", "sha": "abc", "label": "alice:feat", "user": null, "repo": null },
            "base": { "ref": "main", "sha": "def", "label": "owner:main", "user": null, "repo": null },
            "comments": 3,
            "additions": 10,
            "deletions": 2,
            "created_at": "2026-04-25T10:00:00Z",
            "url": "https://example.com",
            "id": 1,
            "node_id": "n",
            "html_url": "https://example.com",
            "diff_url": "https://example.com",
            "patch_url": "https://example.com",
            "issue_url": "https://example.com",
            "commits_url": "https://example.com",
            "review_comments_url": "https://example.com",
            "review_comment_url": "https://example.com",
            "comments_url": "https://example.com",
            "statuses_url": "https://example.com",
            "locked": false
        }
    ])
}

#[tokio::test]
async fn cache_miss_then_hit_returns_304() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = build_client(&server, cache.clone());

    // First call: server responds 200 with ETag.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(query_param("state", "open"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", r#"W/"abc123""#)
                .set_body_json(pulls_body()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let repo = RepoRef::parse("foo/bar").unwrap();
    let prs = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    assert_eq!(prs.items.len(), 1);
    assert_eq!(prs.items[0].number, 42);
    assert_eq!(prs.page, 1);

    server.verify().await;
    server.reset().await;

    // Second call: server should see If-None-Match and respond 304.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(header("if-none-match", r#"W/"abc123""#))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;

    let prs2 = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    assert_eq!(prs2, prs, "304 should yield the cached body");

    server.verify().await;
}

#[tokio::test]
async fn new_etag_overwrites_old_cache_entry() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = build_client(&server, cache.clone());
    let repo = RepoRef::parse("foo/bar").unwrap();

    // First mock: 200 with etag v1.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", r#""v1""#)
                .set_body_json(pulls_body()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let _ = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    server.verify().await;
    server.reset().await;

    // Second call: server returns 200 with new etag v2 (cache is stale).
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(header("if-none-match", r#""v1""#))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", r#""v2""#)
                .set_body_json(pulls_body()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let _ = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    server.verify().await;

    // Third call should now send the v2 etag.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(header("if-none-match", r#""v2""#))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    let _ = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn missing_etag_header_skips_caching() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = build_client(&server, cache.clone());
    let repo = RepoRef::parse("foo/bar").unwrap();

    // First and second calls both succeed with no ETag — cache stays empty,
    // each call hits the server.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pulls_body()))
        .expect(2)
        .mount(&server)
        .await;

    let _ = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    let _ = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn not_found_maps_to_pulls_error() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = build_client(&server, cache);
    let repo = RepoRef::parse("foo/missing").unwrap();

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/missing/pulls$"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    match gh_api::fetch_open_prs_page(&client, &repo, 1).await {
        Err(gh_api::PullsError::NotFound(slug)) => assert_eq!(slug, "foo/missing"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}
