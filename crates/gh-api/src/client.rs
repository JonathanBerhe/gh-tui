//! Authenticated GitHub HTTP client with ETag-conditional GETs.
//!
//! Uses `reqwest` directly (rather than octocrab's typed wrappers) so we have
//! full control over the `If-None-Match` flow and the response headers.
//! Octocrab is still pulled in for its `models::*` deserialization shapes
//! and may host higher-level helpers in later phases.

use std::sync::Arc;

use chrono::DateTime;
use gh_core::{Msg, RateLimit};
use reqwest::{
    header::{HeaderMap, ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH, USER_AGENT},
    StatusCode,
};
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::sync::mpsc::Sender;
use tracing::{debug, instrument};

use crate::cache::EtagCache;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("invalid host `{0}`: {1}")]
    BadHost(String, String),
    #[error("reqwest build failed: {0}")]
    Build(#[from] reqwest::Error),
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("decode error: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("404 Not Found")]
    NotFound,
    #[error("server returned {0}")]
    Status(u16),
    #[error("304 received but cache had no entry for {0}")]
    StaleNotModified(String),
    #[error("graphql errors: {0}")]
    GraphQl(String),
    #[error("graphql request failed: {0}")]
    GraphQlRequest(String),
}

/// A successful API response: parsed body plus metadata extracted from the
/// response (currently just whether a `rel="next"` Link is present).
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub body: T,
    pub has_next: bool,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    /// Pre-rendered authorization header value (`Bearer <token>`).
    auth: String,
    /// REST base URL with trailing slash, e.g. `https://api.github.com/` or
    /// `https://ghe.example.com/api/v3/`. For tests, can be the wiremock
    /// URI directly (`http://127.0.0.1:1234/`).
    base: String,
    /// GraphQL endpoint URL (no trailing slash):
    /// `https://api.github.com/graphql` for github.com,
    /// `https://<host>/api/graphql` for GHE,
    /// `<host>/graphql` when host already has a scheme (test mode).
    graphql_url: String,
    cache: Arc<EtagCache>,
    /// Optional sink for [`Msg::RateLimitUpdate`] events extracted from
    /// response headers. `None` in tests; `Some(tx)` once the binary's
    /// channel is wired in via [`Self::with_tx`].
    tx: Option<Sender<Msg>>,
}

impl Client {
    /// Build a client. `host` is one of:
    /// - `"github.com"` → `https://api.github.com/`
    /// - `"ghe.example.com"` → `https://ghe.example.com/api/v3/`
    /// - any value starting with `http://` or `https://` → used verbatim
    ///   (test mode against a mock server)
    pub fn new(token: &str, host: &str, cache: Arc<EtagCache>) -> Result<Self, ClientError> {
        let base = derive_base_url(host);
        let graphql_url = derive_graphql_url(host);
        let http = reqwest::Client::builder()
            .user_agent(concat!("gh-tui/", env!("CARGO_PKG_VERSION")))
            .gzip(true)
            .build()?;
        Ok(Self {
            http,
            auth: format!("Bearer {token}"),
            base,
            graphql_url,
            cache,
            tx: None,
        })
    }

    /// Attach a channel for rate-limit updates. The binary calls this once
    /// after constructing the client; tests skip it.
    #[must_use]
    pub fn with_tx(mut self, tx: Sender<Msg>) -> Self {
        self.tx = Some(tx);
        self
    }

