//! `ratatui` widgets and screen composition for `gh-tui`. Consumes `&State`
//! from `gh-core` and the layout helpers from `gh-render`. **Never** calls
//! `gh-api` directly — all side effects flow through `Cmd` dispatch.

pub mod images;
pub mod screens;
pub mod widgets;

pub use images::{detect_picker, ImageCache, ImageState};

use gh_core::{Screen, State};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Single entry point used by the binary's render loop. Splits the area into
/// a body region and a one-line status bar; dispatches the body to the
/// appropriate screen renderer based on `state.screen`. `images` carries
/// the shared cache so screens that embed images (PR detail, future
/// commit/file browsers) can render the decoded `StatefulProtocol` in
/// place of placeholder text.
pub fn draw(state: &State, images: &ImageCache, frame: &mut Frame<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // body
            Constraint::Length(1), // contextual keybindings
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    match &state.screen {
        Screen::Welcome => screens::welcome::draw(frame, chunks[0]),
        Screen::Loading { repo } => draw_loading(&repo.slug(), frame, chunks[0]),
        Screen::PrList {
            items,
            selected,
            loading_next,
            ..
        } => {
            screens::pr_list::draw(items, *selected, *loading_next, frame, chunks[0]);
        }
        Screen::LoadingDetail { repo, number } => {
            draw_loading(&format!("{} #{number}", repo.slug()), frame, chunks[0]);
        }
        Screen::PrDetail {
            detail,
            scroll,
            total_lines,
            ..
        } => {
            screens::pr_detail::draw(detail, *scroll, *total_lines, images, frame, chunks[0]);
        }
        Screen::LoadingDiff { repo, number } => {
            draw_loading(&format!("{} #{number} diff", repo.slug()), frame, chunks[0]);
        }
        Screen::DiffView {
            files,
            threads,
            scroll,
            total_lines,
            view_mode,
            ..
        } => {
            screens::diff_view::draw(
                files,
                threads,
                *scroll,
                *total_lines,
                *view_mode,
                frame,
                chunks[0],
            );
        }
        Screen::Error { message, hint } => {
            screens::error::draw(message, hint.as_deref(), frame, chunks[0]);
        }
    }

    frame.render_widget(widgets::keybinds::keybinds_bar(state), chunks[1]);
    frame.render_widget(widgets::status::status_bar(state), chunks[2]);
}

fn draw_loading(slug: &str, frame: &mut Frame<'_>, area: Rect) {
    let line = Line::from(Span::styled(
        format!("loading {slug}…"),
        Style::default().fg(Color::DarkGray),
    ));
    let p = Paragraph::new(vec![Line::raw(""), line]).alignment(Alignment::Center);
    frame.render_widget(p, area);
}
