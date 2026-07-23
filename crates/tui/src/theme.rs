//! One place for colors.
//!
//! Text sticks to `Color::Reset` and `DarkGray` so the pane inherits whatever
//! theme the terminal has; only accents are hard-coded RGB, and selection uses
//! `REVERSED` rather than a background color, which reads correctly on both
//! light and dark terminals.

use ratatui::style::{Color, Modifier, Style};

/// Warm brass — progress bars, keys, the active rule.
pub const ACCENT: Color = Color::Rgb(0xC4, 0x8B, 0x3F);
pub fn primary() -> Style {
    Style::default().fg(Color::Reset)
}

pub fn title() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn selected() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

pub fn key() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
