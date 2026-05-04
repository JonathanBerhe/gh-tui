//! PR detail screen: title, meta line (state, branches, stats, mergeability,
//! checks summary, review decision), markdown body (scrollable), reviews list.

use gh_core::{
    ChecksState, ChecksSummary, Mergeable, PrDetail, PrState, ReviewDecision, ReviewState,
    ReviewSummary,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

pub fn draw(detail: &PrDetail, scroll: u16, total_lines: u16, frame: &mut Frame<'_>, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(1), // meta
            Constraint::Length(1), // blank — visual breath above the rule
            Constraint::Length(1), // separator
            Constraint::Length(1), // blank — visual breath below the rule
            Constraint::Min(1),    // body + reviews (scrollable together)
        ])
        .split(area);

    frame.render_widget(title_line(detail), chunks[0]);
    frame.render_widget(meta_line(detail), chunks[1]);
    frame.render_widget(separator(), chunks[3]);

    // Body sits in a horizontal split: the paragraph fills all but the
    // last column, which carries a vertical scrollbar tracking the
    // current scroll position vs. total content length.
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chunks[5]);
    frame.render_widget(body_and_reviews(detail, scroll), body_chunks[0]);
    render_scrollbar(scroll, total_lines, body_chunks[1], frame);
}

/// Vertical scrollbar on the right edge. `position` is the current scroll
/// (top-line index); `content_length` is the rendered total. Hidden when
/// content fits in viewport (length 0/1).
pub(super) fn render_scrollbar(scroll: u16, total_lines: u16, area: Rect, frame: &mut Frame<'_>) {
    if total_lines <= 1 {
        return;
    }
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("▲"))
        .end_symbol(Some("▼"))
        .track_symbol(Some("│"))
        .thumb_symbol("█")
        .style(Style::default().fg(Color::DarkGray))
        .thumb_style(Style::default().fg(Color::Cyan));
    let mut state = ScrollbarState::new(usize::from(total_lines)).position(usize::from(scroll));
    frame.render_stateful_widget(scrollbar, area, &mut state);
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

    if !d.checks.is_empty() {
        spans.push(sep());
        spans.push(checks_span(&d.checks));
    }

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

fn checks_span(c: &ChecksSummary) -> Span<'static> {
    let (glyph, colour) = match c.state {
        ChecksState::Success => ("✓", Color::Green),
        ChecksState::Failure => ("✗", Color::Red),
        ChecksState::Pending => ("⏳", Color::Yellow),
        ChecksState::Unknown => ("·", Color::DarkGray),
    };
    let total = c.total();
    Span::styled(
        format!("{glyph} checks ({}/{total})", c.passing),
        Style::default().fg(colour),
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

fn body_and_reviews(d: &PrDetail, scroll: u16) -> Paragraph<'static> {
    let mut lines: Vec<Line<'static>> = if d.body.trim().is_empty() {
        vec![Line::from(Span::styled(
            "(no description)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        gh_render::render_markdown(&d.body)
    };

    if !d.reviews.is_empty() {
        // Breathe between markdown body and the reviews block.
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "─".repeat(60),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "Reviews",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));

        for r in &d.reviews {
            lines.push(review_header(r));
            lines.push(review_excerpt(r));
            lines.push(Line::raw(""));
        }
    }

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::NONE))
}

fn review_header(r: &ReviewSummary) -> Line<'static> {
    let author = Span::styled(
        format!("@{:<14}", r.author),
        Style::default().fg(Color::Cyan),
    );
    let (state_text, state_colour) = match r.state {
        ReviewState::Approved => ("APPROVED", Color::Green),
        ReviewState::ChangesRequested => ("CHANGES_REQUESTED", Color::Red),
        ReviewState::Commented => ("COMMENTED", Color::Gray),
        ReviewState::Dismissed => ("DISMISSED", Color::DarkGray),
        ReviewState::Pending => ("PENDING", Color::Yellow),
    };
    let state = Span::styled(format!(" {state_text}"), Style::default().fg(state_colour));
    Line::from(vec![author, state])
}

fn review_excerpt(r: &ReviewSummary) -> Line<'static> {
    if r.body_excerpt.is_empty() {
        Line::from(Span::styled(
            "  (no comment)",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(
            format!("  {}", r.body_excerpt),
            Style::default().fg(Color::Gray),
        ))
    }
}
