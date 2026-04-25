//! Pure reducer. Given `(State, Msg)`, returns `(State, Vec<Cmd>)`.

use crate::{auth::AuthState, cmd::Cmd, msg::Msg, state::State};

/// Commands to dispatch on startup, before any user input.
#[must_use]
pub fn initial_commands() -> Vec<Cmd> {
    vec![Cmd::AuthenticateFromGh]
}

#[must_use]
pub fn reduce(mut state: State, msg: Msg) -> (State, Vec<Cmd>) {
    let cmds = Vec::new();
    match msg {
        Msg::Quit => {
            state.should_quit = true;
        }
        Msg::Tick => {}
        Msg::AuthReady { host, user } => {
            state.auth = AuthState::Authenticated { host, user };
        }
        Msg::AuthMissing { reason } => {
            state.auth = AuthState::Missing { reason };
        }
        Msg::PendingChanged(buf) => {
            state.pending = buf;
        }
    }
    (state, cmds)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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
        assert_eq!(
            state.auth,
            AuthState::Missing {
                reason: "gh not installed".into()
            }
        );
    }

    #[test]
    fn tick_is_no_op() {
        let (state, cmds) = reduce(State::default(), Msg::Tick);
        assert!(!state.should_quit);
        assert_eq!(state.auth, AuthState::Unknown);
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
        let (state, cmds) = reduce(State::default(), Msg::PendingChanged("2d3".into()));
        assert_eq!(state.pending, "2d3");
        assert!(cmds.is_empty());
    }

    #[test]
    fn pending_changed_can_clear() {
        let s = State {
            pending: "g".into(),
            ..State::default()
        };
        let (state, _) = reduce(s, Msg::PendingChanged(String::new()));
        assert_eq!(state.pending, "");
    }
}
