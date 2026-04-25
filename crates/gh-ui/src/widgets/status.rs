//! One-line status bar shared by every screen.
//!
//! Format: `MODE  auth-summary  [pending]  context`
//! - `context` is screen-specific (e.g. `cli/cli` in PR list mode).
//! - PR #3 adds a rate-limit segment between auth and pending.

use gh_core::{Screen, State};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[must_use]
pub fn status_bar(state: &State) -> Paragraph<'static> {
    let mode = Span::styled(
        format!(" {} ", state.mode.label()),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let sep = Span::raw("  ");
    let auth = Span::raw(state.auth.summary());
    let pending = if state.pending.is_empty() {
        Span::raw(String::new())
    } else {
        Span::styled(
            format!("  [{}]", state.pending),
            Style::default().fg(Color::Yellow),
        )
    };
    let context = match &state.screen {
        Screen::PrList { repo, .. } | Screen::Loading { repo } => Span::styled(
            format!("  {}", repo.slug()),
            Style::default().fg(Color::DarkGray),
        ),
        Screen::Welcome | Screen::Error { .. } => Span::raw(String::new()),
    };

    Paragraph::new(Line::from(vec![mode, sep, auth, pending, context]))
        .style(Style::default().bg(Color::Rgb(30, 30, 40)))
}
