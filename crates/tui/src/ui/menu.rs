//! The menu: a plain list of options, reachable from anywhere with `m`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, MENU};
use crate::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![
        Line::from(Span::styled("readingbuddy", theme::title())),
        Line::from(""),
    ];
    for (i, (_, label, hint)) in MENU.iter().enumerate() {
        let marker = if i == app.menu_index { "› " } else { "  " };
        let style = if i == app.menu_index {
            theme::selected()
        } else {
            theme::primary()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker}{label}"), style),
            Span::styled(
                if hint.is_empty() {
                    String::new()
                } else {
                    format!("  {hint}")
                },
                theme::dim(),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("↑↓/jk", theme::key()),
        Span::styled(" move   ", theme::dim()),
        Span::styled("enter", theme::key()),
        Span::styled(" open   ", theme::dim()),
        // The one key that has to be advertised rather than looked up. It sits
        // on the menu because the menu's page is the app's introduction, and a
        // help key nobody can find is a help key nobody has.
        Span::styled("?", theme::key()),
        Span::styled(" help   ", theme::dim()),
        Span::styled("q", theme::key()),
        Span::styled(" quit", theme::dim()),
    ]));

    let width = lines
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(20)
        .saturating_add(4);
    let inner = super::centered(area, width, lines.len() as u16 + 2);
    // A `Block` styles the cells it doesn't draw but never blanks them, so
    // without this the ambient layer shows through between the menu rows.
    f.render_widget(ratatui::widgets::Clear, inner);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim());
    f.render_widget(Paragraph::new(lines).block(block), inner);
}