    /// Conditional GET with ETag caching. `path` is appended to the base URL.
    ///
    /// Returns the parsed body alongside `has_next` — true iff the response's
    /// `Link` header advertises a `rel="next"` page. Callers that don't care
    /// about pagination can ignore the bool.
    ///
    /// On cache hit + 304: deserializes from the cached body, but `has_next`
    /// still comes from the live response (GitHub sends `Link` on 304).
    /// On cache miss / etag mismatch: fetches, stores etag+body, deserializes.
    #[instrument(skip(self), fields(path = %path))]
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<Page<T>, ApiError> {
        let url = self.url(path);

        let cached = self.cache.get(&url).await;

        let mut req = self
            .http
            .get(&url)
            .header(AUTHORIZATION, &self.auth)
            .header(ACCEPT, "application/vnd.github+json")
            .header(USER_AGENT, concat!("gh-tui/", env!("CARGO_PKG_VERSION")));

        if let Some(c) = &cached {
            req = req.header(IF_NONE_MATCH, &c.etag);
        }

        let resp = req.send().await?;
        let status = resp.status();
        debug!(%status, has_cache = cached.is_some(), "got response");

        // Surface rate-limit info from any response (200 / 304 / errors all
        // carry the headers). `try_send` so a busy channel never blocks API
        // calls — dropping a single update is harmless.
        if let Some(rl) = parse_rate_limit(resp.headers()) {
            if let Some(tx) = &self.tx {
                let _ = tx.try_send(Msg::RateLimitUpdate(rl));
            }
        }

        let has_next = has_next_link(resp.headers());

        if status == StatusCode::NOT_MODIFIED {
            return match cached {
                Some(c) => Ok(Page {
                    body: serde_json::from_slice(&c.body)?,
                    has_next,
                }),
                None => Err(ApiError::StaleNotModified(url)),
            };
        }

        if status == StatusCode::NOT_FOUND {
            return Err(ApiError::NotFound);
        }

        if !status.is_success() {
            return Err(ApiError::Status(status.as_u16()));
        }

        let etag = resp
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let body = resp.bytes().await?;

        if let Some(etag) = etag {
            if let Err(e) = self.cache.put(&url, &etag, &body).await {
                debug!(error = %e, "cache put failed");
            }
        }

        Ok(Page {
            body: serde_json::from_slice(&body)?,
            has_next,
        })
    }

    fn url(&self, path: &str) -> String {
        let trimmed = path.trim_start_matches('/');
        format!("{}{}", self.base, trimmed)
    }

