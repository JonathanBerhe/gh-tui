//! Vim-style key resolver for `gh-tui`.
//!
//! Pure function from key-event sequences to [`Action`] via a grammar-style
//! `PendingCommand` accumulator with counts, operators, registers, and
//! layered context stacks. No in-workspace dependencies.
//!
//! Contract: `resolve(key) -> Resolution::{Pending, Action, Cancel}`.
//!
//! Phase 1 ships a stub that maps `q` and `Ctrl+C` to [`Action::Quit`]. The
//! full grammar-driven resolver lands in Phase 2.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Pending,
    Action(Action),
    Cancel,
}

#[must_use]
pub fn resolve(event: KeyEvent) -> Resolution {
    match event.code {
        KeyCode::Char('q') if !event.modifiers.contains(KeyModifiers::CONTROL) => {
            Resolution::Action(Action::Quit)
        }
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
            Resolution::Action(Action::Quit)
        }
        KeyCode::Esc => Resolution::Cancel,
        _ => Resolution::Action(Action::None),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use pretty_assertions::assert_eq;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn q_resolves_to_quit() {
        assert_eq!(
            resolve(key(KeyCode::Char('q'))),
            Resolution::Action(Action::Quit)
        );
    }

    #[test]
    fn ctrl_c_resolves_to_quit() {
        assert_eq!(resolve(ctrl('c')), Resolution::Action(Action::Quit));
    }

    #[test]
    fn plain_c_does_not_quit() {
        assert_eq!(
            resolve(key(KeyCode::Char('c'))),
            Resolution::Action(Action::None)
        );
    }

    #[test]
    fn esc_cancels() {
        assert_eq!(resolve(key(KeyCode::Esc)), Resolution::Cancel);
    }

    #[test]
    fn unrelated_key_is_none_action() {
        assert_eq!(
            resolve(key(KeyCode::Char('j'))),
            Resolution::Action(Action::None)
        );
    }

    #[test]
    fn ctrl_q_is_not_quit() {
        // Stub only binds bare `q` and `Ctrl+C`; `Ctrl+Q` must not hijack.
        assert_eq!(resolve(ctrl('q')), Resolution::Action(Action::None));
    }
}
