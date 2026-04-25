//! Pure reducer. Given `(State, Msg)`, returns `(State, Vec<Cmd>)`.

use crate::{
    auth::AuthState,
    cmd::Cmd,
    msg::{Msg, SelectionJump},
    state::{Screen, State},
};

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
            // off the fetch now.
            if let Screen::Loading { repo } = &state.screen {
                cmds.push(Cmd::FetchPrList { repo: repo.clone() });
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
                cmds.push(Cmd::FetchPrList { repo });
            }
        }
        Msg::RepoResolveFailed(reason) => {
            state.screen = Screen::Error {
                message: format!("could not determine repo: {reason}"),
                hint: Some("pass `owner/name` or run from a clone".to_string()),
            };
        }
        Msg::PrListReady { repo, items } => {
            state.screen = Screen::PrList {
                repo,
                items,
                selected: 0,
            };
        }
        Msg::PrListFailed(reason) => {
            state.screen = Screen::Error {
                message: format!("PR list failed: {reason}"),
                hint: None,
            };
        }
        Msg::SelectionDelta(delta) => {
            if let Screen::PrList {
                items, selected, ..
            } = &mut state.screen
            {
                let len = items.len();
                if len > 0 {
                    let new = (*selected as i64) + i64::from(delta);
                    let clamped = new.clamp(0, (len - 1) as i64);
                    *selected = clamped as usize;
                }
            }
        }
        Msg::SelectionJump(jump) => {
            if let Screen::PrList {
                items, selected, ..
            } = &mut state.screen
            {
                if !items.is_empty() {
                    *selected = match jump {
                        SelectionJump::First => 0,
                        SelectionJump::Last => items.len() - 1,
                    };
                }
            }
        }
        Msg::RateLimitUpdate(rl) => {
            state.rate_limit = Some(rl);
        }
    }
    (state, cmds)
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

    #[test]
    fn repo_resolved_with_auth_emits_fetch_cmd() {
        let (state, cmds) = reduce(authenticated(), Msg::RepoResolved(repo()));
        assert!(matches!(state.screen, Screen::Loading { .. }));
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::FetchPrList { .. }));
    }

    #[test]
    fn repo_resolved_without_auth_waits() {
        let (state, cmds) = reduce(State::default(), Msg::RepoResolved(repo()));
        assert!(matches!(state.screen, Screen::Loading { .. }));
        assert!(cmds.is_empty(), "must not fetch before auth resolves");
    }

    #[test]
    fn auth_ready_after_repo_kicks_off_fetch() {
        // Repo arrives first, auth still Unknown → no Cmd.
        let (s1, _) = reduce(State::default(), Msg::RepoResolved(repo()));
        // Auth then resolves → reducer emits the deferred FetchPrList.
        let (_, cmds) = reduce(
            s1,
            Msg::AuthReady {
                host: "github.com".into(),
                user: Some("alice".into()),
            },
        );
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::FetchPrList { .. }));
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
    fn pr_list_ready_transitions_screen() {
        let (state, _) = reduce(
            authenticated(),
            Msg::PrListReady {
                repo: repo(),
                items: vec![pr(1), pr(2), pr(3)],
            },
        );
        let Screen::PrList {
            items, selected, ..
        } = state.screen
        else {
            panic!("expected PrList");
        };
        assert_eq!(items.len(), 3);
        assert_eq!(selected, 0);
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
        let s = State {
            screen: Screen::PrList {
                repo: repo(),
                items: vec![pr(1), pr(2), pr(3)],
                selected: 0,
            },
            ..State::default()
        };
        let (state, _) = reduce(s, Msg::SelectionDelta(1));
        let Screen::PrList { selected, .. } = state.screen else {
            panic!("expected PrList");
        };
        assert_eq!(selected, 1);
    }

    #[test]
    fn selection_up_at_zero_stays_zero() {
        let s = State {
            screen: Screen::PrList {
                repo: repo(),
                items: vec![pr(1), pr(2)],
                selected: 0,
            },
            ..State::default()
        };
        let (state, _) = reduce(s, Msg::SelectionDelta(-1));
        let Screen::PrList { selected, .. } = state.screen else {
            panic!("expected PrList");
        };
        assert_eq!(selected, 0);
    }

    #[test]
    fn selection_down_at_end_stays() {
        let s = State {
            screen: Screen::PrList {
                repo: repo(),
                items: vec![pr(1), pr(2)],
                selected: 1,
            },
            ..State::default()
        };
        let (state, _) = reduce(s, Msg::SelectionDelta(5));
        let Screen::PrList { selected, .. } = state.screen else {
            panic!("expected PrList");
        };
        assert_eq!(selected, 1);
    }

    #[test]
    fn selection_jump_first_last() {
        let mk = |sel| State {
            screen: Screen::PrList {
                repo: repo(),
                items: vec![pr(1), pr(2), pr(3), pr(4)],
                selected: sel,
            },
            ..State::default()
        };
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
        let s = State {
            screen: Screen::PrList {
                repo: repo(),
                items: vec![],
                selected: 0,
            },
            ..State::default()
        };
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
}
