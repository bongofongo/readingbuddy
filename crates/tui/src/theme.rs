//! One place for colors.
//!
//! Text sticks to `Color::Reset` and `DarkGray` so the pane inherits whatever
//! theme the terminal has; only accents are hard-coded RGB, and selection uses
//! `REVERSED` rather than a background color, which reads correctly on both
//! light and dark terminals.
//!
//! The accent is a single **runtime** color held in a process-global atom, so
//! the zero-arg [`accent`]/[`key`] helpers keep working everywhere while the
//! user retunes it from the settings screen. It packs RGB as `0x00RRGGBB`.

use std::sync::atomic::{AtomicU32, Ordering};

use ratatui::style::{Color, Modifier, Style};

/// Warm brass — the default accent (progress, keys, the active rule).
pub const DEFAULT_ACCENT: u32 = 0xC4_8B_3F;

/// A curated palette the settings screen cycles with ←/→.
pub const PRESETS: &[(&str, u32)] = &[
    ("brass", 0xC4_8B_3F),
    ("teal", 0x3F_A8_9E),
    ("rose", 0xC4_5F_7C),
    ("sky", 0x4F_8F_C4),
    ("sage", 0x7C_A8_5F),
    ("violet", 0x8B_6F_C4),
    ("amber", 0xD1_9A_2E),
];

static ACCENT: AtomicU32 = AtomicU32::new(DEFAULT_ACCENT);

/// Set the accent from packed `0xRRGGBB`.
pub fn set_accent(rgb: u32) {
    ACCENT.store(rgb & 0x00FF_FFFF, Ordering::Relaxed);
}

/// The current accent as packed `0xRRGGBB`.
pub fn accent_rgb() -> u32 {
    ACCENT.load(Ordering::Relaxed)
}

/// The current accent as a ratatui color.
pub fn accent_color() -> Color {
    let v = accent_rgb();
    Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Parse `#RRGGBB` / `RRGGBB` into packed `0xRRGGBB`.
pub fn parse_hex(s: &str) -> Option<u32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(h, 16).ok()
}

/// Render packed `0xRRGGBB` back to `#RRGGBB` for display / persistence.
pub fn to_hex(rgb: u32) -> String {
    format!("#{:06X}", rgb & 0x00FF_FFFF)
}

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
    Style::default().fg(accent_color())
}

pub fn selected() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

pub fn key() -> Style {
    Style::default().fg(accent_color()).add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_round_trips_and_rejects_garbage() {
        assert_eq!(parse_hex("#C48B3F"), Some(0xC4_8B_3F));
        assert_eq!(parse_hex("c48b3f"), Some(0xC4_8B_3F));
        assert_eq!(parse_hex("  #ffffff "), Some(0xFF_FF_FF));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(parse_hex("orange"), None);
        assert_eq!(to_hex(0xC4_8B_3F), "#C48B3F");
    }

    #[test]
    fn set_accent_is_reflected_by_the_getters() {
        let saved = accent_rgb();
        set_accent(0x12_34_56);
        assert_eq!(accent_rgb(), 0x12_34_56);
        assert_eq!(accent_color(), Color::Rgb(0x12, 0x34, 0x56));
        set_accent(saved);
    }
}
