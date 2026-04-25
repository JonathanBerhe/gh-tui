//! Phase 1 placeholder screen. Centered body + one-line status bar.

use gh_core::State;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn draw(state: &State, frame: &mut Frame<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    frame.render_widget(body(state), chunks[0]);
    frame.render_widget(status_bar(state), chunks[1]);
}

fn body(_state: &State) -> Paragraph<'static> {
    let title = Line::from(vec![
        Span::styled("gh-tui", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" — press "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" to quit"),
    ]);
    let hint = Line::from(Span::styled(
        "Phase 1: MVU skeleton + auth",
        Style::default().fg(Color::DarkGray),
    ));

    Paragraph::new(vec![Line::raw(""), title, Line::raw(""), hint])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE))
}

fn status_bar(state: &State) -> Paragraph<'static> {
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

    Paragraph::new(Line::from(vec![mode, sep, auth, pending]))
        .style(Style::default().bg(Color::Rgb(30, 30, 40)))
}
