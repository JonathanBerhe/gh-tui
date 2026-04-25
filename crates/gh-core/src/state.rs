//! Top-level application state.

use crate::{
    auth::AuthState,
    pulls::{PrSummary, RepoRef},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Command,
}

impl Mode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Command => "COMMAND",
        }
    }
}

/// What the body of the screen is currently showing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Screen {
    /// Pre-bootstrap placeholder (Phase 1's body).
    #[default]
    Welcome,
    /// Repo identified, waiting for the API response.
    Loading { repo: RepoRef },
    /// PR list rendered.
    PrList {
        repo: RepoRef,
        items: Vec<PrSummary>,
        selected: usize,
    },
    /// Unrecoverable error; user-facing message + optional hint.
    Error {
        message: String,
        hint: Option<String>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub mode: Mode,
    pub pending: String,
    pub auth: AuthState,
    pub screen: Screen,
    pub should_quit: bool,
}

impl AuthState {
    /// Convenience for the reducer: did auth resolve to a usable token?
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated { .. })
    }
}
