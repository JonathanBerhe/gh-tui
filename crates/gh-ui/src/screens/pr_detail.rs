//! PR detail screen: title, meta line (state, branches, stats, mergeability,
//! checks summary, review decision), markdown body (scrollable), reviews list.

use gh_core::{
    ChecksState, ChecksSummary, Mergeable, PrDetail, PrState, ReviewDecision, ReviewState,
    ReviewSummary,
};
use gh_render::BodyChunk;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};
use ratatui_image::StatefulImage;

use crate::images::{ImageCache, ImageState};

pub fn draw(
    detail: &PrDetail,
    scroll: u16,
    total_lines: u16,
    images: &ImageCache,
    frame: &mut Frame<'_>,
    area: Rect,
) {
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

    // Body sits in a horizontal split: the chunk stack fills all but the
    // last column, which carries a vertical scrollbar tracking the
    // current scroll position vs. total content length.
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chunks[5]);
    let body = build_body_chunks(detail);
    render_body_stack(&body, scroll, images, frame, body_chunks[0]);
    render_scrollbar(scroll, total_lines, body_chunks[1], frame);
}

/// Build the full body chunk sequence: markdown chunks (text, image,
/// mermaid) followed by the optional reviews block as a trailing text
/// chunk. The output is what the chunk-stack renderer walks; image and
/// mermaid still render as placeholder text in v1 (PR #3 / PR #4 swap
/// them for real widgets).
fn build_body_chunks(detail: &PrDetail) -> Vec<BodyChunk> {
    let mut chunks = if detail.body.trim().is_empty() {
        vec![BodyChunk::Text(vec![Line::from(Span::styled(
            "(no description)",
            Style::default().fg(Color::DarkGray),
        ))])]
    } else {
        gh_render::render_markdown_chunks(&detail.body)
    };

    if !detail.reviews.is_empty() {
        let mut review_lines: Vec<Line<'static>> = vec![
            Line::raw(""),
            Line::from(Span::styled(
                "─".repeat(60),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "Reviews",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];
        for r in &detail.reviews {
            review_lines.push(review_header(r));
            review_lines.push(review_excerpt(r));
            review_lines.push(Line::raw(""));
        }
        chunks.push(BodyChunk::Text(review_lines));
    }

    chunks
}

/// Vertical chunk stack with a single shared scroll value.
///
/// Walks the chunks accumulating logical heights; finds the first chunk
/// straddling `scroll`, then allocates rects bottom-up until the viewport
/// fills. Each chunk is rendered into its own rect — text via `Paragraph`,
/// image / mermaid as placeholder text in this PR (PR #3 / PR #4 swap them
/// for actual widgets).
fn render_body_stack(
    body: &[BodyChunk],
    scroll: u16,
    images: &ImageCache,
    frame: &mut Frame<'_>,
    area: Rect,
) {
    if area.height == 0 {
        return;
    }

    // Find the first chunk that contains `scroll` and the offset within it.
    let scroll_u32 = u32::from(scroll);
    let mut acc: u32 = 0;
    let mut first_visible = 0usize;
    let mut offset_in_first: u16 = 0;
    for (i, chunk) in body.iter().enumerate() {
        let h = u32::from(chunk.height());
        if acc + h > scroll_u32 {
            first_visible = i;
            offset_in_first = u16::try_from(scroll_u32 - acc).unwrap_or(u16::MAX);
            break;
        }
        acc += h;
        first_visible = i + 1;
    }

    // Walk visible chunks, allocating sub-rects along the y-axis.
    let mut y: u16 = 0;
    let mut idx = first_visible;
    while idx < body.len() && y < area.height {
        let chunk = &body[idx];
        let chunk_h = chunk.height();
        let skip = if idx == first_visible {
            offset_in_first
        } else {
            0
        };
        let avail = area.height - y;
        let visible = chunk_h.saturating_sub(skip).min(avail);
        if visible == 0 {
            break;
        }
        let rect = Rect {
            x: area.x,
            y: area.y + y,
            width: area.width,
            height: visible,
        };
        match chunk {
            BodyChunk::Text(lines) => {
                let p = Paragraph::new(lines.clone())
                    .wrap(Wrap { trim: false })
                    .scroll((skip, 0))
                    .block(Block::default().borders(Borders::NONE));
                frame.render_widget(p, rect);
            }
            BodyChunk::Image { url, alt } => {
                render_image_chunk(images, url, alt, rect, skip, frame);
            }
            BodyChunk::Mermaid { source } => {
                render_mermaid_chunk(images, source, rect, skip, frame);
            }
        }
        y += visible;
        idx += 1;
    }
}

/// One Image chunk: render the decoded `StatefulProtocol` when present,
/// otherwise fall through to placeholder text. Keeps the matching
/// `BodyChunk::Image` arm small and pulls the cache-locking logic out
/// of the chunk walk.
fn render_image_chunk(
    images: &ImageCache,
    url: &str,
    alt: &str,
    rect: Rect,
    skip: u16,
    frame: &mut Frame<'_>,
) {
    // Hold the lock just long enough to either render the widget (which
    // needs `&mut StatefulProtocol`) or yield the placeholder text. The
    // closure runs to completion before `with_state` returns, so the
    // mutex isn't held across any further `frame` calls.
    enum Outcome {
        Rendered,
        Placeholder(String),
    }
    let outcome = images
        .with_state(url, |state| match state {
            ImageState::Ready(protocol) => {
                // `protocol` is `&mut Box<StatefulProtocol>`; deref so
                // `StatefulImage`'s `ResizeEncodeRender` bound matches.
                frame.render_stateful_widget(StatefulImage::default(), rect, protocol.as_mut());
                Outcome::Rendered
            }
            ImageState::Loading => Outcome::Placeholder(format!("[image: {alt} — loading…]")),
            ImageState::Failed(reason) => {
                Outcome::Placeholder(format!("[image: {alt} — {reason}]"))
            }
        })
        .unwrap_or_else(|| Outcome::Placeholder(format!("[image: {alt}]")));

    let Outcome::Placeholder(label) = outcome else {
        return;
    };
    let p = Paragraph::new(Line::from(Span::styled(
        label,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )))
    .scroll((skip, 0));
    frame.render_widget(p, rect);
}

/// One Mermaid chunk: hash the source the same way the worker does,
/// then mirror the image-chunk render path against the shared cache.
/// PNGs from `mmdc` decode into the same `StatefulProtocol` that backs
/// inline images, so they render through the same `StatefulImage`
/// widget. Fallback text differs slightly so reviewers can tell *why*
/// they're seeing the placeholder (mmdc missing vs render failure vs
/// still rendering).
fn render_mermaid_chunk(
    images: &ImageCache,
    source: &str,
    rect: Rect,
    skip: u16,
    frame: &mut Frame<'_>,
) {
    let hash = gh_render::mermaid_hash(source);
    let n = source.lines().count().max(1);
    enum Outcome {
        Rendered,
        Placeholder(String),
    }
    let outcome = images
        .with_state(&hash, |state| match state {
            ImageState::Ready(protocol) => {
                frame.render_stateful_widget(StatefulImage::default(), rect, protocol.as_mut());
                Outcome::Rendered
            }
            ImageState::Loading => {
                Outcome::Placeholder(format!("[mermaid diagram ({n} lines) — rendering…]"))
            }
            ImageState::Failed(reason) => {
                Outcome::Placeholder(format!("[mermaid diagram ({n} lines) — {reason}]"))
            }
        })
        .unwrap_or_else(|| Outcome::Placeholder(format!("[mermaid diagram ({n} lines)]")));

    let Outcome::Placeholder(label) = outcome else {
        return;
    };
    let p = Paragraph::new(Line::from(Span::styled(
        label,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )))
    .scroll((skip, 0));
    frame.render_widget(p, rect);
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
