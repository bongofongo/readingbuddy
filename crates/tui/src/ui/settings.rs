//! The settings screen: where data lives, the accent, the glyph set, and the
//! Google Books API key. The key can be entered here (`g`, a paste box) or via
//! the CLI (`readingbuddy config set google-api-key`); both write the same
//! mode-600 file. Only a masked form of the key is ever shown.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::render3d::Caps;
use crate::render3d::caps::Passthrough;
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
        ("ambient", app.ambient.motif.label().to_string()),
        ("renderer", format!("{:?}", app.render_mode).to_lowercase()),
        ("terminal", describe_caps(&app.caps)),
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
        Span::styled("a", theme::key()),
        Span::styled(" ambient   ", theme::dim()),
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
    // See `menu::draw`: the ambient layer would otherwise show through.
    f.render_widget(ratatui::widgets::Clear, inner);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim());
    f.render_widget(Paragraph::new(lines).block(block), inner);
}

/// One line summarising what the startup probe found, so a surprising renderer
/// choice can be diagnosed from inside the app rather than by guesswork:
/// `kitty graphics, 19x39px cells, tmux passthrough on (ours)`.
fn describe_caps(caps: &Caps) -> String {
    let mut parts = vec![if caps.kitty_graphics {
        "kitty graphics".to_string()
    } else {
        "no pixel protocol".to_string()
    }];
    let (w, h) = caps.cell_px;
    parts.push(format!(
        "{w}x{h}px cells{}",
        if caps.cell_px_measured {
            ""
        } else {
            " (assumed)"
        }
    ));
    if caps.in_tmux {
        parts.push(
            match caps.passthrough {
                Passthrough::EnabledByUs => "tmux passthrough on (ours)",
                Passthrough::Already => "tmux passthrough on",
                Passthrough::Unavailable => "tmux passthrough blocked",
                Passthrough::NotTmux => "tmux",
            }
            .to_string(),
        );
    }
    parts.join(", ")
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
