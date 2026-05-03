//! PR list screen: a column-aligned table of open pull requests with an
//! optional "loading more…" footer while the next page is in flight.

use chrono::{DateTime, Utc};
use gh_core::PrSummary;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

/// Column widths. `#N` fits 6-digit issue numbers (cli/cli is at ~13k);
/// state badge is fixed; title takes the rest of the row; author and
/// the relative-opened time are right-sized for typical content.
const COL_NUMBER: u16 = 7;
const COL_STATE: u16 = 6;
const COL_AUTHOR: u16 = 22;
const COL_OPENED: u16 = 8;

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
        Cell::from("OPENED"),
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
        Constraint::Length(COL_OPENED),
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

    // Note: GitHub's `/pulls` list endpoint does NOT return additions /
    // deletions / comment counts (those land only on the per-PR detail
    // fetch). We previously rendered `+0 -0` and `💬 0` for every row,
    // which was misleading. We replaced those columns with the relative
    // open time — which IS in the list response. A future GraphQL
    // search-based list can re-introduce the diff stats with real data.
    let opened = Cell::from(Line::from(Span::styled(
        relative_age(p.created_at),
        Style::default().fg(Color::DarkGray),
    )));

    // One blank row between PRs gives the eye a place to land. Halves
    // visible PRs in a fixed area but the user feedback was unanimous
    // that the previous tight layout was hard to scan.
    Row::new(vec![number, state, title, author, opened]).bottom_margin(1)
}

/// Compact relative duration (`3h`, `2d`, `5mo`, `1y`) suitable for a
/// short table column. Future-dated timestamps (clock skew) clamp to
/// `now`. We don't pluralise — the column is too narrow to spell it out
/// and the convention matches `gh pr list`'s output.
fn relative_age(then: DateTime<Utc>) -> String {
    let now = Utc::now();
    let dur = now.signed_duration_since(then);
    let secs = dur.num_seconds().max(0);
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;
    if mins < 1 {
        "now".to_string()
    } else if mins < 60 {
        format!("{mins}m")
    } else if hours < 24 {
        format!("{hours}h")
    } else if days < 30 {
        format!("{days}d")
    } else if days < 365 {
        format!("{}mo", days / 30)
    } else {
        format!("{}y", days / 365)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn relative_age_buckets() {
        let now = Utc::now();
        assert_eq!(relative_age(now), "now");
        assert_eq!(relative_age(now - Duration::minutes(5)), "5m");
        assert_eq!(relative_age(now - Duration::hours(3)), "3h");
        assert_eq!(relative_age(now - Duration::days(2)), "2d");
        assert_eq!(relative_age(now - Duration::days(60)), "2mo");
        assert_eq!(relative_age(now - Duration::days(800)), "2y");
    }

    #[test]
    fn relative_age_future_clamps_to_now() {
        let future = Utc::now() + Duration::hours(1);
        assert_eq!(relative_age(future), "now");
    }
}
