//! Tracing setup: daily-rolled log file under `$XDG_STATE_HOME/gh-tui/`.
//!
//! Never panics on setup failure — logs a stderr warning before the TUI takes
//! the terminal, and continues with whatever subscriber did install.

use std::path::{Path, PathBuf};

use anyhow::Result;
use directories::ProjectDirs;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[must_use]
pub fn log_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "gh-tui").and_then(|p| {
        let dir = p
            .state_dir()
            .unwrap_or_else(|| p.data_local_dir())
            .to_path_buf();
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    })
}

pub fn install(level: &str, override_file: Option<&Path>) -> Result<WorkerGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let (writer, guard) = match override_file {
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let appender = RollingFileAppender::new(
                Rotation::NEVER,
                path.parent().unwrap_or_else(|| Path::new(".")),
                path.file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("gh-tui.log")),
            );
            tracing_appender::non_blocking(appender)
        }
        None => {
            let dir = log_dir().unwrap_or_else(|| PathBuf::from("."));
            let appender = RollingFileAppender::new(Rotation::DAILY, &dir, "gh-tui.log");
            tracing_appender::non_blocking(appender)
        }
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(writer).with_ansi(false))
        .init();

    Ok(guard)
}
