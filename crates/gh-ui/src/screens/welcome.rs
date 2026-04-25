//! Pre-bootstrap placeholder body. Used while auth resolves and no repo has
//! been determined yet. Status bar is drawn separately by the dispatcher.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn draw(frame: &mut Frame<'_>, area: Rect) {
    let title = Line::from(vec![
        Span::styled("gh-tui", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" — press "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" to quit"),
    ]);
    let hint = Line::from(Span::styled(
        "pass `owner/name` or `cd` into a repo",
        Style::default().fg(Color::DarkGray),
    ));

    let p = Paragraph::new(vec![Line::raw(""), title, Line::raw(""), hint])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    frame.render_widget(p, area);
}
