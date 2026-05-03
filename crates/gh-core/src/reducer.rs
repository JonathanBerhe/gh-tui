//! Pure reducer. Given `(State, Msg)`, returns `(State, Vec<Cmd>)`.

use crate::{
    auth::AuthState,
    cmd::Cmd,
    msg::{JumpDirection, Msg, SelectionJump},
    state::{Screen, State},
};

/// Trigger an auto-fetch of the next page when the selection lands within
/// this many items of the loaded boundary.
const PAGINATION_LOAD_THRESHOLD: usize = 5;

/// Approximate rendered height of one review entry in the PR detail view
/// (header line + body excerpt line + blank). Drives `{`/`}` jump targets.
const REVIEW_BLOCK_HEIGHT: u16 = 3;
/// Padding between body and reviews block: separator + "Reviews" header
/// + blank line.
const REVIEWS_HEADER_OFFSET: u16 = 3;

/// Commands to dispatch on startup, before any user input.
///
/// The auth Cmd always fires. The repo-resolve Cmd is conditional on whether
/// the binary already has an argv-supplied repo; the binary handles that
/// branch and posts `Msg::RepoResolved` directly when it does.
#[must_use]
pub fn initial_commands() -> Vec<Cmd> {
    vec![Cmd::AuthenticateFromGh]
}

