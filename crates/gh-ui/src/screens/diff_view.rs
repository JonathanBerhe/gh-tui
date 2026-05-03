//! PR diff screen: title, summary line (file count, totals), separator,
//! scrollable diff body via `gh_render::render_diff`.

use gh_core::FilePatch;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw(files: &[FilePatch], scroll: u16, frame: &mut Frame<'_>, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(1), // summary
            Constraint::Length(1), // separator
            Constraint::Min(1),    // body
        ])
        .split(area);

    frame.render_widget(title_line(files.len()), chunks[0]);
    frame.render_widget(summary_line(files), chunks[1]);
    frame.render_widget(separator(), chunks[2]);
    frame.render_widget(body(files, scroll), chunks[3]);
}

fn title_line(file_count: usize) -> Paragraph<'static> {
    let label = Span::styled(
        "diff",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    let count = Span::styled(
        format!(
            "  {file_count} file{}",
            if file_count == 1 { "" } else { "s" }
        ),
        Style::default().fg(Color::DarkGray),
    );
    Paragraph::new(Line::from(vec![label, count]))
}

fn summary_line(files: &[FilePatch]) -> Paragraph<'static> {
    let (adds, dels) = files
        .iter()
        .fold((0u32, 0u32), |(a, d), f| (a + f.additions, d + f.deletions));
    let stats = Span::styled(
        format!("+{adds} -{dels}"),
        Style::default().fg(Color::DarkGray),
    );
    let omitted = files.iter().filter(|f| f.patch.is_none()).count();
    let mut spans = vec![stats];
    if omitted > 0 {
        spans.push(Span::styled(
            format!("  •  {omitted} omitted (too large or binary)"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    Paragraph::new(Line::from(spans))
}

fn separator() -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(
        "─".repeat(120),
        Style::default().fg(Color::DarkGray),
    )))
}

fn body(files: &[FilePatch], scroll: u16) -> Paragraph<'static> {
    let lines = if files.is_empty() {
        vec![Line::from(Span::styled(
            "(no changed files)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        gh_render::render_diff(files)
    };

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::NONE))
}
