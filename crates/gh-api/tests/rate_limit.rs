//! Verify the client extracts `x-ratelimit-*` headers and posts a
//! `Msg::RateLimitUpdate` to the bound channel.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use gh_api::{Client, EtagCache};
use gh_core::{Msg, RepoRef};
use serde_json::json;
use tokio::sync::mpsc;
use wiremock::{
    matchers::{method, path_regex},
    Mock, MockServer, ResponseTemplate,
};

fn empty_pulls() -> serde_json::Value {
    json!([])
}

#[tokio::test]
async fn rate_limit_headers_are_posted_to_channel() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let (tx, mut rx) = mpsc::channel::<Msg>(8);

    let client = Client::new("fake-token", &server.uri(), cache)
        .unwrap()
        .with_tx(tx);

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-limit", "5000")
                .insert_header("x-ratelimit-remaining", "4998")
                .insert_header("x-ratelimit-reset", "1800000000")
                .set_body_json(empty_pulls()),
        )
        .mount(&server)
        .await;

    let _ = gh_api::fetch_open_prs_page(&client, &RepoRef::parse("foo/bar").unwrap(), 1)
        .await
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("rate-limit msg must arrive within 1s")
        .expect("channel must not close");

    match msg {
        Msg::RateLimitUpdate(rl) => {
            assert_eq!(rl.remaining, 4998);
            assert_eq!(rl.limit, 5000);
        }
        other => panic!("expected RateLimitUpdate, got {other:?}"),
    }
}

#[tokio::test]
async fn missing_rate_limit_headers_post_nothing() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let (tx, mut rx) = mpsc::channel::<Msg>(8);
    let client = Client::new("fake-token", &server.uri(), cache)
        .unwrap()
        .with_tx(tx);

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_pulls()))
        .mount(&server)
        .await;

    let _ = gh_api::fetch_open_prs_page(&client, &RepoRef::parse("foo/bar").unwrap(), 1)
        .await
        .unwrap();

    // Brief grace period — channel should stay empty.
    let result = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        result.is_err(),
        "no msg should be posted when headers missing"
    );
}

#[tokio::test]
async fn malformed_rate_limit_headers_are_skipped() {
    let server = MockServer::start().await;
    let cache = Arc::new(EtagCache::in_memory());
    let (tx, mut rx) = mpsc::channel::<Msg>(8);
    let client = Client::new("fake-token", &server.uri(), cache)
        .unwrap()
        .with_tx(tx);

    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/foo/bar/pulls$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-limit", "five-thousand")
                .insert_header("x-ratelimit-remaining", "lots")
                .insert_header("x-ratelimit-reset", "soon")
                .set_body_json(empty_pulls()),
        )
        .mount(&server)
        .await;

    let _ = gh_api::fetch_open_prs_page(&client, &RepoRef::parse("foo/bar").unwrap(), 1)
        .await
        .unwrap();

    let result = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        result.is_err(),
        "malformed headers should be silently skipped"
    );
}
