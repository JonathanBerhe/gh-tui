//! Vim-style key resolver. Pure function from `KeyEvent` sequences to
//! [`Action`] via a grammar-style accumulator that tracks pre/post-operator
//! counts, the active operator, and the `g`-prefix state.
//!
//! Grammar (Phase 2 subset):
//!
//! ```text
//! command   := count? ( motion | operator count? motion-or-self )
//! count     := digit+ ; bare `0` is the line-start motion, not a digit
//! motion    := h | j | k | l | w | b | gg | G | 0 | $
//! operator  := d | y | c
//! ```
//!
//! `<op><op>` (e.g. `dd`) operates on the current line.
//! `<count1><op><count2><motion>` multiplies counts (vim semantics).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, Motion, Operator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// More input expected to complete the command.
    Pending,
    /// A complete command resolved.
    Action(Action),
    /// User aborted a partial command (`Esc`); resolver is now empty.
    Cancel,
}

#[derive(Default, Debug, Clone)]
struct PendingCommand {
    count1: Option<usize>,
    operator: Option<Operator>,
    count2: Option<usize>,
    /// `g` pressed once; waiting for the second key (e.g. `gg`).
    g_partial: bool,
}

impl PendingCommand {
    fn is_empty(&self) -> bool {
        self.count1.is_none() && self.operator.is_none() && self.count2.is_none() && !self.g_partial
    }
}

#[derive(Default, Debug, Clone)]
pub struct Resolver {
    pending: PendingCommand,
}

