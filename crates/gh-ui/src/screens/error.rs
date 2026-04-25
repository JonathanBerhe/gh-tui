//! Full-screen error renderer with optional hint copy.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw(message: &str, hint: Option<&str>, frame: &mut Frame<'_>, area: Rect) {
    let mut lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "✘ error",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::raw(message.to_string())),
    ];
    if let Some(h) = hint {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("hint: {h}"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    let p = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::NONE));
    frame.render_widget(p, area);
}
