//! Contextual keybindings strip rendered above the status bar. Helix /
//! lazygit pattern: a single line of dim hints scoped to the active
//! screen so users can discover bindings without `?` (which lands later
//! when the TOML keymap loader does).

use gh_core::{Screen, State};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[must_use]
pub fn keybinds_bar(state: &State) -> Paragraph<'static> {
    let hints: &[(&str, &str)] = match &state.screen {
        Screen::PrList { .. } => &[("j/k", "navigate"), ("l/Enter", "open"), ("q", "quit")],
        Screen::PrDetail { .. } => &[
            ("j/k", "scroll"),
            ("gg/G", "top/bottom"),
            ("{/}", "reviews"),
            ("l/Tab", "diff"),
            ("h/Bksp", "back"),
            ("q", "quit"),
        ],
        Screen::DiffView { view_mode, .. } => match view_mode {
            gh_core::DiffViewMode::Unified => &[
                ("j/k", "scroll"),
                ("gg/G", "top/bottom"),
                ("{/}", "files"),
                ("s", "split"),
                ("h/Bksp", "back"),
                ("q", "quit"),
            ],
            gh_core::DiffViewMode::Split => &[
                ("j/k", "scroll"),
                ("gg/G", "top/bottom"),
                ("{/}", "files"),
                ("s", "unified"),
                ("h/Bksp", "back"),
                ("q", "quit"),
            ],
        },
        Screen::LoadingDetail { .. } | Screen::LoadingDiff { .. } => {
            &[("h/Bksp", "cancel"), ("q", "quit")]
        }
        Screen::Welcome | Screen::Loading { .. } | Screen::Error { .. } => &[("q", "quit")],
    };

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(Color::DarkGray);
    let sep_style = Style::default().fg(Color::DarkGray);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(hints.len() * 4);
    spans.push(Span::raw(" "));
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ·  ", sep_style));
        }
        spans.push(Span::styled((*key).to_string(), key_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*label).to_string(), label_style));
    }
    Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Rgb(20, 20, 28)))
}
