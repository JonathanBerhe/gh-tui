//! Authentication state tracked in [`crate::State`].
//!
//! The worker that produces these transitions lives in `gh-api`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthState {
    #[default]
    Unknown,
    Authenticated {
        host: String,
        user: Option<String>,
    },
    Missing {
        reason: String,
    },
}

impl AuthState {
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Unknown => "loading auth…".to_string(),
            Self::Authenticated {
                host,
                user: Some(u),
            } => format!("@{u} on {host}"),
            Self::Authenticated { host, user: None } => format!("authenticated on {host}"),
            Self::Missing { reason } => format!("no auth — {reason}"),
        }
    }
}
