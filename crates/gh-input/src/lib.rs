//! Vim-style key resolver for `gh-tui`.
//!
//! Pure function from `KeyEvent` sequences to [`Action`] via a grammar-style
//! [`Resolver`] with counts, operators, and a g-prefix accumulator. No
//! in-workspace dependencies; consumes `crossterm::event::KeyEvent` at the
//! edge.
//!
//! Contract: [`Resolver::feed`] returns [`Resolution::Pending`] for in-progress
//! commands, [`Resolution::Action`] when a complete command resolves, or
//! [`Resolution::Cancel`] when `Esc` aborts a partial command.
//!
//! Phase 2 ships the grammar foundation (counts, motions, operators).
//! Context stacks and TOML keymaps land in subsequent PRs.

pub mod action;
pub mod resolver;

pub use action::{Action, Direction, Motion, Operator};
pub use resolver::{Resolution, Resolver};
