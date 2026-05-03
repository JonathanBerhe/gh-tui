//! PR diff screen: title, summary line, separator, scrollable body.
//! Body layout is either unified (single column via `gh_render::render_diff`)
//! or split (two columns via `gh_render::render_diff_split`); the active
//! `DiffViewMode` decides which sub-renderer runs.

mod split;

use gh_core::{DiffViewMode, FilePatch, ReviewThread};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw(
    files: &[FilePatch],
    threads: &[ReviewThread],
    scroll: u16,
    view_mode: DiffViewMode,
    frame: &mut Frame<'_>,
    area: Rect,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(1), // summary
            Constraint::Length(1), // separator
            Constraint::Min(1),    // body
        ])
        .split(area);

    frame.render_widget(title_line(files.len(), threads.len(), view_mode), chunks[0]);
    frame.render_widget(summary_line(files), chunks[1]);
    frame.render_widget(separator(), chunks[2]);
    match view_mode {
        DiffViewMode::Unified => {
            frame.render_widget(unified_body(files, threads, scroll), chunks[3]);
        }
        DiffViewMode::Split => split::draw(files, threads, scroll, frame, chunks[3]),
    }
}

fn title_line(
    file_count: usize,
    thread_count: usize,
    view_mode: DiffViewMode,
) -> Paragraph<'static> {
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
    let mode = Span::styled(
        format!(
            "  •  {}",
            match view_mode {
                DiffViewMode::Unified => "unified",
                DiffViewMode::Split => "split",
            }
        ),
        Style::default().fg(Color::DarkGray),
    );
    let mut spans = vec![label, count, mode];
    if thread_count > 0 {
        spans.push(Span::styled(
            format!(
                "  •  {thread_count} review thread{}",
                if thread_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }
    Paragraph::new(Line::from(spans))
}

fn summary_line(files: &[FilePatch]) -> Paragraph<'static> {
    let (adds, dels, omitted) = files.iter().fold((0u32, 0u32, 0usize), |(a, d, o), f| {
        (
            a.saturating_add(f.additions),
            d.saturating_add(f.deletions),
            o + usize::from(f.patch.is_none()),
        )
    });
    let stats = Span::styled(
        format!("+{adds} -{dels}"),
        Style::default().fg(Color::DarkGray),
    );
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

fn unified_body(files: &[FilePatch], threads: &[ReviewThread], scroll: u16) -> Paragraph<'static> {
    let lines = if files.is_empty() {
        vec![Line::from(Span::styled(
            "(no changed files)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        gh_render::render_diff(files, threads)
    };

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::NONE))
}
