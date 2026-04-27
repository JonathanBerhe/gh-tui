//! One-line status bar shared by every screen.
//!
//! Format: `MODE  auth-summary  rate-limit  [pending]  context`
//! - `rate-limit` is `remaining/limit` (e.g. `4998/5000`) coloured by
//!   severity once a [`RateLimit`] has been observed.
//! - `pending` shows the in-progress vim command.
//! - `context` is screen-specific (e.g. `cli/cli` in PR list mode).

use gh_core::{RateLimit, Screen, State, Tier};
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
    let rate = rate_limit_span(state.rate_limit.as_ref());
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
        Screen::LoadingDetail { repo, number } => Span::styled(
            format!("  {} #{number}", repo.slug()),
            Style::default().fg(Color::DarkGray),
        ),
        Screen::PrDetail { repo, detail, .. } => Span::styled(
            format!("  {} #{}", repo.slug(), detail.number),
            Style::default().fg(Color::DarkGray),
        ),
        Screen::Welcome | Screen::Error { .. } => Span::raw(String::new()),
    };

    Paragraph::new(Line::from(vec![mode, sep, auth, rate, pending, context]))
        .style(Style::default().bg(Color::Rgb(30, 30, 40)))
}

fn rate_limit_span(rl: Option<&RateLimit>) -> Span<'static> {
    let Some(rl) = rl else {
        return Span::raw(String::new());
    };
    let colour = match rl.tier() {
        Tier::Healthy => Color::DarkGray,
        Tier::Warning => Color::Yellow,
        Tier::Critical => Color::Red,
    };
    Span::styled(
        format!("  {}/{}", rl.remaining, rl.limit),
        Style::default().fg(colour),
    )
}
