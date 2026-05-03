//! Split-view sub-renderer: takes the `(left, right)` row pairs from
//! `gh_render::render_diff_split` and lays them out in a two-column layout.
//! Both columns share the same `scroll` offset (synchronised vertical
//! scrolling).

use gh_core::FilePatch;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw(files: &[FilePatch], scroll: u16, frame: &mut Frame<'_>, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(1), // gutter
            Constraint::Percentage(50),
        ])
        .split(area);

    if files.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "(no changed files)",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(empty, cols[0]);
        return;
    }

    let rows = gh_render::render_diff_split(files);
    let (left_lines, right_lines): (Vec<Line<'static>>, Vec<Line<'static>>) =
        rows.into_iter().unzip();

    frame.render_widget(column(left_lines, scroll), cols[0]);
    frame.render_widget(gutter_column(), cols[1]);
    frame.render_widget(column(right_lines, scroll), cols[2]);
}

fn column(lines: Vec<Line<'static>>, scroll: u16) -> Paragraph<'static> {
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::NONE))
}

fn gutter_column() -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(
        "│",
        Style::default().fg(Color::DarkGray),
    )))
    .block(Block::default().borders(Borders::NONE))
}
