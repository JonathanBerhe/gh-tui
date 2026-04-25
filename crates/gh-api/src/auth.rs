//! Auth detection — shells out to `gh auth token`, respects `GH_TOKEN` /
//! `GITHUB_TOKEN` / `GH_HOST` env vars, and looks up the active username
//! via `gh api user`.
//!
//! Host resolution remains minimal: `GH_HOST` env or `github.com`. GHE users
//! who haven't set `GH_HOST` will see "github.com" in the status bar even
//! though API calls work — full multi-host detection is deferred.

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
/// 1. `GH_TOKEN` or `GITHUB_TOKEN` env vars (token).
/// 2. Shell out to `gh auth token` (token).
/// 3. Otherwise, `Missing`.
///
/// In parallel with the token resolution, also calls `gh api user` to look
/// up the active username. The user lookup is best-effort; failure (e.g. no
/// network, GHE quirk) returns `user: None` and still reports authenticated.
#[instrument(skip_all)]
pub async fn detect_auth() -> AuthOutcome {
    let host = host_from_env();

    // If we have a token in env, no need to shell out for it; still look up
    // the username in parallel.
    if let Some(token) = env_token() {
        let user = lookup_user().await;
        debug!(%host, ?user, "auth via env var");
        return AuthOutcome::Token { token, host, user };
    }

    // No env token: resolve the token and the username concurrently. They
    // both shell out to `gh`, so total wall time stays close to one call.
    let (token_res, user) = tokio::join!(run_gh_auth_token(), lookup_user());

    match token_res {
        Ok(Some(token)) => {
            debug!(%host, ?user, "auth via `gh auth token`");
            AuthOutcome::Token { token, host, user }
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

/// Best-effort lookup of the active username via `gh api user --jq .login`.
/// Returns `None` on any failure (gh missing, network down, JSON shape change).
async fn lookup_user() -> Option<String> {
    let output = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let login = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if login.is_empty() {
        None
    } else {
        Some(login)
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
