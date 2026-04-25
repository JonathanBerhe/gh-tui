//! Verify the `Link: rel="next"` header drives `PrPage::has_more`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gh_api::{Client, EtagCache};
use gh_core::RepoRef;
use serde_json::json;
use wiremock::{
    matchers::{method, path_regex, query_param},
    Mock, MockServer, ResponseTemplate,
};

fn empty_pulls() -> serde_json::Value {
    json!([])
}

#[tokio::test]
async fn first_page_with_next_link_sets_has_more_true() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    r#"<https://api.github.com/repos/foo/bar/pulls?page=2>; rel="next""#,
                )
                .set_body_json(empty_pulls()),
        )
        .mount(&server)
        .await;

    let p = gh_api::fetch_open_prs_page(&client, &RepoRef::parse("foo/bar").unwrap(), 1)
        .await
        .unwrap();
    assert_eq!(p.page, 1);
    assert!(p.has_more, "rel=next must set has_more");
}

#[tokio::test]
async fn last_page_without_next_link_sets_has_more_false() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    r#"<https://api.github.com/repos/foo/bar/pulls?page=1>; rel="prev""#,
                )
                .set_body_json(empty_pulls()),
        )
        .mount(&server)
        .await;

    let p = gh_api::fetch_open_prs_page(&client, &RepoRef::parse("foo/bar").unwrap(), 2)
        .await
        .unwrap();
    assert_eq!(p.page, 2);
    assert!(!p.has_more, "no rel=next means no more pages");
}

#[tokio::test]
async fn each_page_url_caches_independently() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let client = Client::new("fake-token", &server.uri(), cache).unwrap();
    let repo = RepoRef::parse("foo/bar").unwrap();

    // Page 1 with etag.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", r#""p1""#)
                .insert_header(
                    "link",
                    r#"<https://api.github.com/repos/foo/bar/pulls?page=2>; rel="next""#,
                )
                .set_body_json(empty_pulls()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let _ = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    server.verify().await;
    server.reset().await;

    // Page 2 with etag.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", r#""p2""#)
                .set_body_json(empty_pulls()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let _ = gh_api::fetch_open_prs_page(&client, &repo, 2)
        .await
        .unwrap();
    server.verify().await;
    server.reset().await;

    // Re-request page 1: should send the page-1 etag, not page-2's.
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .and(query_param("page", "1"))
        .and(wiremock::matchers::header("if-none-match", r#""p1""#))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    let _ = gh_api::fetch_open_prs_page(&client, &repo, 1)
        .await
        .unwrap();
    server.verify().await;
}
