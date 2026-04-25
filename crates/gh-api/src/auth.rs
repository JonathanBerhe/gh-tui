//! Auth detection — shells out to `gh auth token`, respects `GH_TOKEN` /
//! `GITHUB_TOKEN` / `GH_HOST` env vars.
//!
//! Host resolution is minimal in Phase 1: `GH_HOST` env or `github.com`.
//! Phase 2 parses `gh auth status` for full host + user detection.

use std::env;

use tokio::process::Command;
use tracing::{debug, instrument, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthOutcome {
    Token {
        token: String,
        host: String,
        user: Option<String>,
    },
    Missing {
        reason: String,
    },
}

impl AuthOutcome {
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Token { .. })
    }
}

fn host_from_env() -> String {
    env::var("GH_HOST")
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "github.com".to_string())
}

fn env_token() -> Option<String> {
    ["GH_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find_map(|k| env::var(k).ok().filter(|v| !v.is_empty()))
}

/// Detect the active auth context.
///
/// Resolution order:
/// 1. `GH_TOKEN` or `GITHUB_TOKEN` env vars.
/// 2. Shell out to `gh auth token`.
/// 3. Otherwise, `Missing`.
#[instrument(skip_all)]
pub async fn detect_auth() -> AuthOutcome {
    let host = host_from_env();

    if let Some(token) = env_token() {
        debug!(%host, "auth via env var");
        return AuthOutcome::Token {
            token,
            host,
            user: None,
        };
    }

    match run_gh_auth_token().await {
        Ok(Some(token)) => {
            debug!(%host, "auth via `gh auth token`");
            AuthOutcome::Token {
                token,
                host,
                user: None,
            }
        }
        Ok(None) => AuthOutcome::Missing {
            reason: "run `gh auth login`".to_string(),
        },
        Err(reason) => {
            warn!(%reason, "gh auth token failed");
            AuthOutcome::Missing { reason }
        }
    }
}

async fn run_gh_auth_token() -> Result<Option<String>, String> {
    let output = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "`gh` not installed".to_string(),
            other => format!("failed to run `gh`: {other}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim();
        return Err(if msg.is_empty() {
            format!("gh exited with {}", output.status)
        } else {
            msg.to_string()
        });
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        Ok(None)
    } else {
        Ok(Some(token))
    }
}