#[must_use]
pub fn reduce(mut state: State, msg: Msg) -> (State, Vec<Cmd>) {
    let mut cmds = Vec::new();
    match msg {
        Msg::Quit => {
            state.should_quit = true;
        }
        Msg::Tick => {}
        Msg::AuthReady { host, user } => {
            state.auth = AuthState::Authenticated { host, user };
            // If a repo was already resolved while waiting for auth, kick
            // off the first page fetch now.
            if let Screen::Loading { repo } = &state.screen {
                cmds.push(Cmd::FetchPrPage {
                    repo: repo.clone(),
                    page: 1,
                });
            }
        }
        Msg::AuthMissing { reason } => {
            state.auth = AuthState::Missing {
                reason: reason.clone(),
            };
            // Auth failure is fatal for any screen that needs the API.
            if matches!(state.screen, Screen::Loading { .. }) {
                state.screen = Screen::Error {
                    message: format!("auth failed: {reason}"),
                    hint: Some("run `gh auth login`".to_string()),
                };
            }
        }
        Msg::PendingChanged(buf) => {
            state.pending = buf;
        }
        Msg::RepoResolved(repo) => {
            state.screen = Screen::Loading { repo: repo.clone() };
            // Only fetch once auth is in hand. Otherwise wait for AuthReady.
            if state.auth.is_authenticated() {
                cmds.push(Cmd::FetchPrPage { repo, page: 1 });
            }
        }
        Msg::RepoResolveFailed(reason) => {
            state.screen = Screen::Error {
                message: format!("could not determine repo: {reason}"),
                hint: Some("pass `owner/name` or run from a clone".to_string()),
            };
        }
        Msg::PrPageReady {
            repo,
            page,
            items,
            has_more,
        } => {
            match &mut state.screen {
                // First page replaces the Loading placeholder.
                Screen::Loading { repo: cur } if *cur == repo && page == 1 => {
                    state.screen = Screen::PrList {
                        repo,
                        items,
                        selected: 0,
                        pages_loaded: 1,
                        has_more,
                        loading_next: false,
                    };
                }
                // Subsequent pages append, only if they're for the active
                // repo and the next sequential page index.
                Screen::PrList {
                    repo: cur,
                    items: cur_items,
                    pages_loaded,
                    has_more: cur_has_more,
                    loading_next,
                    ..
                } if *cur == repo && page == *pages_loaded + 1 => {
                    cur_items.extend(items);
                    *pages_loaded = page;
                    *cur_has_more = has_more;
                    *loading_next = false;
                }
                // Stale page (screen changed, repo changed, or out-of-order
                // delivery) — drop it. The current screen state wins.
                _ => {}
            }
        }
        Msg::PrListFailed(reason) => {
            state.screen = Screen::Error {
                message: format!("PR list failed: {reason}"),
                hint: None,
            };
        }
        Msg::OpenSelectedPr => {
            // Only meaningful from the PR list. Snapshot the current screen
            // onto the nav stack and transition to LoadingDetail.
            if let Screen::PrList {
                repo,
                items,
                selected,
                ..
            } = &state.screen
            {
                if let Some(pr) = items.get(*selected) {
                    let number = pr.number;
                    let repo_ref = repo.clone();
                    let prior = std::mem::replace(
                        &mut state.screen,
                        Screen::LoadingDetail {
                            repo: repo_ref.clone(),
                            number,
                        },
                    );
                    state.nav_stack.push(prior);
                    cmds.push(Cmd::FetchPrDetail {
                        repo: repo_ref,
                        number,
                    });
                }
            }
        }
        Msg::PrDetailReady { detail, body_lines } => {
            // Only consume if we're still waiting for THIS detail. Stale
            // responses (number mismatch or screen changed) drop silently.
            if let Screen::LoadingDetail { repo, number } = &state.screen {
                if detail.number == *number {
                    let review_offsets = compute_review_offsets(body_lines, detail.reviews.len());
                    state.screen = Screen::PrDetail {
                        repo: repo.clone(),
                        detail,
                        scroll: 0,
                        review_offsets,
                    };
                }
            }
        }
        Msg::PrDetailFailed(reason) => {
            // Only transition if we were actually loading a detail; otherwise
            // a stale failure shouldn't blow away an unrelated screen.
            if matches!(state.screen, Screen::LoadingDetail { .. }) {
                state.screen = Screen::Error {
                    message: format!("PR detail failed: {reason}"),
                    hint: None,
                };
            }
        }
        Msg::OpenDiff => {
            // Only meaningful from PR detail. Snapshot the current screen
            // onto the nav stack and transition to LoadingDiff.
            if let Screen::PrDetail { repo, detail, .. } = &state.screen {
                let number = detail.number;
                let repo_ref = repo.clone();
                let prior = std::mem::replace(
                    &mut state.screen,
                    Screen::LoadingDiff {
                        repo: repo_ref.clone(),
                        number,
                    },
                );
                state.nav_stack.push(prior);
                cmds.push(Cmd::FetchPrDiff {
                    repo: repo_ref,
                    number,
                });
            }
        }
        Msg::DiffReady {
            repo,
            number,
            files,
            file_offsets,
        } => {
            // Only consume when we're still waiting for THIS diff. Stale
            // responses (number mismatch or screen changed) drop silently.
            if let Screen::LoadingDiff {
                repo: cur_repo,
                number: cur_n,
            } = &state.screen
            {
                if *cur_repo == repo && *cur_n == number {
                    state.screen = Screen::DiffView {
                        repo,
                        number,
                        files,
                        scroll: 0,
                        file_offsets,
                    };
                }
            }
        }
        Msg::DiffFailed(reason) => {
            if matches!(state.screen, Screen::LoadingDiff { .. }) {
                state.screen = Screen::Error {
                    message: format!("PR diff failed: {reason}"),
                    hint: None,
                };
            }
        }
        Msg::Back => {
            state.screen = state.nav_stack.pop().unwrap_or(Screen::Welcome);
        }
        Msg::SelectionDelta(delta) => match &mut state.screen {
            Screen::PrList {
                repo,
                items,
                selected,
                pages_loaded,
                has_more,
                loading_next,
            } => {
                let len = items.len();
                if len > 0 {
                    let new = (*selected as i64) + i64::from(delta);
                    let clamped = new.clamp(0, (len - 1) as i64);
                    *selected = clamped as usize;

                    // Auto-fetch next page when selection nears the boundary.
                    let remaining = len.saturating_sub(*selected);
                    if remaining <= PAGINATION_LOAD_THRESHOLD && *has_more && !*loading_next {
                        *loading_next = true;
                        cmds.push(Cmd::FetchPrPage {
                            repo: repo.clone(),
                            page: *pages_loaded + 1,
                        });
                    }
                }
            }
            Screen::PrDetail { scroll, .. } | Screen::DiffView { scroll, .. } => {
                let new = i32::from(*scroll).saturating_add(delta);
                *scroll = u16::try_from(new.max(0)).unwrap_or(u16::MAX);
            }
            Screen::Welcome
            | Screen::Loading { .. }
            | Screen::LoadingDetail { .. }
            | Screen::LoadingDiff { .. }
            | Screen::Error { .. } => {}
        },
        Msg::SelectionJump(jump) => match &mut state.screen {
            Screen::PrList {
                items, selected, ..
            } if !items.is_empty() => {
                *selected = match jump {
                    SelectionJump::First => 0,
                    SelectionJump::Last => items.len() - 1,
                };
            }
            Screen::PrDetail { scroll, .. } | Screen::DiffView { scroll, .. } => {
                *scroll = match jump {
                    SelectionJump::First => 0,
                    SelectionJump::Last => u16::MAX,
                };
            }
            _ => {}
        },
        Msg::SectionJump { count, direction } => match &mut state.screen {
            Screen::PrDetail {
                scroll,
                review_offsets,
                ..
            } if !review_offsets.is_empty() => {
                *scroll = next_section_scroll(*scroll, review_offsets, count, direction);
            }
            Screen::DiffView {
                scroll,
                file_offsets,
                ..
            } if !file_offsets.is_empty() => {
                *scroll = next_section_scroll(*scroll, file_offsets, count, direction);
            }
            _ => {}
        },
        Msg::RateLimitUpdate(rl) => {
            state.rate_limit = Some(rl);
        }
    }
    (state, cmds)
}

/// Compute the line offset of each review entry in the PR detail body view.
/// Result is empty when there are no reviews.
fn compute_review_offsets(body_lines: u16, n_reviews: usize) -> Vec<u16> {
    if n_reviews == 0 {
        return Vec::new();
    }
    let base = body_lines.saturating_add(REVIEWS_HEADER_OFFSET);
    (0..n_reviews)
        .map(|i| {
            let step = REVIEW_BLOCK_HEIGHT.saturating_mul(u16::try_from(i).unwrap_or(u16::MAX));
            base.saturating_add(step)
        })
        .collect()
}

