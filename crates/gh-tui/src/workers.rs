//! Async command workers. A reducer emits `Cmd`s; `dispatch` turns each one
//! into a task that runs the side effect and posts a `Msg` back on the mpsc
//! channel. One match arm per `Cmd` variant; future commands add branches.

use gh_api::auth::{detect_auth, AuthOutcome};
use gh_core::{Cmd, Msg};
use tokio::sync::mpsc::Sender;
use tracing::warn;

pub fn dispatch(cmd: Cmd, tx: Sender<Msg>) {
    match cmd {
        Cmd::AuthenticateFromGh => {
            tokio::spawn(async move {
                let msg = match detect_auth().await {
                    AuthOutcome::Token { host, user, .. } => Msg::AuthReady { host, user },
                    AuthOutcome::Missing { reason } => Msg::AuthMissing { reason },
                };
                if let Err(e) = tx.send(msg).await {
                    warn!(error = %e, "failed to post auth msg — channel closed");
                }
            });
        }
    }
}