impl Resolver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Human-readable rendering of the pending buffer for the status bar.
    /// Empty string when no command is in progress.
    #[must_use]
    pub fn pending_display(&self) -> String {
        let mut s = String::new();
        if let Some(c) = self.pending.count1 {
            s.push_str(&c.to_string());
        }
        if let Some(op) = self.pending.operator {
            s.push(op.key());
        }
        if let Some(c) = self.pending.count2 {
            s.push_str(&c.to_string());
        }
        if self.pending.g_partial {
            s.push('g');
        }
        s
    }

    /// Feed one key event to the resolver.
    pub fn feed(&mut self, key: KeyEvent) -> Resolution {
        // Esc always cancels (and reports None on an empty resolver).
        if key.code == KeyCode::Esc {
            let was_empty = self.pending.is_empty();
            self.reset();
            return if was_empty {
                Resolution::Action(Action::None)
            } else {
                Resolution::Cancel
            };
        }

        // Ctrl+C always quits, regardless of pending state.
        if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.reset();
            return Resolution::Action(Action::Quit);
        }

        // `g`-prefix completion takes priority (consumes the next key).
        if self.pending.g_partial {
            self.pending.g_partial = false;
            return match key.code {
                KeyCode::Char('g') => self.complete_motion(Motion::DocStart),
                _ => self.cancel_pending(),
            };
        }

        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let d = c.to_digit(10).unwrap_or(0) as usize;
                self.feed_digit(d)
            }

            // Bare `q` quits when not modifying with Ctrl/Alt.
            KeyCode::Char('q')
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.reset();
                Resolution::Action(Action::Quit)
            }

            // Operators.
            KeyCode::Char('d') => self.feed_operator(Operator::Delete),
            KeyCode::Char('y') => self.feed_operator(Operator::Yank),
            KeyCode::Char('c') => self.feed_operator(Operator::Change),

            // `g`-prefix start.
            KeyCode::Char('g') => {
                self.pending.g_partial = true;
                Resolution::Pending
            }

            // Single-key motions.
            KeyCode::Char('h') | KeyCode::Left => self.complete_motion(Motion::Left),
            KeyCode::Char('j') | KeyCode::Down => self.complete_motion(Motion::Down),
            KeyCode::Char('k') | KeyCode::Up => self.complete_motion(Motion::Up),
            KeyCode::Char('l') | KeyCode::Right => self.complete_motion(Motion::Right),
            KeyCode::Char('w') => self.complete_motion(Motion::WordForward),
            KeyCode::Char('b') => self.complete_motion(Motion::WordBackward),
            KeyCode::Char('G') => self.complete_motion(Motion::DocEnd),
            KeyCode::Char('$') => self.complete_motion(Motion::LineEnd),
            // `0` is handled in `feed_digit`.
            _ => {
                if self.pending.is_empty() {
                    Resolution::Action(Action::None)
                } else {
                    self.cancel_pending()
                }
            }
        }
    }

    fn feed_digit(&mut self, d: usize) -> Resolution {
        let target = if self.pending.operator.is_none() {
            &mut self.pending.count1
        } else {
            &mut self.pending.count2
        };
        if d == 0 && target.is_none() {
            return self.complete_motion(Motion::LineStart);
        }
        *target = Some(target.unwrap_or(0) * 10 + d);
        Resolution::Pending
    }

    fn feed_operator(&mut self, op: Operator) -> Resolution {
        match self.pending.operator {
            // Same operator twice = operate on current line (`dd`, `yy`, `cc`).
            Some(existing) if existing == op => self.complete_motion(Motion::CurrentLine),
            // Conflicting operator mid-command — cancel.
            Some(_) => self.cancel_pending(),
            None => {
                self.pending.operator = Some(op);
                Resolution::Pending
            }
        }
    }

    fn complete_motion(&mut self, motion: Motion) -> Resolution {
        let count1 = self.pending.count1.unwrap_or(1);
        let action = match self.pending.operator {
            Some(op) => {
                let count2 = self.pending.count2.unwrap_or(1);
                Action::Operate {
                    count: count1 * count2,
                    op,
                    motion,
                }
            }
            None => Action::Move {
                count: count1,
                motion,
            },
        };
        self.reset();
        Resolution::Action(action)
    }

    fn cancel_pending(&mut self) -> Resolution {
        self.reset();
        Resolution::Cancel
    }

    fn reset(&mut self) {
        self.pending = PendingCommand::default();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use pretty_assertions::assert_eq;

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        k(KeyCode::Char(c))
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn feed_seq(keys: &[KeyEvent]) -> Vec<Resolution> {
        let mut r = Resolver::new();
        keys.iter().map(|k| r.feed(*k)).collect()
    }

    fn final_action(keys: &[KeyEvent]) -> Action {
        let mut r = Resolver::new();
        let mut last = Resolution::Pending;
        for key in keys {
            last = r.feed(*key);
        }
        match last {
            Resolution::Action(a) => a,
            other => panic!("expected Action, got {other:?}"),
        }
    }

    // ── quit ────────────────────────────────────────────────────────────

    #[test]
    fn q_quits() {
        assert_eq!(final_action(&[ch('q')]), Action::Quit);
    }

    #[test]
    fn ctrl_c_quits() {
        assert_eq!(final_action(&[ctrl('c')]), Action::Quit);
    }

    #[test]
    fn ctrl_q_does_not_quit() {
        // Ctrl+Q is not bound; with no pending state, returns Action::None.
        assert_eq!(final_action(&[ctrl('q')]), Action::None);
    }

    #[test]
    fn ctrl_c_quits_even_mid_command() {
        let mut r = Resolver::new();
        assert_eq!(r.feed(ch('d')), Resolution::Pending);
        assert_eq!(r.feed(ctrl('c')), Resolution::Action(Action::Quit));
    }

    // ── escape / cancel ─────────────────────────────────────────────────

    #[test]
    fn esc_on_empty_is_none() {
        assert_eq!(final_action(&[k(KeyCode::Esc)]), Action::None);
    }

    #[test]
    fn esc_mid_command_cancels_and_resets() {
        let mut r = Resolver::new();
        assert_eq!(r.feed(ch('d')), Resolution::Pending);
        assert_eq!(r.feed(k(KeyCode::Esc)), Resolution::Cancel);
        // Resolver is now empty: a fresh `j` should produce Move(Down, 1).
        assert_eq!(
            r.feed(ch('j')),
            Resolution::Action(Action::Move {
                count: 1,
                motion: Motion::Down
            })
        );
    }

    // ── single-key motions ──────────────────────────────────────────────

    #[test]
    fn j_moves_down() {
        assert_eq!(
            final_action(&[ch('j')]),
            Action::Move {
                count: 1,
                motion: Motion::Down
            }
        );
    }

    #[test]
    fn k_moves_up() {
        assert_eq!(
            final_action(&[ch('k')]),
            Action::Move {
                count: 1,
                motion: Motion::Up
            }
        );
    }

    #[test]
    fn h_moves_left() {
        assert_eq!(
            final_action(&[ch('h')]),
            Action::Move {
                count: 1,
                motion: Motion::Left
            }
        );
    }

    #[test]
    fn l_moves_right() {
        assert_eq!(
            final_action(&[ch('l')]),
            Action::Move {
                count: 1,
                motion: Motion::Right
            }
        );
    }

    #[test]
    fn arrow_keys_match_hjkl() {
        assert_eq!(
            final_action(&[k(KeyCode::Down)]).to_motion(),
            Some(Motion::Down)
        );
        assert_eq!(
            final_action(&[k(KeyCode::Up)]).to_motion(),
            Some(Motion::Up)
        );
        assert_eq!(
            final_action(&[k(KeyCode::Left)]).to_motion(),
            Some(Motion::Left)
        );
        assert_eq!(
            final_action(&[k(KeyCode::Right)]).to_motion(),
            Some(Motion::Right)
        );
    }

    #[test]
    fn capital_g_goes_to_doc_end() {
        assert_eq!(
            final_action(&[ch('G')]),
            Action::Move {
                count: 1,
                motion: Motion::DocEnd
            }
        );
    }

    #[test]
    fn dollar_goes_to_line_end() {
        assert_eq!(
            final_action(&[ch('$')]),
            Action::Move {
                count: 1,
                motion: Motion::LineEnd
            }
        );
    }

    // ── multi-key motions ───────────────────────────────────────────────

    #[test]
    fn gg_goes_to_doc_start() {
        let r = feed_seq(&[ch('g'), ch('g')]);
        assert_eq!(r[0], Resolution::Pending);
        assert_eq!(
            r[1],
            Resolution::Action(Action::Move {
                count: 1,
                motion: Motion::DocStart
            })
        );
    }

    #[test]
    fn g_then_other_cancels() {
        let r = feed_seq(&[ch('g'), ch('x')]);
        assert_eq!(r[0], Resolution::Pending);
        assert_eq!(r[1], Resolution::Cancel);
    }

    // ── counts ──────────────────────────────────────────────────────────

    #[test]
    fn count_then_motion() {
        let r = feed_seq(&[ch('1'), ch('0'), ch('j')]);
        assert_eq!(r[0], Resolution::Pending);
        assert_eq!(r[1], Resolution::Pending);
        assert_eq!(
            r[2],
            Resolution::Action(Action::Move {
                count: 10,
                motion: Motion::Down
            })
        );
    }

    #[test]
    fn count_with_gg() {
        let r = feed_seq(&[ch('5'), ch('g'), ch('g')]);
        assert_eq!(r[0], Resolution::Pending);
        assert_eq!(r[1], Resolution::Pending);
        assert_eq!(
            r[2],
            Resolution::Action(Action::Move {
                count: 5,
                motion: Motion::DocStart
            })
        );
    }

    #[test]
    fn bare_zero_is_line_start_motion() {
        assert_eq!(
            final_action(&[ch('0')]),
            Action::Move {
                count: 1,
                motion: Motion::LineStart
            }
        );
    }

    #[test]
    fn zero_after_digit_is_a_digit() {
        // `10j` — the `0` extends the count, doesn't fire LineStart.
        assert_eq!(
            final_action(&[ch('1'), ch('0'), ch('j')]),
            Action::Move {
                count: 10,
                motion: Motion::Down
            }
        );
    }

    #[test]
    fn three_digit_count() {
        assert_eq!(
            final_action(&[ch('1'), ch('2'), ch('3'), ch('j')]),
            Action::Move {
                count: 123,
                motion: Motion::Down
            }
        );
    }

    // ── operators ───────────────────────────────────────────────────────

    #[test]
    fn dw_deletes_word_forward() {
        let r = feed_seq(&[ch('d'), ch('w')]);
        assert_eq!(r[0], Resolution::Pending);
        assert_eq!(
            r[1],
            Resolution::Action(Action::Operate {
                count: 1,
                op: Operator::Delete,
                motion: Motion::WordForward
            })
        );
    }

    #[test]
    fn yw_yanks_word() {
        assert_eq!(
            final_action(&[ch('y'), ch('w')]),
            Action::Operate {
                count: 1,
                op: Operator::Yank,
                motion: Motion::WordForward
            }
        );
    }

    #[test]
    fn cw_changes_word() {
        assert_eq!(
            final_action(&[ch('c'), ch('w')]),
            Action::Operate {
                count: 1,
                op: Operator::Change,
                motion: Motion::WordForward
            }
        );
    }

    #[test]
    fn dd_operates_on_current_line() {
        assert_eq!(
            final_action(&[ch('d'), ch('d')]),
            Action::Operate {
                count: 1,
                op: Operator::Delete,
                motion: Motion::CurrentLine
            }
        );
    }

    #[test]
    fn yy_operates_on_current_line() {
        assert_eq!(
            final_action(&[ch('y'), ch('y')]),
            Action::Operate {
                count: 1,
                op: Operator::Yank,
                motion: Motion::CurrentLine
            }
        );
    }

    #[test]
    fn cc_operates_on_current_line() {
        assert_eq!(
            final_action(&[ch('c'), ch('c')]),
            Action::Operate {
                count: 1,
                op: Operator::Change,
                motion: Motion::CurrentLine
            }
        );
    }

    #[test]
    fn d_then_y_cancels() {
        // Conflicting operator mid-command.
        let r = feed_seq(&[ch('d'), ch('y')]);
        assert_eq!(r[0], Resolution::Pending);
        assert_eq!(r[1], Resolution::Cancel);
    }

    // ── operator + count + motion ───────────────────────────────────────

    #[test]
    fn d3w_deletes_three_words() {
        assert_eq!(
            final_action(&[ch('d'), ch('3'), ch('w')]),
            Action::Operate {
                count: 3,
                op: Operator::Delete,
                motion: Motion::WordForward
            }
        );
    }

    #[test]
    fn pre_count_operator_motion() {
        // `2dw` — pre-count multiplies into the operator's effect.
        assert_eq!(
            final_action(&[ch('2'), ch('d'), ch('w')]),
            Action::Operate {
                count: 2,
                op: Operator::Delete,
                motion: Motion::WordForward
            }
        );
    }

    #[test]
    fn pre_count_operator_post_count_motion_multiplies() {
        // `2d3w` — counts multiply (vim semantics): delete 6 words.
        assert_eq!(
            final_action(&[ch('2'), ch('d'), ch('3'), ch('w')]),
            Action::Operate {
                count: 6,
                op: Operator::Delete,
                motion: Motion::WordForward
            }
        );
    }

    #[test]
    fn count_then_dd() {
        // `3dd` — delete 3 lines.
        assert_eq!(
            final_action(&[ch('3'), ch('d'), ch('d')]),
            Action::Operate {
                count: 3,
                op: Operator::Delete,
                motion: Motion::CurrentLine
            }
        );
    }

    #[test]
    fn d_zero_targets_line_start() {
        // `d0` — delete to line start.
        assert_eq!(
            final_action(&[ch('d'), ch('0')]),
            Action::Operate {
                count: 1,
                op: Operator::Delete,
                motion: Motion::LineStart
            }
        );
    }

    #[test]
    fn d_dollar_targets_line_end() {
        assert_eq!(
            final_action(&[ch('d'), ch('$')]),
            Action::Operate {
                count: 1,
                op: Operator::Delete,
                motion: Motion::LineEnd
            }
        );
    }

    #[test]
    fn d_gg_targets_doc_start() {
        let r = feed_seq(&[ch('d'), ch('g'), ch('g')]);
        assert_eq!(r[0], Resolution::Pending);
        assert_eq!(r[1], Resolution::Pending);
        assert_eq!(
            r[2],
            Resolution::Action(Action::Operate {
                count: 1,
                op: Operator::Delete,
                motion: Motion::DocStart
            })
        );
    }

    // ── pending display ─────────────────────────────────────────────────

    #[test]
    fn pending_display_empty_initially() {
        let r = Resolver::new();
        assert_eq!(r.pending_display(), "");
    }

    #[test]
    fn pending_display_renders_in_progress_command() {
        let mut r = Resolver::new();
        r.feed(ch('1'));
        r.feed(ch('2'));
        r.feed(ch('d'));
        r.feed(ch('3'));
        assert_eq!(r.pending_display(), "12d3");
        r.feed(ch('w'));
        assert_eq!(r.pending_display(), "");
    }

    #[test]
    fn pending_display_g_partial() {
        let mut r = Resolver::new();
        r.feed(ch('g'));
        assert_eq!(r.pending_display(), "g");
    }

    // ── reset behaviour ─────────────────────────────────────────────────

    #[test]
    fn resolver_clean_after_action() {
        let mut r = Resolver::new();
        r.feed(ch('5'));
        r.feed(ch('j'));
        // After completion, fresh keys start a new command.
        assert_eq!(
            r.feed(ch('k')),
            Resolution::Action(Action::Move {
                count: 1,
                motion: Motion::Up
            })
        );
    }

    /// Helper: extract the motion from a Move/Operate action for arrow-key tests.
    impl Action {
        fn to_motion(self) -> Option<Motion> {
            match self {
                Self::Move { motion, .. } | Self::Operate { motion, .. } => Some(motion),
                _ => None,
            }
        }
    }
}
