//! PR list screen: scrollable selectable list of open PRs with optional
//! "Loading more…" footer while the next page is in flight.

use gh_core::PrSummary;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

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
    // applicable; otherwise the list takes the full area.
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

    let list_items: Vec<ListItem<'_>> = items.iter().map(render_row).collect();
    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▎ ");
    let mut list_state = ListState::default();
    list_state.select(Some(selected));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

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

fn render_row(p: &PrSummary) -> ListItem<'static> {
    let number = Span::styled(
        format!("#{:<5}", p.number),
        Style::default().fg(Color::DarkGray),
    );
    let draft = if p.draft {
        Span::styled("DRAFT ", Style::default().fg(Color::Yellow))
    } else {
        Span::raw("")
    };
    let title = Span::styled(p.title.clone(), Style::default().fg(Color::White));
    let separator = Span::raw("  ");
    let by = Span::styled(
        format!("by @{}", p.author),
        Style::default().fg(Color::Cyan),
    );
    let stats = Span::styled(
        format!("  +{} -{}  💬 {}", p.additions, p.deletions, p.comments),
        Style::default().fg(Color::DarkGray),
    );
    ListItem::new(Line::from(vec![number, draft, title, separator, by, stats]))
}