    /// Issue a GraphQL request, parse the response as `Q`.
    ///
    /// Hand-rolled rather than using `cynic::http::ReqwestExt` because that
    /// helper currently expects a different reqwest major than ours.
    /// Surfaces rate-limit headers via the optional `tx` channel.
    #[instrument(skip(self, vars))]
    pub async fn graphql<Q, V>(&self, vars: V) -> Result<Q, ApiError>
    where
        Q: cynic::QueryBuilder<V> + serde::de::DeserializeOwned + 'static,
        V: cynic::QueryVariables + serde::Serialize,
    {
        let operation = Q::build(vars);
        let body = serde_json::to_vec(&operation)?;

        let resp = self
            .http
            .post(&self.graphql_url)
            .header(AUTHORIZATION, &self.auth)
            .header(USER_AGENT, concat!("gh-tui/", env!("CARGO_PKG_VERSION")))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await?;

        let status = resp.status();

        // Surface rate-limit headers from any GraphQL response.
        if let Some(rl) = parse_rate_limit(resp.headers()) {
            if let Some(tx) = &self.tx {
                let _ = tx.try_send(Msg::RateLimitUpdate(rl));
            }
        }

        if !status.is_success() {
            return Err(ApiError::Status(status.as_u16()));
        }

        let bytes = resp.bytes().await?;
        let parsed: cynic::GraphQlResponse<Q> = serde_json::from_slice(&bytes)?;

        if let Some(data) = parsed.data {
            if let Some(errors) = parsed.errors.as_ref() {
                if !errors.is_empty() {
                    debug!(?errors, "graphql errors alongside data");
                }
            }
            return Ok(data);
        }

        let errs = parsed
            .errors
            .map(|es| {
                es.into_iter()
                    .map(|e| e.message)
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_else(|| "no data and no errors returned".to_string());
        Err(ApiError::GraphQl(errs))
    }
}

/// True iff the response's `Link` header advertises `rel="next"`.
fn has_next_link(headers: &HeaderMap) -> bool {
    headers
        .get("link")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.split(',').any(|part| part.contains(r#"rel="next""#)))
}

/// Parse the `x-ratelimit-{remaining,limit,reset}` headers into a [`RateLimit`].
/// Returns `None` if any of the three is missing or malformed — the rate-limit
/// indicator just won't update for that response.
fn parse_rate_limit(headers: &HeaderMap) -> Option<RateLimit> {
    let h = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    };
    let remaining = h("x-ratelimit-remaining")?.parse::<u32>().ok()?;
    let limit = h("x-ratelimit-limit")?.parse::<u32>().ok()?;
    let reset_secs = h("x-ratelimit-reset")?.parse::<i64>().ok()?;
    let reset_at = DateTime::from_timestamp(reset_secs, 0)?;
    Some(RateLimit {
        remaining,
        limit,
        reset_at,
    })
}

fn derive_base_url(host: &str) -> String {
    if host.starts_with("http://") || host.starts_with("https://") {
        let mut s = host.to_string();
        if !s.ends_with('/') {
            s.push('/');
        }
        s
    } else if host == "github.com" {
        "https://api.github.com/".to_string()
    } else {
        format!("https://{host}/api/v3/")
    }
}

/// GraphQL endpoint URL. NOT under `/api/v3/` for GHE — that's the REST root;
/// GraphQL lives at `<host>/api/graphql`.
fn derive_graphql_url(host: &str) -> String {
    if host.starts_with("http://") || host.starts_with("https://") {
        let mut s = host.trim_end_matches('/').to_string();
        s.push_str("/graphql");
        s
    } else if host == "github.com" {
        "https://api.github.com/graphql".to_string()
    } else {
        format!("https://{host}/api/graphql")
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("base", &self.base).finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn base_url_for_github_com() {
        assert_eq!(derive_base_url("github.com"), "https://api.github.com/");
    }

    #[test]
    fn base_url_for_ghe() {
        assert_eq!(
            derive_base_url("ghe.example.com"),
            "https://ghe.example.com/api/v3/"
        );
    }

    #[test]
    fn base_url_passes_through_explicit_scheme() {
        assert_eq!(
            derive_base_url("http://127.0.0.1:1234"),
            "http://127.0.0.1:1234/"
        );
        assert_eq!(
            derive_base_url("http://127.0.0.1:1234/"),
            "http://127.0.0.1:1234/"
        );
    }

    #[test]
    fn has_next_link_detects_next_rel() {
        let mut h = HeaderMap::new();
        h.insert(
            "link",
            r#"<https://api.github.com/x?page=2>; rel="next", <https://api.github.com/x?page=5>; rel="last""#
                .parse()
                .unwrap(),
        );
        assert!(has_next_link(&h));
    }

    #[test]
    fn has_next_link_false_when_only_prev() {
        let mut h = HeaderMap::new();
        h.insert(
            "link",
            r#"<https://api.github.com/x?page=1>; rel="prev""#.parse().unwrap(),
        );
        assert!(!has_next_link(&h));
    }

    #[test]
    fn has_next_link_false_when_missing() {
        assert!(!has_next_link(&HeaderMap::new()));
    }

    #[test]
    fn graphql_url_for_github_com() {
        assert_eq!(
            derive_graphql_url("github.com"),
            "https://api.github.com/graphql"
        );
    }

    #[test]
    fn graphql_url_for_ghe_uses_api_graphql_not_v3() {
        // GHE GraphQL is at /api/graphql, NOT /api/v3/graphql.
        assert_eq!(
            derive_graphql_url("ghe.example.com"),
            "https://ghe.example.com/api/graphql"
        );
    }

    #[test]
    fn graphql_url_passes_through_explicit_scheme() {
        assert_eq!(
            derive_graphql_url("http://127.0.0.1:1234"),
            "http://127.0.0.1:1234/graphql"
        );
        assert_eq!(
            derive_graphql_url("http://127.0.0.1:1234/"),
            "http://127.0.0.1:1234/graphql"
        );
    }
}
