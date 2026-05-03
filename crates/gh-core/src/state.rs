//! Top-level application state.

use crate::{
    auth::AuthState,
    pulls::{FilePatch, PrDetail, PrSummary, RepoRef},
    rate_limit::RateLimit,
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
    /// Repo identified, waiting for the PR list API response.
    Loading { repo: RepoRef },
    /// PR list rendered.
    PrList {
        repo: RepoRef,
        items: Vec<PrSummary>,
        selected: usize,
        /// Highest page index already appended (1-based).
        pages_loaded: u32,
        /// Whether the last response indicated more pages exist.
        has_more: bool,
        /// True while a next-page fetch is in flight; gates the auto-scroll
        /// trigger so we don't fire concurrent requests.
        loading_next: bool,
    },
    /// Waiting for PR detail GraphQL response.
    LoadingDetail { repo: RepoRef, number: u64 },
    /// PR detail rendered. `scroll` is in logical-line units; `review_offsets`
    /// is the pre-computed line offset of each review entry, used by `{`/`}`
    /// to jump between reviews.
    PrDetail {
        repo: RepoRef,
        detail: PrDetail,
        scroll: u16,
        review_offsets: Vec<u16>,
    },
    /// Waiting for the per-file diff (REST `/pulls/{n}/files`) response.
    LoadingDiff { repo: RepoRef, number: u64 },
    /// PR diff rendered. `scroll` is in logical-line units; `file_offsets` is
    /// the pre-computed line offset of each file's first hunk, used by `{`/`}`
    /// to jump between files.
    DiffView {
        repo: RepoRef,
        number: u64,
        files: Vec<FilePatch>,
        scroll: u16,
        file_offsets: Vec<u16>,
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
    /// Stack of prior screens for back-navigation. `Msg::Back` pops one;
    /// empty stack returns to `Screen::Welcome`.
    pub nav_stack: Vec<Screen>,
    pub rate_limit: Option<RateLimit>,
    pub should_quit: bool,
}

impl AuthState {
    /// Convenience for the reducer: did auth resolve to a usable token?
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated { .. })
    }
}
