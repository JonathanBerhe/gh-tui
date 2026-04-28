//! PR detail screen (Phase 4 PR #1: raw body, no markdown rendering yet).
//!
//! Layout:
//! ```text
//! #N  title
//! state badge  •  alice wants to merge feat/foo into main  •  +A -D  •  REVIEW_DECISION
//! ────────────────────────────────────────────────────────────
//! <body, wrapped, scrollable>
//! ```

use gh_core::{Mergeable, PrDetail, PrState, ReviewDecision};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw(detail: &PrDetail, scroll: u16, frame: &mut Frame<'_>, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(1), // meta
            Constraint::Length(1), // separator
            Constraint::Min(1),    // body
        ])
        .split(area);

    frame.render_widget(title_line(detail), chunks[0]);
    frame.render_widget(meta_line(detail), chunks[1]);
    frame.render_widget(separator(), chunks[2]);
    frame.render_widget(body(detail, scroll), chunks[3]);
}

fn title_line(d: &PrDetail) -> Paragraph<'static> {
    let number = Span::styled(
        format!("#{}  ", d.number),
        Style::default().fg(Color::DarkGray),
    );
    let title = Span::styled(
        d.title.clone(),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    Paragraph::new(Line::from(vec![number, title]))
}

fn meta_line(d: &PrDetail) -> Paragraph<'static> {
    let state = state_badge(d.state, d.draft);
    let sep = || Span::styled("  •  ", Style::default().fg(Color::DarkGray));
    let branches = Span::styled(
        format!(
            "{} wants to merge {} into {}",
            d.author, d.head_ref, d.base_ref
        ),
        Style::default().fg(Color::Gray),
    );
    let stats = Span::styled(
        format!("+{} -{}", d.additions, d.deletions),
        Style::default().fg(Color::DarkGray),
    );
    let mergeable = Span::styled(
        match d.mergeable {
            Mergeable::Yes => "mergeable",
            Mergeable::No => "conflicts",
            Mergeable::Unknown => "checking…",
        },
        Style::default().fg(match d.mergeable {
            Mergeable::Yes => Color::Green,
            Mergeable::No => Color::Red,
            Mergeable::Unknown => Color::DarkGray,
        }),
    );
    let mut spans = vec![state, sep(), branches, sep(), stats, sep(), mergeable];

    if d.review_decision != ReviewDecision::None {
        spans.push(sep());
        spans.push(review_decision_span(d.review_decision));
    }

    Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false })
}

fn state_badge(state: PrState, draft: bool) -> Span<'static> {
    if draft {
        return Span::styled(
            " DRAFT ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    let (label, bg) = match state {
        PrState::Open => (" OPEN ", Color::Green),
        PrState::Closed => (" CLOSED ", Color::Red),
        PrState::Merged => (" MERGED ", Color::Magenta),
    };
    Span::styled(
        label,
        Style::default()
            .fg(Color::Black)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )
}

fn review_decision_span(decision: ReviewDecision) -> Span<'static> {
    let (text, colour) = match decision {
        ReviewDecision::Approved => ("✓ APPROVED", Color::Green),
        ReviewDecision::ChangesRequested => ("✗ CHANGES_REQUESTED", Color::Red),
        ReviewDecision::ReviewRequired => ("⏳ REVIEW_REQUIRED", Color::Yellow),
        ReviewDecision::None => ("", Color::DarkGray),
    };
    Span::styled(text, Style::default().fg(colour))
}

fn separator() -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(
        "─".repeat(120),
        Style::default().fg(Color::DarkGray),
    )))
}

fn body(d: &PrDetail, scroll: u16) -> Paragraph<'static> {
    let lines = if d.body.trim().is_empty() {
        vec![Line::from(Span::styled(
            "(no description)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        gh_render::render_markdown(&d.body)
    };
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::NONE))
}
