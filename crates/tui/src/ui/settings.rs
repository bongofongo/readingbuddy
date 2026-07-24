//! The settings screen: a read-only view of where data lives and which glyph
//! set the object uses. The Google Books API key is set via the CLI
//! (`readingbuddy config set google-api-key`, mode-600 file); here we only show
//! whether one is present.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let cfg = &app.engine.config;
    let key = if cfg.google_api_key.is_some() {
        "set"
    } else {
        "not set (keyless, lower quota)"
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
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "set the API key with:  readingbuddy config set google-api-key",
        theme::dim(),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("enter", theme::key()),
        Span::styled(" toggle glyphs   ", theme::dim()),
        Span::styled("esc", theme::key()),
        Span::styled(" menu", theme::dim()),
    ]));

    let width = lines.iter().map(|l| l.width() as u16).max().unwrap_or(20).saturating_add(4);
    let inner = super::centered(area, width, lines.len() as u16 + 2);
    let block = Block::default().borders(Borders::ALL).border_style(theme::dim());
    f.render_widget(Paragraph::new(lines).block(block), inner);
}
