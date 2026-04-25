//! Async command workers. A reducer emits `Cmd`s; `dispatch` turns each one
//! into a task that runs the side effect and posts a `Msg` back on the mpsc
//! channel. One match arm per `Cmd` variant; future commands add branches.

use std::sync::Arc;

use gh_api::auth::{detect_auth, AuthOutcome};
use gh_api::{list_open_prs, resolve_from_cwd, Client, EtagCache};
use gh_core::{Cmd, Msg};
use tokio::sync::{mpsc::Sender, OnceCell};
use tracing::{debug, warn};

/// Shared context every worker needs.
///
/// `client` is filled by the auth worker once a token is in hand. Subsequent
/// fetch workers read it; if not yet set, they post `PrListFailed("auth not
/// ready")` rather than panicking. The reducer also gates fetch dispatch on
/// `state.auth.is_authenticated()`, so this fallback is belt-and-braces.
///
/// `cache` is the ETag cache built at startup, shared across the lifetime of
/// the binary. Persistent (SQLite) when the cache directory is writable,
/// else falls back to in-memory.
#[derive(Clone)]
pub struct AppCtx {
    pub tx: Sender<Msg>,
    pub client: Arc<OnceCell<Client>>,
    pub cache: Arc<EtagCache>,
}

impl AppCtx {
    pub fn new(tx: Sender<Msg>, cache: Arc<EtagCache>) -> Self {
        Self {
            tx,
            client: Arc::new(OnceCell::new()),
            cache,
        }
    }
}

pub fn dispatch(cmd: Cmd, ctx: AppCtx) {
    match cmd {
        Cmd::AuthenticateFromGh => {
            tokio::spawn(async move {
                let outcome = detect_auth().await;
                let msg = match outcome {
                    AuthOutcome::Token { token, host, user } => {
                        match Client::new(&token, &host, ctx.cache.clone()) {
                            Ok(client) => {
                                // OnceCell::set returns Err if already set;
                                // a second auth attempt would land here.
                                // Today we only auth once at startup.
                                if ctx.client.set(client).is_err() {
                                    warn!("client already initialised; ignoring re-auth");
                                }
                                Msg::AuthReady { host, user }
                            }
                            Err(e) => Msg::AuthMissing {
                                reason: format!("client init failed: {e}"),
                            },
                        }
                    }
                    AuthOutcome::Missing { reason } => Msg::AuthMissing { reason },
                };
                let _ = ctx.tx.send(msg).await;
            });
        }
        Cmd::ResolveRepoFromCwd => {
            tokio::spawn(async move {
                let msg = match resolve_from_cwd().await {
                    Ok(repo) => {
                        debug!(slug = %repo.slug(), "repo resolved from cwd");
                        Msg::RepoResolved(repo)
                    }
                    Err(e) => Msg::RepoResolveFailed(e.to_string()),
                };
                let _ = ctx.tx.send(msg).await;
            });
        }
        Cmd::FetchPrList { repo } => {
            tokio::spawn(async move {
                let Some(client) = ctx.client.get() else {
                    let _ = ctx
                        .tx
                        .send(Msg::PrListFailed("auth not ready".to_string()))
                        .await;
                    return;
                };
                let msg = match list_open_prs(client, &repo).await {
                    Ok(items) => Msg::PrListReady { repo, items },
                    Err(e) => Msg::PrListFailed(e.to_string()),
                };
                let _ = ctx.tx.send(msg).await;
            });
        }
    }
}
