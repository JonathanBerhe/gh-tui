//! `gh-tui` entry point: argv parsing, tracing setup, panic guard, and the
//! MVU event loop driver.

mod app;
mod terminal;
mod tracing_init;
mod workers;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "gh-tui", version, about = "Fast terminal UI for GitHub")]
struct Args {
    /// `owner/name` of the repo to show. If omitted, infers from cwd via
    /// `gh repo view`.
    repo: Option<String>,

    /// Logging verbosity (honors `RUST_LOG` if set).
    #[arg(long, default_value = "warn", env = "GH_TUI_LOG_LEVEL")]
    log_level: String,

    /// Override the log file path (default: `$XDG_STATE_HOME/gh-tui/gh-tui.log`).
    #[arg(long)]
    log_file: Option<PathBuf>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Install logging first so early-startup events are captured.
    let _tracing_guard = tracing_init::install(&args.log_level, args.log_file.as_deref())?;

    // Probe the terminal's image protocol BEFORE entering the alt-screen
    // so the synchronous query reaches the host terminal directly. `None`
    // means we render placeholder text for image chunks.
    let picker = gh_ui::detect_picker();

    // Panic hook must be installed BEFORE raw mode, and must restore the
    // terminal BEFORE the previous hook prints the payload.
    terminal::install_panic_hook();

    let _terminal_guard = terminal::TerminalGuard::enter()?;
    let terminal = terminal::new_terminal()?;

    app::run(terminal, args.repo, picker).await
}
