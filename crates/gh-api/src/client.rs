//! Thin wrapper around `octocrab::Octocrab`.
//!
//! Phase 3 keeps this minimal — just constructs the client with the resolved
//! token and host. PR #2 adds an ETag cache layer; PR #3 adds rate-limit
//! header extraction.

use std::sync::Arc;

use octocrab::Octocrab;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("invalid base URI for host `{host}`: {source}")]
    BadBaseUri {
        host: String,
        #[source]
        source: octocrab::Error,
    },
    #[error("octocrab build failed: {0}")]
    Build(#[from] octocrab::Error),
}

#[derive(Clone)]
pub struct Client {
    inner: Arc<Octocrab>,
}

impl Client {
    /// Build an octocrab instance bound to `token` and `host`. For GHE, the
    /// REST base URI is `https://<host>/api/v3`; for `github.com` octocrab's
    /// default is correct.
    pub fn new(token: &str, host: &str) -> Result<Self, ClientError> {
        let mut builder = Octocrab::builder().personal_token(token.to_string());
        if host != "github.com" {
            let base = format!("https://{host}/api/v3/");
            builder = builder
                .base_uri(base.clone())
                .map_err(|source| ClientError::BadBaseUri {
                    host: host.to_string(),
                    source,
                })?;
        }
        let octocrab = builder.build()?;
        Ok(Self {
            inner: Arc::new(octocrab),
        })
    }

    /// Borrow the underlying octocrab instance for direct API calls.
    #[must_use]
    pub fn octocrab(&self) -> &Octocrab {
        &self.inner
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}
