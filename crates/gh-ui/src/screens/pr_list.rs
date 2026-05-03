//! PR list screen: a column-aligned table of open pull requests with an
//! optional "loading more…" footer while the next page is in flight.

use gh_core::PrSummary;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

/// Column widths. `#N` fits 6-digit issue numbers (cli/cli is at ~13k);
/// state badge is fixed; title takes the rest of the row; author and stats
/// are right-sized for typical content.
const COL_NUMBER: u16 = 7;
const COL_STATE: u16 = 6;
const COL_AUTHOR: u16 = 22;
const COL_STATS: u16 = 11;
const COL_COMMENTS: u16 = 5;

pub fn draw(
    items: &[PrSummary],
    selected: usize,
    loading_next: bool,
    frame: &mut Frame<'_>,
    area: Rect,
) {
    if items.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            "  No open pull requests.",
            Style::default().fg(Color::DarkGray),
        )))
        .block(Block::default().borders(Borders::NONE));
        frame.render_widget(p, area);
        return;
    }

    // Reserve a single line at the bottom for the "Loading more…" hint when
    // applicable; otherwise the table takes the full area.
    let chunks = if loading_next {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(area)
    };

    let header = Row::new(vec![
        Cell::from("    #"),
        Cell::from(""),
        Cell::from("TITLE"),
        Cell::from("AUTHOR"),
        Cell::from("    +/-"),
        Cell::from("  💬"),
    ])
    .style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let widths = [
        Constraint::Length(COL_NUMBER),
        Constraint::Length(COL_STATE),
        Constraint::Min(20),
        Constraint::Length(COL_AUTHOR),
        Constraint::Length(COL_STATS),
        Constraint::Length(COL_COMMENTS),
    ];

    let rows: Vec<Row<'static>> = items.iter().map(render_row).collect();

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▎ ");

    let mut state = TableState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(table, chunks[0], &mut state);

    if loading_next {
        let hint = Paragraph::new(Line::from(Span::styled(
            "  loading more…",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
        frame.render_widget(hint, chunks[1]);
    }
}

fn render_row(p: &PrSummary) -> Row<'static> {
    let number = Cell::from(Line::from(Span::styled(
        format!("#{}", p.number),
        Style::default().fg(Color::DarkGray),
    )));

    let state = if p.draft {
        Cell::from(Line::from(Span::styled(
            " DRAFT",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )))
    } else {
        // Open PRs get a small green dot. We don't carry merged/closed state
        // through the list query yet — those land when the search-driven
        // list lands in a future phase.
        Cell::from(Line::from(Span::styled(
            "  ●",
            Style::default().fg(Color::Green),
        )))
    };

    let title = Cell::from(Line::from(Span::styled(
        p.title.clone(),
        Style::default().fg(Color::White),
    )));

    let author = Cell::from(Line::from(Span::styled(
        format!("@{}", p.author),
        Style::default().fg(Color::Cyan),
    )));

    let stats = Cell::from(Line::from(vec![
        Span::styled(
            format!("+{}", p.additions),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" "),
        Span::styled(format!("-{}", p.deletions), Style::default().fg(Color::Red)),
    ]));

    let comments = Cell::from(Line::from(Span::styled(
        p.comments.to_string(),
        Style::default().fg(Color::DarkGray),
    )));

    Row::new(vec![number, state, title, author, stats, comments])
}
