//! The settings screen: where data lives, the accent, the glyph set, and the
//! Google Books API key. The key can be entered here (`g`, a paste box) or via
//! the CLI (`readingbuddy config set google-api-key`); both write the same
//! mode-600 file. Only a masked form of the key is ever shown.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let cfg = &app.engine.config;
    let key = match cfg.google_api_key.as_deref() {
        Some(k) => format!("set  {}", mask(k)),
        None => "not set (keyless, lower quota)".to_string(),
    };
    let rows = [
        ("database", cfg.db_url.clone()),
        ("images", cfg.images_dir.display().to_string()),
        ("vault", cfg.vault_dir.display().to_string()),
        ("google key", key.to_string()),
        ("glyph set", format!("{:?}", app.params.glyphs)),
    ];

    let mut lines = vec![
        Line::from(Span::styled("settings", theme::title())),
        Line::from(""),
    ];
    for (label, value) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("{label:<12}"), theme::dim()),
            Span::styled(value, theme::primary()),
        ]));
    }
    // Accent gets a live swatch beside its hex.
    lines.push(Line::from(vec![
        Span::styled(format!("{:<12}", "accent"), theme::dim()),
        Span::styled("███", theme::accent()),
        Span::styled(
            format!(" {}", theme::to_hex(theme::accent_rgb())),
            theme::primary(),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("← →", theme::key()),
        Span::styled(" accent   ", theme::dim()),
        Span::styled("/", theme::key()),
        Span::styled(" hex   ", theme::dim()),
        Span::styled("enter", theme::key()),
        Span::styled(" glyphs   ", theme::dim()),
        Span::styled("g", theme::key()),
        Span::styled(" api key   ", theme::dim()),
        Span::styled("esc", theme::key()),
        Span::styled(" menu", theme::dim()),
    ]));

    let width = lines
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(20)
        .saturating_add(4);
    let inner = super::centered(area, width, lines.len() as u16 + 2);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim());
    f.render_widget(Paragraph::new(lines).block(block), inner);
}

/// Show a secret without revealing it: `AIza…f3Qk` (first/last 4 chars),
/// mirroring the CLI's `config_file::mask`.
fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 8 {
        "*".repeat(chars.len().max(4))
    } else {
        format!(
            "{}…{}",
            chars[..4].iter().collect::<String>(),
            chars[chars.len() - 4..].iter().collect::<String>()
        )
    }
}
