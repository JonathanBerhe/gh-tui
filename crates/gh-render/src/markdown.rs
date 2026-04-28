//! Markdown → `Vec<Line<'static>>` renderer for the PR detail body.
//!
//! Handles the subset GitHub PR descriptions actually use: paragraphs,
//! headings, bold/italic/strikethrough, inline code, fenced code blocks,
//! lists (bulleted + ordered, two-deep), blockquotes, links, images, and
//! horizontal rules.
//!
//! Image and Mermaid blocks are replaced by short text placeholders;
//! Phase 6 will hoist them as separate widget placements.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

const HR_WIDTH: usize = 60;
const PLACEHOLDER_BULLET_L0: &str = "  • ";
const PLACEHOLDER_BULLET_L1: &str = "    ◦ ";
const BLOCKQUOTE_PREFIX: &str = "▏ ";
const CODE_FG: Color = Color::LightCyan;
const LINK_FG: Color = Color::Blue;
const DIM: Color = Color::DarkGray;

/// Render markdown source to a vector of styled lines suitable for
/// `ratatui::widgets::Paragraph`. Wrap is applied at render time by the
/// caller; `scroll` is in logical-line units.
#[must_use]
pub fn render(input: &str) -> Vec<Line<'static>> {
    let mut r = Renderer::default();
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(input, opts);
    for event in parser {
        r.handle(event);
    }
    r.finalize()
}

#[derive(Default)]
struct Renderer {
    out: Vec<Line<'static>>,
    cur: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    list_stack: Vec<ListCtx>,
    /// Some(language) while inside a fenced code block.
    in_code: Option<String>,
    /// Lines collected for the current code block (so we can detect the
    /// `mermaid` placeholder swap).
    code_lines: Vec<String>,
    in_blockquote: bool,
    /// Capture state for a Link tag — captures text spans separately so we
    /// can append " (url)" after the closing tag.
    link_url: Option<String>,
    link_spans: Vec<Span<'static>>,
    /// Capture state for an Image tag — pulldown emits text events with the
    /// alt text inside the Image scope.
    image_alt: Option<String>,
}

#[derive(Debug, Clone)]
struct ListCtx {
    /// `Some(n)` for ordered (next number to emit), `None` for bulleted.
    next_number: Option<u64>,
    /// Have we emitted the marker for the current item yet?
    item_started: bool,
}

