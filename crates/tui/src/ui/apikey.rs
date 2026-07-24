//! The Google Books API-key modal: a paste box, a verifying beat, then a
//! success/failure result page. Floats over the settings screen.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{ApiKeyModal, ApiKeyStage};
use crate::theme;

/// A block cursor for the entry line.
const CURSOR: &str = "▏";

pub fn render(f: &mut Frame, area: Rect, modal: &ApiKeyModal) {
    let (title, lines) = match &modal.stage {
        ApiKeyStage::Entry { value } => (" Google Books API key ", entry_lines(value)),
        ApiKeyStage::Verifying => (" Verifying… ", verifying_lines()),
        ApiKeyStage::Result { ok: true, message } => (" Key saved ", success_lines(message)),
        ApiKeyStage::Result { ok: false, message } => (" Key rejected ", failure_lines(message)),
    };

    // Wide enough for a hint line, short and centered.
    let box_area = super::centered(
        area,
        54.min(area.width.max(1)),
        (lines.len() as u16 + 2).min(area.height.max(1)),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(Span::styled(title, theme::title()));
    f.render_widget(Clear, box_area);
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
        box_area,
    );
}

fn entry_lines(value: &str) -> Vec<Line<'static>> {
    // Empty: a dim placeholder sits right on the input line and disappears the
    // moment the key starts filling it (one asterisk per character — never the
    // key itself).
    let field = if value.is_empty() {
        Line::from(vec![
            Span::styled(CURSOR, theme::accent()),
            Span::styled("Paste your key, then submit.", theme::dim()),
        ])
    } else {
        Line::from(vec![
            Span::styled("*".repeat(value.chars().count()), theme::primary()),
            Span::styled(CURSOR, theme::accent()),
        ])
    };
    // A blank line above and below keeps the input line centred: equidistant
    // from the title border and the hint row.
    vec![
        Line::from(""),
        field,
        Line::from(""),
        hint(&[("Ctrl+V", "paste"), ("Enter", "submit"), ("Esc", "cancel")]),
    ]
}

fn verifying_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            "Checking the key with Google…",
            theme::primary(),
        )),
        Line::from(""),
    ]
}

fn success_lines(message: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(format!("✓  {message}"), theme::accent())),
        Line::from(""),
        hint(&[("any key", "close")]),
    ]
}

fn failure_lines(message: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled("✗  couldn't verify the key", theme::primary())),
        Line::from(Span::styled(message.to_string(), theme::dim())),
        Line::from(""),
        hint(&[("Enter", "retry"), ("Esc", "cancel")]),
    ]
}

/// A `key label · key label` footer line.
fn hint(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (k, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("   ", theme::dim()));
        }
        spans.push(Span::styled(k.to_string(), theme::key()));
        spans.push(Span::styled(format!(" {label}"), theme::dim()));
    }
    Line::from(spans)
}
