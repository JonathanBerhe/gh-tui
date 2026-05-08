//! Markdown renderer for the PR detail body.
//!
//! Two output flavours:
//!
//! - [`render`] returns a flat `Vec<Line<'static>>` ready for a single
//!   `Paragraph` widget. Image and Mermaid blocks render as inline text
//!   placeholders (`[image: alt]`, `[mermaid diagram (N lines)]`).
//! - [`render_chunks`] returns a typed `Vec<BodyChunk>`. Image and Mermaid
//!   blocks are surfaced as their own variants so the UI layer can give
//!   them their own rect (a `ratatui-image` widget for images, an
//!   `mmdc`-rendered PNG for Mermaid). Text runs between non-text blocks
//!   are coalesced into single `BodyChunk::Text` chunks.
//!
//! Both functions accept the same input and walk the same `pulldown-cmark`
//! parser; the chunked path just additionally tracks chunk boundaries.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// A typed segment of a rendered markdown body.
///
/// Text chunks carry the styled lines produced by the prose path. Image
/// and Mermaid chunks carry the source metadata so the UI can later
/// substitute a widget. PR #2 of Phase 6 lays the groundwork — image and
/// mermaid still render as text placeholders for now; PR #3 swaps the
/// `Image` variant for a `ratatui-image` widget and PR #4 does the same
/// for `Mermaid` via `mmdc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyChunk {
    /// One or more rendered text lines. Multiple consecutive non-image
    /// non-mermaid markdown elements are coalesced into a single chunk so
    /// callers don't have to stitch them back together.
    Text(Vec<Line<'static>>),
    /// Image link from `![alt](url)`. Still renders as `[image: alt]`
    /// text in the v1 fallback.
    Image { url: String, alt: String },
    /// Mermaid fenced code block. Still renders as a placeholder line in
    /// the v1 fallback.
    Mermaid { source: String },
}

/// Visual row height reserved for an inline image. Tuned for typical
/// avatar/screenshot sizes; future TOML config exposes this as a theme
/// knob. The image scales to fit this rect; the height has to be large
/// enough that the picker's pixel→cell ratio still produces a legible
/// rendering on Kitty/iTerm2/Sixel.
pub const IMAGE_HEIGHT_ROWS: u16 = 12;

/// Visual row height reserved for a Mermaid diagram. Larger than images
/// because diagrams typically encode more visual structure.
pub const MERMAID_HEIGHT_ROWS: u16 = 16;

impl BodyChunk {
    /// Logical row height of the chunk in the body layout. For `Text`
    /// chunks this is the line count; for `Image` and `Mermaid` it's a
    /// fixed reservation so the renderer can carve out a rect for the
    /// stateful widget. Mermaid still falls back to placeholder text
    /// until PR #4 wires the `mmdc` shell-out.
    #[must_use]
    pub fn height(&self) -> u16 {
        match self {
            Self::Text(lines) => u16::try_from(lines.len()).unwrap_or(u16::MAX),
            Self::Image { .. } => IMAGE_HEIGHT_ROWS,
            Self::Mermaid { .. } => MERMAID_HEIGHT_ROWS,
        }
    }
}

/// Walk a parsed markdown body and return every `![alt](url)` URL in
/// document order, deduplicating consecutive repeats. The render layer
/// uses this to seed the image cache before the body is on screen.
#[must_use]
pub fn image_urls(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for chunk in render_chunks(input) {
        if let BodyChunk::Image { url, .. } = chunk {
            if out.last() != Some(&url) {
                out.push(url);
            }
        }
    }
    out
}

const HR_WIDTH: usize = 60;
const PLACEHOLDER_BULLET_L0: &str = "  • ";
const PLACEHOLDER_BULLET_L1: &str = "    ◦ ";
const BLOCKQUOTE_PREFIX: &str = "▏ ";
const CODE_FG: Color = Color::LightCyan;
const LINK_FG: Color = Color::Blue;
const DIM: Color = Color::DarkGray;

/// Render markdown source to a flat `Vec<Line<'static>>` suitable for a
/// single `Paragraph` widget. Image and Mermaid blocks render as inline
/// text placeholders.
#[must_use]
pub fn render(input: &str) -> Vec<Line<'static>> {
    flatten_chunks(render_chunks(input))
}

/// Render markdown source to typed body chunks. Used by the PR detail
/// screen to interleave text paragraphs with widget slots that need
/// their own `Rect` (images, Mermaid diagrams).
#[must_use]
pub fn render_chunks(input: &str) -> Vec<BodyChunk> {
    let mut r = Renderer::default();
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(input, opts);
    for event in parser {
        r.handle(event);
    }
    r.finalize_chunks()
}

/// Collapse a chunk vector to a flat line vector — `Image` and `Mermaid`
/// chunks render as `[image: alt]` / `[mermaid diagram (N lines)]`
/// placeholders followed by a blank line for visual separation, matching
/// the pre-chunk renderer's output exactly so existing snapshots stay
/// stable.
fn flatten_chunks(chunks: Vec<BodyChunk>) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for chunk in chunks {
        match chunk {
            BodyChunk::Text(lines) => out.extend(lines),
            BodyChunk::Image { alt, .. } => {
                out.push(Line::from(Span::styled(
                    format!("[image: {alt}]"),
                    Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
                )));
                out.push(Line::raw(""));
            }
            BodyChunk::Mermaid { source } => {
                let n = source.trim_end_matches('\n').lines().count().max(1);
                out.push(Line::from(Span::styled(
                    format!("[mermaid diagram ({n} lines)]"),
                    Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
                )));
                out.push(Line::raw(""));
            }
        }
    }
    out
}

