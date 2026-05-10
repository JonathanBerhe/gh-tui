//! Messages dispatched to the reducer.
//!
//! `Msg` expresses **domain events**, not raw key presses. Keys are resolved
//! to `gh_input::Action` first; the binary maps `Action -> Msg`.

use crate::{
    pulls::{FilePatch, PrDetail, PrSummary, RepoRef, ReviewThread},
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
    /// pulling `gh-render` into `gh-core`. `image_urls` lists every
    /// `![alt](url)` link the body references — the reducer fans out
    /// `Cmd::FetchImage` for each so the cache is warm by the time the
    /// renderer reaches the matching `Image` chunk. `mermaid_blocks`
    /// carries `(hash, source)` for every Mermaid diagram in the body —
    /// the reducer fans out `Cmd::RenderMermaid` for each.
    PrDetailReady {
        detail: PrDetail,
        body_lines: u16,
        image_urls: Vec<String>,
        mermaid_blocks: Vec<(String, String)>,
    },
    /// PR detail GraphQL request failed.
    PrDetailFailed(String),
    /// User pressed Tab from the PR detail screen.
    OpenDiff,
    /// Per-file diff fetch landed. `file_offsets` is the rendered line offset
    /// of each file's first hunk (computed by the worker via `gh_render`) so
    /// the reducer can serve `{`/`}` jumps without pulling `gh-render` into
    /// `gh-core`. `threads` carries inline review comments fetched in
    /// parallel via the GraphQL `pullRequest.reviewThreads` query.
    DiffReady {
        repo: RepoRef,
        number: u64,
        files: Vec<FilePatch>,
        threads: Vec<ReviewThread>,
        file_offsets: Vec<u16>,
        /// Total rendered line count, used by the reducer to clamp
        /// `scroll` so `G` and motion deltas behave correctly.
        total_lines: u16,
    },
    /// Per-file diff fetch failed.
    DiffFailed(String),
    /// An image fetch finished — the cache now has the decoded protocol
    /// (or a `Failed` slot). Reducer is a no-op; the message exists
    /// purely to wake the event loop so the next render picks up the
    /// new cache state.
    ImageReady {
        url: String,
    },
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
