//! Terminal lifecycle: RAII guard + panic hook that restores the terminal
//! before propagating, so a crash never leaves the user with a broken shell.

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore();
    }
}

/// Idempotent terminal restoration. Safe to call from a panic hook and from
/// `Drop`; best-effort, never panics.
pub fn restore() -> io::Result<()> {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture, Show);
    disable_raw_mode()
}

pub fn new_terminal() -> Result<Tui> {
    let backend = CrosstermBackend::new(io::stdout());
    Ok(Terminal::new(backend)?)
}

/// Install a panic hook that restores the terminal before delegating to the
/// previous hook. Must be called **before** `TerminalGuard::enter`.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        previous(info);
    }));
}
