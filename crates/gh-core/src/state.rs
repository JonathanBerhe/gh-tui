//! Top-level application state.

use crate::AuthState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Command,
}

impl Mode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Command => "COMMAND",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub mode: Mode,
    pub pending: String,
    pub auth: AuthState,
    pub should_quit: bool,
}
