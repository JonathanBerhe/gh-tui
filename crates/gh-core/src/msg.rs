//! Messages dispatched to the reducer.
//!
//! `Msg` expresses **domain events**, not raw key presses. Keys are resolved
//! to `gh_input::Action` first; the binary maps `Action -> Msg`.

use crate::{
    pulls::{PrDetail, PrSummary, RepoRef},
    rate_limit::RateLimit,
};

#[derive(Debug, Clone)]
pub enum Msg {
    Tick,
    AuthReady {
        host: String,
        user: Option<String>,
    },
    AuthMissing {
        reason: String,
    },
    /// Vim resolver's pending-buffer display has changed.
    PendingChanged(String),
    /// Repo argv parsed or `gh repo view` succeeded.
    RepoResolved(RepoRef),
    /// `gh repo view` failed (no argv + not in a repo).
    RepoResolveFailed(String),
    /// One page of the PR list arrived from the API.
    PrPageReady {
        repo: RepoRef,
        page: u32,
        items: Vec<PrSummary>,
        has_more: bool,
    },
    /// The PR list fetch failed.
    PrListFailed(String),
    /// User pressed Enter on the selected PR row.
    OpenSelectedPr,
    /// User pressed Backspace from a sub-screen.
    Back,
    /// PR detail GraphQL response landed. `body_lines` is the number of
    /// rendered markdown lines (computed by the worker via `gh_render`) so
    /// the reducer can pre-compute review-section scroll offsets without
    /// pulling `gh-render` into `gh-core`.
    PrDetailReady {
        detail: PrDetail,
        body_lines: u16,
    },
    /// PR detail GraphQL request failed.
    PrDetailFailed(String),
    /// Move the selection in the PR list.
    SelectionDelta(i32),
    /// Jump selection to first or last item.
    SelectionJump(SelectionJump),
    /// Jump scroll position to the next/prev section (review entry,
    /// diff hunk, etc.) within the current screen. Counted via vim-style
    /// `count{` / `count}`.
    ReviewJump {
        count: usize,
        direction: JumpDirection,
    },
    /// Fresh rate-limit reading from response headers.
    RateLimitUpdate(RateLimit),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionJump {
    First,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpDirection {
    Next,
    Prev,
}