impl Renderer {
    fn finalize(mut self) -> Vec<Line<'static>> {
        self.flush_line();
        self.out
    }

    fn handle(&mut self, ev: Event<'_>) {
        if let Some(_lang) = self.in_code.as_ref() {
            // Inside a code block, only Text and End events matter.
            match ev {
                Event::Text(t) => self.code_lines.push(t.into_string()),
                Event::End(TagEnd::CodeBlock) => self.close_code_block(),
                _ => {}
            }
            return;
        }

        match ev {
            // ── block-level starts ────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush_line();
                if !self.out.is_empty() {
                    self.out.push(Line::raw(""));
                }
                let prefix = "#".repeat(heading_level(level));
                self.cur.push(Span::styled(
                    format!("{prefix} "),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ));
                self.style_stack
                    .push(Style::default().add_modifier(Modifier::BOLD));
            }
            Event::Start(Tag::Paragraph) => {
                // No-op; paragraph contents append to the current line.
            }
            Event::Start(Tag::BlockQuote(_)) => {
                self.flush_line();
                self.in_blockquote = true;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                self.flush_line();
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) => s.into_string(),
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                };
                self.in_code = Some(lang);
                self.code_lines.clear();
            }
            Event::Start(Tag::List(start)) => {
                self.flush_line();
                self.list_stack.push(ListCtx {
                    next_number: start,
                    item_started: false,
                });
            }
            Event::Start(Tag::Item) => {
                self.flush_line();
                let depth = self.list_stack.len().saturating_sub(1);
                if let Some(ctx) = self.list_stack.last_mut() {
                    ctx.item_started = true;
                    let marker = match ctx.next_number {
                        Some(n) => {
                            ctx.next_number = Some(n + 1);
                            format!("  {n}. ")
                        }
                        None => if depth == 0 {
                            PLACEHOLDER_BULLET_L0
                        } else {
                            PLACEHOLDER_BULLET_L1
                        }
                        .to_string(),
                    };
                    self.cur
                        .push(Span::styled(marker, Style::default().fg(DIM)));
                }
            }
            Event::Start(Tag::Emphasis) => {
                self.style_stack
                    .push(Style::default().add_modifier(Modifier::ITALIC));
            }
            Event::Start(Tag::Strong) => {
                self.style_stack
                    .push(Style::default().add_modifier(Modifier::BOLD));
            }
            Event::Start(Tag::Strikethrough) => {
                self.style_stack
                    .push(Style::default().add_modifier(Modifier::CROSSED_OUT));
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.link_url = Some(dest_url.into_string());
                self.link_spans = std::mem::take(&mut self.cur);
                // Push a marker style; actual styling applied when we close.
            }
            Event::Start(Tag::Image { dest_url: _, .. }) => {
                self.image_alt = Some(String::new());
            }

            // ── block-level ends ──────────────────────────────────────
            Event::End(TagEnd::Heading(_)) => {
                self.style_stack.pop();
                self.flush_line();
            }
            Event::End(TagEnd::Paragraph) => {
                self.flush_line();
                // Blank separator after a paragraph (unless followed by a
                // list item which the renderer hugs).
                if !self.out.is_empty() && !in_list(&self.list_stack) {
                    self.out.push(Line::raw(""));
                }
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.flush_line();
                self.in_blockquote = false;
                if !self.out.is_empty() {
                    self.out.push(Line::raw(""));
                }
            }
            Event::End(TagEnd::List(_)) => {
                self.flush_line();
                self.list_stack.pop();
                if self.list_stack.is_empty() && !self.out.is_empty() {
                    self.out.push(Line::raw(""));
                }
            }
            Event::End(TagEnd::Item) => {
                self.flush_line();
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {
                self.style_stack.pop();
            }
            Event::End(TagEnd::Link) => {
                let url = self.link_url.take().unwrap_or_default();
                let collected = std::mem::take(&mut self.cur);
                let mut prior = std::mem::take(&mut self.link_spans);
                // Style the captured text as a link.
                for sp in collected {
                    let styled = Span::styled(
                        sp.content,
                        sp.style.fg(LINK_FG).add_modifier(Modifier::UNDERLINED),
                    );
                    prior.push(styled);
                }
                // Append " (url)" in dim.
                if !url.is_empty() {
                    prior.push(Span::styled(format!(" ({url})"), Style::default().fg(DIM)));
                }
                self.cur = prior;
            }
            Event::End(TagEnd::Image) => {
                let alt = self.image_alt.take().unwrap_or_default();
                self.flush_line();
                self.out.push(Line::from(Span::styled(
                    format!("[image: {alt}]"),
                    Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
                )));
            }

            // ── inline / text ─────────────────────────────────────────
            Event::Text(t) => {
                if let Some(buf) = self.image_alt.as_mut() {
                    buf.push_str(&t);
                    return;
                }
                let style = self.current_style();
                self.cur.push(Span::styled(t.into_string(), style));
            }
            Event::Code(t) => {
                self.cur
                    .push(Span::styled(t.into_string(), Style::default().fg(CODE_FG)));
            }
            Event::SoftBreak => {
                let style = self.current_style();
                self.cur.push(Span::styled(" ", style));
            }
            Event::HardBreak => {
                self.flush_line();
            }
            Event::Rule => {
                self.flush_line();
                self.out.push(Line::from(Span::styled(
                    "─".repeat(HR_WIDTH),
                    Style::default().fg(DIM),
                )));
                self.out.push(Line::raw(""));
            }
            Event::TaskListMarker(checked) => {
                let glyph = if checked { "[x] " } else { "[ ] " };
                self.cur.push(Span::styled(
                    glyph,
                    Style::default().fg(if checked { Color::Green } else { DIM }),
                ));
            }

            // Skip everything else (HTML, footnotes, math, definitions, etc.)
            _ => {}
        }
    }

    fn current_style(&self) -> Style {
        let mut style = Style::default();
        for layer in &self.style_stack {
            style = style.patch(*layer);
        }
        style
    }

    fn flush_line(&mut self) {
        if self.cur.is_empty() {
            return;
        }
        let mut spans = std::mem::take(&mut self.cur);
        if self.in_blockquote {
            spans.insert(0, Span::styled(BLOCKQUOTE_PREFIX, Style::default().fg(DIM)));
        }
        self.out.push(Line::from(spans));
    }

    fn close_code_block(&mut self) {
        let lang = self.in_code.take().unwrap_or_default();

        // Mermaid placeholder: don't render the code, leave a hint.
        if lang.eq_ignore_ascii_case("mermaid") {
            // pulldown-cmark concatenates code-block contents into a single
            // Text event; count newlines for the (N lines) hint.
            let body: String = self.code_lines.drain(..).collect();
            let n = body.trim_end_matches('\n').lines().count().max(1);
            self.out.push(Line::from(Span::styled(
                format!("[mermaid diagram ({n} lines)]"),
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            )));
            self.out.push(Line::raw(""));
            return;
        }

        if !lang.is_empty() {
            self.out.push(Line::from(Span::styled(
                format!("```{lang}"),
                Style::default().fg(DIM),
            )));
        }
        for line in self.code_lines.drain(..) {
            // pulldown emits each source line as its own Text event, often
            // with a trailing newline. Strip it.
            let text = line.trim_end_matches('\n').to_string();
            self.out
                .push(Line::from(Span::styled(text, Style::default().fg(CODE_FG))));
        }
        if !lang.is_empty() {
            self.out.push(Line::from(Span::styled(
                "```".to_string(),
                Style::default().fg(DIM),
            )));
        }
        self.out.push(Line::raw(""));
    }
}

const fn heading_level(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn in_list(stack: &[ListCtx]) -> bool {
    stack.last().is_some_and(|c| c.item_started)
}
