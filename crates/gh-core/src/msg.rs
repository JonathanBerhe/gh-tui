//! Messages dispatched to the reducer.
//!
//! `Msg` expresses **domain events**, not raw key presses. Keys are resolved
//! to `gh_input::Action` first; the binary maps `Action -> Msg`.

use crate::{
    pulls::{FilePatch, PrDetail, PrSummary, RepoRef},
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
    /// User pressed Tab from the PR detail screen.
    OpenDiff,
    /// Per-file diff fetch landed. `file_offsets` is the rendered line offset
    /// of each file's first hunk (computed by the worker via `gh_render`) so
    /// the reducer can serve `{`/`}` jumps without pulling `gh-render` into
    /// `gh-core`.
    DiffReady {
        repo: RepoRef,
        number: u64,
        files: Vec<FilePatch>,
        file_offsets: Vec<u16>,
    },
    /// Per-file diff fetch failed.
    DiffFailed(String),
    /// Flip the diff view between unified and split layouts.
    ToggleDiffViewMode,
    /// Move the selection in the PR list.
    SelectionDelta(i32),
    /// Jump selection to first or last item.
    SelectionJump(SelectionJump),
    /// Jump scroll position to the next/prev section within the current
    /// screen — review entry in `PrDetail`, file in `DiffView`, etc. Counted
    /// via vim-style `count{` / `count}`.
    SectionJump {
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