/// Pick the next/prev section's scroll position relative to the current one.
/// Used by both `PrDetail` (review entries) and `DiffView` (file headers).
fn next_section_scroll(
    current: u16,
    offsets: &[u16],
    count: usize,
    direction: JumpDirection,
) -> u16 {
    debug_assert!(!offsets.is_empty());
    let count = count.max(1);
    match direction {
        JumpDirection::Next => {
            // Find first offset strictly greater than current; advance count-1 more.
            let start = offsets
                .iter()
                .position(|&o| o > current)
                .unwrap_or(offsets.len() - 1);
            let target = (start + count - 1).min(offsets.len() - 1);
            offsets[target]
        }
        JumpDirection::Prev => {
            // Find last offset strictly less than current; retreat count-1 more.
            let start = offsets.iter().rposition(|&o| o < current).unwrap_or(0);
            let target = start.saturating_sub(count - 1);
            offsets[target]
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::pulls::{PrSummary, RepoRef};
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    fn repo() -> RepoRef {
        RepoRef::parse("a/b").unwrap()
    }

    fn pr(n: u64) -> PrSummary {
        PrSummary {
            number: n,
            title: format!("pr {n}"),
            author: "alice".into(),
            draft: false,
            head_ref: "feat".into(),
            base_ref: "main".into(),
            comments: 0,
            created_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            additions: 0,
            deletions: 0,
        }
    }

    fn authenticated() -> State {
        State {
            auth: AuthState::Authenticated {
                host: "github.com".into(),
                user: Some("alice".into()),
            },
            ..State::default()
        }
    }

    // ── existing Phase 1/2 tests, kept ─────────────────────────────────

    #[test]
    fn quit_msg_sets_should_quit() {
        let (state, cmds) = reduce(State::default(), Msg::Quit);
        assert!(state.should_quit);
        assert!(cmds.is_empty());
    }

    #[test]
    fn auth_ready_transitions_auth_state() {
        let (state, _) = reduce(
            State::default(),
            Msg::AuthReady {
                host: "github.com".into(),
                user: Some("alice".into()),
            },
        );
        assert_eq!(
            state.auth,
            AuthState::Authenticated {
                host: "github.com".into(),
                user: Some("alice".into()),
            }
        );
    }

    #[test]
    fn auth_missing_transitions_auth_state() {
        let (state, _) = reduce(
            State::default(),
            Msg::AuthMissing {
                reason: "gh not installed".into(),
            },
        );
        assert!(matches!(state.auth, AuthState::Missing { .. }));
    }

    #[test]
    fn tick_is_no_op() {
        let (state, cmds) = reduce(State::default(), Msg::Tick);
        assert!(!state.should_quit);
        assert!(cmds.is_empty());
    }

    #[test]
    fn initial_commands_includes_auth() {
        let cmds = initial_commands();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::AuthenticateFromGh));
    }

    #[test]
    fn pending_changed_sets_state_pending() {
        let (state, _) = reduce(State::default(), Msg::PendingChanged("2d3".into()));
        assert_eq!(state.pending, "2d3");
    }

    // ── Phase 3 ────────────────────────────────────────────────────────

    fn pr_list_state(items: Vec<PrSummary>, selected: usize) -> State {
        State {
            screen: Screen::PrList {
                repo: repo(),
                items,
                selected,
                pages_loaded: 1,
                has_more: false,
                loading_next: false,
            },
            ..State::default()
        }
    }

    fn pr_list_state_with_more(items: Vec<PrSummary>, selected: usize) -> State {
        State {
            screen: Screen::PrList {
                repo: repo(),
                items,
                selected,
                pages_loaded: 1,
                has_more: true,
                loading_next: false,
            },
            ..State::default()
        }
    }

    fn loading_first_page(repo: crate::pulls::RepoRef) -> State {
        // Helper for tests: the binary transitions to Loading before the
        // first fetch returns; we simulate that here.
        State {
            screen: Screen::Loading { repo },
            auth: AuthState::Authenticated {
                host: "github.com".into(),
                user: Some("alice".into()),
            },
            ..State::default()
        }
    }

    #[test]
    fn repo_resolved_with_auth_emits_first_page_fetch() {
        let (state, cmds) = reduce(authenticated(), Msg::RepoResolved(repo()));
        assert!(matches!(state.screen, Screen::Loading { .. }));
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::FetchPrPage { page: 1, .. }));
    }

    #[test]
    fn repo_resolved_without_auth_waits() {
        let (state, cmds) = reduce(State::default(), Msg::RepoResolved(repo()));
        assert!(matches!(state.screen, Screen::Loading { .. }));
        assert!(cmds.is_empty(), "must not fetch before auth resolves");
    }

    #[test]
    fn auth_ready_after_repo_kicks_off_first_page_fetch() {
        // Repo arrives first, auth still Unknown → no Cmd.
        let (s1, _) = reduce(State::default(), Msg::RepoResolved(repo()));
        // Auth then resolves → reducer emits the deferred FetchPrPage.
        let (_, cmds) = reduce(
            s1,
            Msg::AuthReady {
                host: "github.com".into(),
                user: Some("alice".into()),
            },
        );
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::FetchPrPage { page: 1, .. }));
    }

    #[test]
    fn auth_missing_during_loading_transitions_to_error() {
        let (s1, _) = reduce(State::default(), Msg::RepoResolved(repo()));
        let (s2, _) = reduce(
            s1,
            Msg::AuthMissing {
                reason: "no gh".into(),
            },
        );
        assert!(matches!(s2.screen, Screen::Error { .. }));
    }

    #[test]
    fn first_page_ready_transitions_to_pr_list() {
        let (state, _) = reduce(
            loading_first_page(repo()),
            Msg::PrPageReady {
                repo: repo(),
                page: 1,
                items: vec![pr(1), pr(2), pr(3)],
                has_more: true,
            },
        );
        let Screen::PrList {
            items,
            selected,
            pages_loaded,
            has_more,
            loading_next,
            ..
        } = state.screen
        else {
            panic!("expected PrList");
        };
        assert_eq!(items.len(), 3);
        assert_eq!(selected, 0);
        assert_eq!(pages_loaded, 1);
        assert!(has_more);
        assert!(!loading_next);
    }

    #[test]
    fn second_page_ready_appends_items() {
        let s1 = pr_list_state_with_more(vec![pr(1), pr(2)], 0);
        // Mark loading_next so the test mirrors how the reducer would have
        // gated the in-flight request before the page arrives.
        let s1 = State {
            screen: match s1.screen {
                Screen::PrList {
                    repo,
                    items,
                    selected,
                    pages_loaded,
                    has_more,
                    ..
                } => Screen::PrList {
                    repo,
                    items,
                    selected,
                    pages_loaded,
                    has_more,
                    loading_next: true,
                },
                _ => unreachable!(),
            },
            ..s1
        };
        let (s2, _) = reduce(
            s1,
            Msg::PrPageReady {
                repo: repo(),
                page: 2,
                items: vec![pr(3), pr(4), pr(5)],
                has_more: false,
            },
        );
        let Screen::PrList {
            items,
            pages_loaded,
            has_more,
            loading_next,
            ..
        } = s2.screen
        else {
            panic!("expected PrList");
        };
        assert_eq!(items.len(), 5);
        assert_eq!(pages_loaded, 2);
        assert!(!has_more);
        assert!(!loading_next);
    }

    #[test]
    fn out_of_order_page_is_dropped() {
        let s = pr_list_state_with_more(vec![pr(1), pr(2)], 0);
        let (s2, _) = reduce(
            s,
            Msg::PrPageReady {
                repo: repo(),
                page: 5, // skipped pages 2,3,4 — stale
                items: vec![pr(99)],
                has_more: false,
            },
        );
        let Screen::PrList {
            items,
            pages_loaded,
            ..
        } = s2.screen
        else {
            panic!("expected PrList");
        };
        assert_eq!(items.len(), 2, "stale page must not append");
        assert_eq!(pages_loaded, 1);
    }

    #[test]
    fn selection_near_boundary_triggers_next_page_fetch() {
        let items = (1..=30).map(pr).collect::<Vec<_>>();
        let s = pr_list_state_with_more(items, 24); // 30 items, selected 24
        let (s2, cmds) = reduce(s, Msg::SelectionDelta(1)); // → 25, remaining 5
        let Screen::PrList { loading_next, .. } = s2.screen else {
            panic!()
        };
        assert!(loading_next, "should set loading_next");
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::FetchPrPage { page: 2, .. }));
    }

    #[test]
    fn selection_far_from_boundary_does_not_fetch() {
        let items = (1..=30).map(pr).collect::<Vec<_>>();
        let s = pr_list_state_with_more(items, 0);
        let (s2, cmds) = reduce(s, Msg::SelectionDelta(1));
        let Screen::PrList { loading_next, .. } = s2.screen else {
            panic!()
        };
        assert!(!loading_next);
        assert!(cmds.is_empty());
    }

    #[test]
    fn loading_next_blocks_concurrent_fetches() {
        let items = (1..=30).map(pr).collect::<Vec<_>>();
        let s = State {
            screen: Screen::PrList {
                repo: repo(),
                items,
                selected: 25,
                pages_loaded: 1,
                has_more: true,
                loading_next: true, // already in flight
            },
            ..State::default()
        };
        let (_, cmds) = reduce(s, Msg::SelectionDelta(1));
        assert!(cmds.is_empty(), "must not fetch while loading_next");
    }

    #[test]
    fn last_page_clears_has_more() {
        // After the last page arrives, scrolling near boundary doesn't fetch.
        let items = (1..=30).map(pr).collect::<Vec<_>>();
        let s = pr_list_state(items, 25);
        let (_, cmds) = reduce(s, Msg::SelectionDelta(1));
        assert!(cmds.is_empty());
    }

    #[test]
    fn pr_list_failed_transitions_to_error() {
        let (state, _) = reduce(authenticated(), Msg::PrListFailed("404".into()));
        assert!(matches!(state.screen, Screen::Error { .. }));
    }

    #[test]
    fn repo_resolve_failed_transitions_to_error() {
        let (state, _) = reduce(
            State::default(),
            Msg::RepoResolveFailed("not in a repo".into()),
        );
        assert!(matches!(state.screen, Screen::Error { .. }));
    }

    #[test]
    fn selection_down_increments() {
        let s = pr_list_state(vec![pr(1), pr(2), pr(3)], 0);
        let (state, _) = reduce(s, Msg::SelectionDelta(1));
        let Screen::PrList { selected, .. } = state.screen else {
            panic!("expected PrList");
        };
        assert_eq!(selected, 1);
    }

    #[test]
    fn selection_up_at_zero_stays_zero() {
        let s = pr_list_state(vec![pr(1), pr(2)], 0);
        let (state, _) = reduce(s, Msg::SelectionDelta(-1));
        let Screen::PrList { selected, .. } = state.screen else {
            panic!("expected PrList");
        };
        assert_eq!(selected, 0);
    }

    #[test]
    fn selection_down_at_end_stays() {
        let s = pr_list_state(vec![pr(1), pr(2)], 1);
        let (state, _) = reduce(s, Msg::SelectionDelta(5));
        let Screen::PrList { selected, .. } = state.screen else {
            panic!("expected PrList");
        };
        assert_eq!(selected, 1);
    }

    #[test]
    fn selection_jump_first_last() {
        let mk = |sel| pr_list_state(vec![pr(1), pr(2), pr(3), pr(4)], sel);
        let (s, _) = reduce(mk(2), Msg::SelectionJump(SelectionJump::First));
        let Screen::PrList { selected, .. } = s.screen else {
            panic!()
        };
        assert_eq!(selected, 0);

        let (s, _) = reduce(mk(0), Msg::SelectionJump(SelectionJump::Last));
        let Screen::PrList { selected, .. } = s.screen else {
            panic!()
        };
        assert_eq!(selected, 3);
    }

    #[test]
    fn selection_on_empty_list_is_noop() {
        let s = pr_list_state(vec![], 0);
        let (state, _) = reduce(s, Msg::SelectionDelta(1));
        let Screen::PrList { selected, .. } = state.screen else {
            panic!("expected PrList");
        };
        assert_eq!(selected, 0);
    }

    #[test]
    fn selection_msg_outside_pr_list_is_noop() {
        // In Welcome screen, selection deltas don't crash and don't mutate.
        let (state, _) = reduce(State::default(), Msg::SelectionDelta(3));
        assert!(matches!(state.screen, Screen::Welcome));
    }

    #[test]
    fn rate_limit_update_sets_state() {
        let rl = crate::rate_limit::RateLimit {
            remaining: 4998,
            limit: 5000,
            reset_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        };
        let (state, cmds) = reduce(State::default(), Msg::RateLimitUpdate(rl));
        assert_eq!(state.rate_limit, Some(rl));
        assert!(cmds.is_empty());
    }

    // ── Phase 4: PR detail ─────────────────────────────────────────────

    fn pr_detail(number: u64) -> crate::pulls::PrDetail {
        crate::pulls::PrDetail {
            number,
            title: format!("pr {number}"),
            body: String::new(),
            state: crate::pulls::PrState::Open,
            draft: false,
            mergeable: crate::pulls::Mergeable::Yes,
            author: "alice".into(),
            head_ref: "feat".into(),
            base_ref: "main".into(),
            additions: 10,
            deletions: 2,
            review_decision: crate::pulls::ReviewDecision::None,
            reviews: Vec::new(),
            checks: crate::pulls::ChecksSummary {
                state: crate::pulls::ChecksState::Unknown,
                passing: 0,
                failing: 0,
                pending: 0,
            },
        }
    }

    #[test]
    fn open_selected_pr_pushes_to_nav_stack_and_emits_fetch() {
        let s = pr_list_state(vec![pr(1), pr(2), pr(3)], 1);
        let (s2, cmds) = reduce(s, Msg::OpenSelectedPr);
        assert!(matches!(s2.screen, Screen::LoadingDetail { number: 2, .. }));
        assert_eq!(s2.nav_stack.len(), 1, "prior PrList should be on stack");
        assert!(matches!(s2.nav_stack[0], Screen::PrList { .. }));
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::FetchPrDetail { number: 2, .. }));
    }

    #[test]
    fn open_selected_pr_outside_pr_list_is_noop() {
        let (s, cmds) = reduce(State::default(), Msg::OpenSelectedPr);
        assert!(matches!(s.screen, Screen::Welcome));
        assert!(cmds.is_empty());
        assert!(s.nav_stack.is_empty());
    }

    #[test]
    fn open_selected_pr_with_empty_list_is_noop() {
        let s = pr_list_state(vec![], 0);
        let (s2, cmds) = reduce(s, Msg::OpenSelectedPr);
        assert!(matches!(s2.screen, Screen::PrList { .. }));
        assert!(cmds.is_empty());
    }

    #[test]
    fn pr_detail_ready_transitions_to_pr_detail() {
        let s = State {
            screen: Screen::LoadingDetail {
                repo: repo(),
                number: 7,
            },
            nav_stack: vec![Screen::PrList {
                repo: repo(),
                items: vec![pr(7)],
                selected: 0,
                pages_loaded: 1,
                has_more: false,
                loading_next: false,
            }],
            ..State::default()
        };
        let (s2, _) = reduce(
            s,
            Msg::PrDetailReady {
                detail: pr_detail(7),
                body_lines: 0,
            },
        );
        let Screen::PrDetail { detail, scroll, .. } = s2.screen else {
            panic!("expected PrDetail");
        };
        assert_eq!(detail.number, 7);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn pr_detail_ready_for_stale_number_is_dropped() {
        // We're loading #7 but a #99 detail arrives — drop it.
        let s = State {
            screen: Screen::LoadingDetail {
                repo: repo(),
                number: 7,
            },
            ..State::default()
        };
        let (s2, _) = reduce(
            s,
            Msg::PrDetailReady {
                detail: pr_detail(99),
                body_lines: 0,
            },
        );
        assert!(matches!(s2.screen, Screen::LoadingDetail { number: 7, .. }));
    }

    #[test]
    fn pr_detail_failed_transitions_to_error_keeping_nav_stack() {
        let prior = Screen::PrList {
            repo: repo(),
            items: vec![pr(7)],
            selected: 0,
            pages_loaded: 1,
            has_more: false,
            loading_next: false,
        };
        let s = State {
            screen: Screen::LoadingDetail {
                repo: repo(),
                number: 7,
            },
            nav_stack: vec![prior],
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::PrDetailFailed("rate limited".into()));
        assert!(matches!(s2.screen, Screen::Error { .. }));
        assert_eq!(s2.nav_stack.len(), 1, "stack survives so Back recovers");
    }

    #[test]
    fn pr_detail_failed_outside_loading_is_noop() {
        let (s, _) = reduce(State::default(), Msg::PrDetailFailed("boom".into()));
        assert!(matches!(s.screen, Screen::Welcome));
    }

    #[test]
    fn back_from_pr_detail_pops_to_pr_list() {
        let prior = Screen::PrList {
            repo: repo(),
            items: vec![pr(7), pr(8)],
            selected: 1,
            pages_loaded: 1,
            has_more: false,
            loading_next: false,
        };
        let s = State {
            screen: Screen::PrDetail {
                repo: repo(),
                detail: pr_detail(7),
                scroll: 5,
                review_offsets: Vec::new(),
            },
            nav_stack: vec![prior],
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::Back);
        let Screen::PrList { selected, .. } = s2.screen else {
            panic!("expected PrList");
        };
        assert_eq!(selected, 1, "selection preserved across detail round-trip");
        assert!(s2.nav_stack.is_empty());
    }

    #[test]
    fn back_from_loading_detail_pops_to_pr_list() {
        let prior = Screen::PrList {
            repo: repo(),
            items: vec![pr(7)],
            selected: 0,
            pages_loaded: 1,
            has_more: false,
            loading_next: false,
        };
        let s = State {
            screen: Screen::LoadingDetail {
                repo: repo(),
                number: 7,
            },
            nav_stack: vec![prior],
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::Back);
        assert!(matches!(s2.screen, Screen::PrList { .. }));
    }

    #[test]
    fn back_from_error_pops_to_pr_list() {
        let prior = Screen::PrList {
            repo: repo(),
            items: vec![pr(7)],
            selected: 0,
            pages_loaded: 1,
            has_more: false,
            loading_next: false,
        };
        let s = State {
            screen: Screen::Error {
                message: "boom".into(),
                hint: None,
            },
            nav_stack: vec![prior],
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::Back);
        assert!(matches!(s2.screen, Screen::PrList { .. }));
    }

    #[test]
    fn body_scroll_in_pr_detail_increments_scroll() {
        let s = State {
            screen: Screen::PrDetail {
                repo: repo(),
                detail: pr_detail(7),
                scroll: 5,
                review_offsets: Vec::new(),
            },
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::SelectionDelta(3));
        let Screen::PrDetail { scroll, .. } = s2.screen else {
            panic!("expected PrDetail")
        };
        assert_eq!(scroll, 8);
    }

    #[test]
    fn body_scroll_at_zero_does_not_underflow() {
        let s = State {
            screen: Screen::PrDetail {
                repo: repo(),
                detail: pr_detail(7),
                scroll: 0,
                review_offsets: Vec::new(),
            },
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::SelectionDelta(-5));
        let Screen::PrDetail { scroll, .. } = s2.screen else {
            panic!("expected PrDetail")
        };
        assert_eq!(scroll, 0);
    }

    #[test]
    fn body_scroll_jump_first_resets_scroll() {
        let s = State {
            screen: Screen::PrDetail {
                repo: repo(),
                detail: pr_detail(7),
                scroll: 42,
                review_offsets: Vec::new(),
            },
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::SelectionJump(SelectionJump::First));
        let Screen::PrDetail { scroll, .. } = s2.screen else {
            panic!("expected PrDetail")
        };
        assert_eq!(scroll, 0);
    }

    fn detail_with_reviews(n_reviews: usize) -> State {
        State {
            screen: Screen::PrDetail {
                repo: repo(),
                detail: pr_detail(7),
                scroll: 0,
                review_offsets: compute_review_offsets(10, n_reviews),
            },
            ..State::default()
        }
    }

    #[test]
    fn review_jump_next_advances_to_first_offset() {
        let s = detail_with_reviews(3);
        let (s2, _) = reduce(
            s,
            Msg::SectionJump {
                count: 1,
                direction: JumpDirection::Next,
            },
        );
        let Screen::PrDetail {
            scroll,
            review_offsets,
            ..
        } = s2.screen
        else {
            panic!()
        };
        assert_eq!(scroll, review_offsets[0]);
    }

    #[test]
    fn review_jump_next_with_count_skips() {
        let s = detail_with_reviews(5);
        let (s2, _) = reduce(
            s,
            Msg::SectionJump {
                count: 3,
                direction: JumpDirection::Next,
            },
        );
        let Screen::PrDetail {
            scroll,
            review_offsets,
            ..
        } = s2.screen
        else {
            panic!()
        };
        assert_eq!(scroll, review_offsets[2]);
    }

    #[test]
    fn review_jump_at_last_review_stays() {
        let mut s = detail_with_reviews(3);
        if let Screen::PrDetail {
            scroll,
            review_offsets,
            ..
        } = &mut s.screen
        {
            *scroll = review_offsets[2];
        }
        let (s2, _) = reduce(
            s,
            Msg::SectionJump {
                count: 1,
                direction: JumpDirection::Next,
            },
        );
        let Screen::PrDetail {
            scroll,
            review_offsets,
            ..
        } = s2.screen
        else {
            panic!()
        };
        assert_eq!(scroll, review_offsets[2], "last review is a fixed point");
    }

    #[test]
    fn review_jump_prev_returns_to_earlier_offset() {
        let mut s = detail_with_reviews(3);
        if let Screen::PrDetail {
            scroll,
            review_offsets,
            ..
        } = &mut s.screen
        {
            *scroll = review_offsets[2];
        }
        let (s2, _) = reduce(
            s,
            Msg::SectionJump {
                count: 1,
                direction: JumpDirection::Prev,
            },
        );
        let Screen::PrDetail {
            scroll,
            review_offsets,
            ..
        } = s2.screen
        else {
            panic!()
        };
        assert_eq!(scroll, review_offsets[1]);
    }

    #[test]
    fn review_jump_with_no_reviews_is_noop() {
        let s = detail_with_reviews(0);
        let (s2, _) = reduce(
            s,
            Msg::SectionJump {
                count: 1,
                direction: JumpDirection::Next,
            },
        );
        let Screen::PrDetail { scroll, .. } = s2.screen else {
            panic!()
        };
        assert_eq!(scroll, 0, "no offsets → no movement");
    }

    #[test]
    fn body_scroll_jump_last_sets_scroll_to_u16_max() {
        let s = State {
            screen: Screen::PrDetail {
                repo: repo(),
                detail: pr_detail(7),
                scroll: 0,
                review_offsets: Vec::new(),
            },
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::SelectionJump(SelectionJump::Last));
        let Screen::PrDetail { scroll, .. } = s2.screen else {
            panic!("expected PrDetail")
        };
        assert_eq!(scroll, u16::MAX);
    }

    #[test]
    fn back_with_empty_nav_stack_returns_to_welcome() {
        let s = State {
            screen: Screen::PrDetail {
                repo: repo(),
                detail: pr_detail(7),
                scroll: 0,
                review_offsets: Vec::new(),
            },
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::Back);
        assert!(matches!(s2.screen, Screen::Welcome));
    }

    #[test]
    fn rate_limit_update_overwrites_previous() {
        let rl1 = crate::rate_limit::RateLimit {
            remaining: 5000,
            limit: 5000,
            reset_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        };
        let rl2 = crate::rate_limit::RateLimit {
            remaining: 4500,
            limit: 5000,
            reset_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0).unwrap(),
        };
        let (s, _) = reduce(State::default(), Msg::RateLimitUpdate(rl1));
        let (s, _) = reduce(s, Msg::RateLimitUpdate(rl2));
        assert_eq!(s.rate_limit, Some(rl2));
    }

    // ── Phase 5: PR diff view ──────────────────────────────────────────

    fn pr_detail_state(number: u64) -> State {
        State {
            screen: Screen::PrDetail {
                repo: repo(),
                detail: pr_detail(number),
                scroll: 0,
                review_offsets: Vec::new(),
            },
            ..State::default()
        }
    }

    fn file_patch(path: &str) -> crate::pulls::FilePatch {
        crate::pulls::FilePatch {
            path: path.into(),
            previous_path: None,
            status: crate::pulls::PatchStatus::Modified,
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1 +1 @@\n-old\n+new".into()),
            blob_sha: "deadbeef".into(),
        }
    }

    #[test]
    fn open_diff_in_pr_detail_pushes_nav_and_emits_fetch() {
        let (s2, cmds) = reduce(pr_detail_state(7), Msg::OpenDiff);
        assert!(matches!(s2.screen, Screen::LoadingDiff { number: 7, .. }));
        assert_eq!(s2.nav_stack.len(), 1, "prior PrDetail should be on stack");
        assert!(matches!(s2.nav_stack[0], Screen::PrDetail { .. }));
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::FetchPrDiff { number: 7, .. }));
    }

    #[test]
    fn open_diff_outside_pr_detail_is_noop() {
        let (s, cmds) = reduce(State::default(), Msg::OpenDiff);
        assert!(matches!(s.screen, Screen::Welcome));
        assert!(cmds.is_empty());
        assert!(s.nav_stack.is_empty());
    }

    #[test]
    fn diff_ready_transitions_to_diff_view() {
        let s = State {
            screen: Screen::LoadingDiff {
                repo: repo(),
                number: 7,
            },
            ..State::default()
        };
        let (s2, _) = reduce(
            s,
            Msg::DiffReady {
                repo: repo(),
                number: 7,
                files: vec![file_patch("a.rs"), file_patch("b.rs")],
                file_offsets: vec![0, 12],
            },
        );
        let Screen::DiffView {
            files,
            scroll,
            file_offsets,
            ..
        } = s2.screen
        else {
            panic!("expected DiffView");
        };
        assert_eq!(files.len(), 2);
        assert_eq!(scroll, 0);
        assert_eq!(file_offsets, vec![0, 12]);
    }

    #[test]
    fn diff_ready_for_stale_number_is_dropped() {
        let s = State {
            screen: Screen::LoadingDiff {
                repo: repo(),
                number: 7,
            },
            ..State::default()
        };
        let (s2, _) = reduce(
            s,
            Msg::DiffReady {
                repo: repo(),
                number: 99,
                files: vec![file_patch("x.rs")],
                file_offsets: vec![0],
            },
        );
        assert!(matches!(s2.screen, Screen::LoadingDiff { number: 7, .. }));
    }

    #[test]
    fn diff_failed_transitions_to_error_keeping_nav_stack() {
        let prior = Screen::PrDetail {
            repo: repo(),
            detail: pr_detail(7),
            scroll: 0,
            review_offsets: Vec::new(),
        };
        let s = State {
            screen: Screen::LoadingDiff {
                repo: repo(),
                number: 7,
            },
            nav_stack: vec![prior],
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::DiffFailed("rate limited".into()));
        assert!(matches!(s2.screen, Screen::Error { .. }));
        assert_eq!(s2.nav_stack.len(), 1, "stack survives so Back recovers");
    }

    #[test]
    fn back_from_diff_view_pops_to_pr_detail() {
        let prior = Screen::PrDetail {
            repo: repo(),
            detail: pr_detail(7),
            scroll: 0,
            review_offsets: Vec::new(),
        };
        let s = State {
            screen: Screen::DiffView {
                repo: repo(),
                number: 7,
                files: vec![file_patch("a.rs")],
                scroll: 5,
                file_offsets: vec![0],
            },
            nav_stack: vec![prior],
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::Back);
        assert!(matches!(s2.screen, Screen::PrDetail { .. }));
        assert!(s2.nav_stack.is_empty());
    }

    #[test]
    fn selection_delta_in_diff_view_scrolls() {
        let s = State {
            screen: Screen::DiffView {
                repo: repo(),
                number: 7,
                files: vec![file_patch("a.rs")],
                scroll: 5,
                file_offsets: vec![0],
            },
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::SelectionDelta(3));
        let Screen::DiffView { scroll, .. } = s2.screen else {
            panic!("expected DiffView");
        };
        assert_eq!(scroll, 8);
    }

    #[test]
    fn selection_delta_in_diff_view_at_zero_does_not_underflow() {
        let s = State {
            screen: Screen::DiffView {
                repo: repo(),
                number: 7,
                files: vec![file_patch("a.rs")],
                scroll: 0,
                file_offsets: vec![0],
            },
            ..State::default()
        };
        let (s2, _) = reduce(s, Msg::SelectionDelta(-5));
        let Screen::DiffView { scroll, .. } = s2.screen else {
            panic!("expected DiffView");
        };
        assert_eq!(scroll, 0);
    }

    #[test]
    fn section_jump_next_in_diff_view_advances_to_next_file() {
        let s = State {
            screen: Screen::DiffView {
                repo: repo(),
                number: 7,
                files: vec![file_patch("a.rs"), file_patch("b.rs"), file_patch("c.rs")],
                scroll: 0,
                file_offsets: vec![0, 10, 20],
            },
            ..State::default()
        };
        let (s2, _) = reduce(
            s,
            Msg::SectionJump {
                count: 1,
                direction: JumpDirection::Next,
            },
        );
        let Screen::DiffView { scroll, .. } = s2.screen else {
            panic!("expected DiffView");
        };
        assert_eq!(scroll, 10);
    }

    #[test]
    fn section_jump_with_no_files_is_noop() {
        let s = State {
            screen: Screen::DiffView {
                repo: repo(),
                number: 7,
                files: vec![],
                scroll: 0,
                file_offsets: vec![],
            },
            ..State::default()
        };
        let (s2, _) = reduce(
            s,
            Msg::SectionJump {
                count: 1,
                direction: JumpDirection::Next,
            },
        );
        let Screen::DiffView { scroll, .. } = s2.screen else {
            panic!("expected DiffView");
        };
        assert_eq!(scroll, 0);
    }
}