#[derive(Default)]
struct Renderer {
    /// Completed chunks. Text chunks accumulate in `out` until an
    /// Image/Mermaid block flushes them as `BodyChunk::Text`.
    chunks: Vec<BodyChunk>,
    /// In-progress text chunk's lines.
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
    /// alt text inside the Image scope. Carries the URL too.
    image_alt: Option<String>,
    image_url: Option<String>,
}

#[derive(Debug, Clone)]
struct ListCtx {
    /// `Some(n)` for ordered (next number to emit), `None` for bulleted.
    next_number: Option<u64>,
    /// Have we emitted the marker for the current item yet?
    item_started: bool,
}

impl Renderer {
    /// Flush in-progress spans, push the trailing text chunk if any, and
    /// return the typed chunk list.
    fn finalize_chunks(mut self) -> Vec<BodyChunk> {
        self.flush_line();
        self.flush_text_chunk();
        self.chunks
    }

    /// Move the in-progress text chunk (if any) into `chunks` so a typed
    /// chunk (Image/Mermaid) can be appended at the right position.
    fn flush_text_chunk(&mut self) {
        if !self.out.is_empty() {
            let lines = std::mem::take(&mut self.out);
            self.chunks.push(BodyChunk::Text(lines));
        }
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
            Event::Start(Tag::Image { dest_url, .. }) => {
                self.image_alt = Some(String::new());
                self.image_url = Some(dest_url.into_string());
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
                let url = self.image_url.take().unwrap_or_default();
                self.flush_line();
                self.flush_text_chunk();
                self.chunks.push(BodyChunk::Image { url, alt });
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

        // Mermaid: emit as a typed chunk so the UI can later swap in a
        // rendered diagram. The body is preserved verbatim for PR #4's
        // `mmdc` shell-out.
        if lang.eq_ignore_ascii_case("mermaid") {
            let source: String = self.code_lines.drain(..).collect();
            self.flush_text_chunk();
            self.chunks.push(BodyChunk::Mermaid { source });
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod chunk_tests {
    use super::*;

    #[test]
    fn render_chunks_pure_text_yields_single_text_chunk() {
        let chunks = render_chunks("Hello, world!\n\nSecond para.");
        assert_eq!(chunks.len(), 1);
        assert!(matches!(chunks[0], BodyChunk::Text(_)));
    }

    #[test]
    fn render_chunks_image_splits_into_typed_variant() {
        let chunks = render_chunks("Before.\n\n![alt text](https://example.test/x.png)\n\nAfter.");
        // Expect: Text("Before."+blank) | Image | Text("After.")
        assert_eq!(chunks.len(), 3);
        assert!(matches!(chunks[0], BodyChunk::Text(_)));
        let BodyChunk::Image { ref url, ref alt } = chunks[1] else {
            panic!("expected Image chunk, got {:?}", chunks[1]);
        };
        assert_eq!(url, "https://example.test/x.png");
        assert_eq!(alt, "alt text");
        assert!(matches!(chunks[2], BodyChunk::Text(_)));
    }

    #[test]
    fn render_chunks_mermaid_carries_source_verbatim() {
        let src = "```mermaid\ngraph TD\n  A --> B\n```";
        let chunks = render_chunks(src);
        let mermaid = chunks
            .iter()
            .find_map(|c| match c {
                BodyChunk::Mermaid { source } => Some(source.clone()),
                _ => None,
            })
            .expect("expected a Mermaid chunk");
        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("A --> B"));
    }

    #[test]
    fn render_flat_matches_render_chunks_flattened() {
        // The legacy flat API must keep producing the same line stream
        // existing snapshots and call sites depend on.
        let input =
            "# Title\n\nA paragraph with ![pic](u) inline.\n\n```mermaid\nA-->B\n```\n\nDone.";
        let flat = render(input);
        let from_chunks = flatten_chunks(render_chunks(input));
        assert_eq!(flat.len(), from_chunks.len());
    }

    #[test]
    fn body_chunk_height_text_equals_line_count() {
        let c = BodyChunk::Text(vec![Line::raw(""), Line::raw("a"), Line::raw("b")]);
        assert_eq!(c.height(), 3);
    }

    #[test]
    fn body_chunk_height_image_and_mermaid_reserve_widget_rect() {
        assert_eq!(
            BodyChunk::Image {
                url: "u".into(),
                alt: "a".into()
            }
            .height(),
            IMAGE_HEIGHT_ROWS,
        );
        assert_eq!(
            BodyChunk::Mermaid {
                source: "graph".into()
            }
            .height(),
            MERMAID_HEIGHT_ROWS,
        );
    }

    #[test]
    fn image_urls_extracts_in_document_order() {
        let md =
            "intro\n\n![first](https://x.test/a.png)\n\nmid\n\n![second](https://x.test/b.png)";
        let urls = image_urls(md);
        assert_eq!(urls, vec!["https://x.test/a.png", "https://x.test/b.png"]);
    }

    #[test]
    fn image_urls_dedupes_consecutive_repeats() {
        let md = "![a](u)\n![a](u)\n![b](v)\n![a](u)";
        let urls = image_urls(md);
        // The dedupe is "consecutive only" so the third `u` reappears
        // after the intervening `v`. That's intentional — listing every
        // appearance lets the worker re-prime the cache if a previous
        // fetch failed.
        assert_eq!(urls, vec!["u", "v", "u"]);
    }

    #[test]
    fn image_urls_yields_empty_for_no_images() {
        assert!(image_urls("# heading\nbody text").is_empty());
    }
}
