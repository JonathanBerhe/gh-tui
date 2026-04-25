//! Pure domain for `gh-tui`: [`State`], [`Msg`], [`Cmd`], and reducers.
//!
//! This crate has **no I/O** and **no `ratatui` dependency**. Reducers are pure
//! functions `(State, Msg) -> (State, Vec<Cmd>)`; commands describe side
//! effects that workers in the binary dispatch against `gh-api` and friends.
//!
//! See `CLAUDE.md` at the repo root for architectural invariants.

pub mod auth;
pub mod cmd;
pub mod msg;
pub mod reducer;
pub mod state;

pub use auth::AuthState;
pub use cmd::Cmd;
pub use msg::Msg;
pub use reducer::{initial_commands, reduce};
pub use state::{Mode, State};
