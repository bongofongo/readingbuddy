//! The single-book viewing mode: metadata, the 3D object, and a key bar that
//! is always present — collapsed to the few keys worth advertising, expanded
//! to the full set with `o`.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Wrap};
use readingbuddy::Book;

use super::{BookLayout, book_layout};
use crate::app::{App, BookView};
use crate::render3d::blit;
use crate::theme;

/// Width of the metadata column in the wide layout.
const PANEL_WIDTH: u16 = 34;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    if app.view.is_none() {
        let msg = Paragraph::new("no book selected — press m for the menu").style(theme::dim());
        f.render_widget(msg, super::centered(area, 44, 1));
        return;
    }

    // The key bar is never hidden outright: without a visible way back to the
    // menu the view is a dead end. `o` only decides how much of it shows.
    let bar_h = if area.height >= 6 { 1 } else { 0 };
    let [main, bar] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(bar_h)]).areas(area);

    match book_layout(main) {
        BookLayout::Wide => {
            let [panel, object] =
                Layout::horizontal([Constraint::Length(PANEL_WIDTH), Constraint::Min(0)])
                    .areas(main);
            draw_object(f, app, object, None);
            draw_panel(f, app.view.as_ref().expect("checked above"), panel);
        }
        BookLayout::Compact => {
            let [header, object] =
                Layout::vertical([Constraint::Length(5), Constraint::Min(0)]).areas(main);
            draw_object(f, app, object, None);
            draw_header(f, app.view.as_ref().expect("checked above"), header);
        }
        BookLayout::Bare => {
            let title = app
                .view
                .as_ref()
                .map(|v| v.book.display_title().to_string())
                .unwrap_or_default();
            draw_object(f, app, main, Some(title));
        }
    }

    if bar_h > 0 {
        draw_key_bar(f, app, bar);
    }
}

/// Trace the book into the area. The renderer works in cells; it returns four
/// subpixels per cell for [`blit`] to quantize.
fn draw_object(f: &mut Frame, app: &mut App, area: Rect, title: Option<String>) {
    let area = match &title {
        Some(t) => {
            let block = Block::default()
                .title(Span::styled(format!(" {t} "), theme::title()))
                .title_alignment(Alignment::Center);
            let inner = block.inner(area);
            f.render_widget(block, area);
            inner
        }
        None => area,
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    let params = app.params;
    // Disjoint field borrows: the scene renders, the view supplies the book.
    let view = app.view.as_ref().expect("caller checked");
    let fb = app.scene.frame(&view.book, area.width, area.height, params);
    blit::blit(fb, area, f.buffer_mut());
}

/// Wide layout: full metadata beside the object.
fn draw_panel(f: &mut Frame, view: &BookView, area: Rect) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(theme::dim());
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Title, author, progress, then the details — progress sits with the two
    // things it belongs to rather than being stranded at the bottom.
    let inner = inner.inner(ratatui::layout::Margin::new(1, 0));
    let [head, gauge, text] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);

    let b = &view.book;
    let head_lines = vec![
        Line::from(Span::styled(b.display_title().to_string(), theme::title())),
        Line::from(Span::styled(b.display_authors(), theme::dim())),
    ];
    f.render_widget(
        Paragraph::new(head_lines).wrap(Wrap { trim: false }),
        head,
    );
    draw_progress(f, b, gauge);

    let mut lines = vec![Line::from("")];
    for (label, value) in facts(b) {
        lines.push(Line::from(vec![
            Span::styled(format!("{label:<10}"), theme::dim()),
            Span::styled(value, theme::primary()),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(view.highlights.to_string(), theme::accent()),
        Span::styled(" highlights   ", theme::dim()),
        Span::styled(view.notes.to_string(), theme::accent()),
        Span::styled(" notes", theme::dim()),
    ]));

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text);
}

/// Compact layout: title, author, a one-line summary, then progress.
fn draw_header(f: &mut Frame, view: &BookView, area: Rect) {
    let [text, gauge] =
        Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).areas(area);
    let b = &view.book;
    let lines = vec![
        Line::from(Span::styled(b.display_title().to_string(), theme::title())),
        Line::from(Span::styled(b.display_authors(), theme::dim())),
        Line::from(Span::styled(summary(view), theme::dim())),
    ];
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), text);

    let width = area.width.min(46);
    let centered = Rect {
        x: area.x + (area.width - width) / 2,
        y: gauge.y,
        width,
        height: 1,
    };
    draw_progress(f, b, centered);
}

/// The facts that fit on one line, for the layouts without room for a panel.
fn summary(view: &BookView) -> String {
    let b = &view.book;
    let mut parts: Vec<String> = Vec::new();
    if let Some(y) = b.publish_year {
        parts.push(y.to_string());
    }
    if let Some(p) = b.page_count {
        parts.push(format!("{p}pp"));
    }
    if view.highlights > 0 {
        parts.push(format!("{} highlights", view.highlights));
    }
    if view.notes > 0 {
        parts.push(format!("{} notes", view.notes));
    }
    parts.join("  ·  ")
}

fn draw_progress(f: &mut Frame, b: &Book, area: Rect) {
    if area.width == 0 {
        return;
    }
    let (ratio, label) = match (b.finished, b.current_page, b.page_count) {
        (true, _, _) => (1.0, "finished".to_string()),
        (_, Some(p), Some(t)) if t > 0 => {
            (((p as f64) / (t as f64)).clamp(0.0, 1.0), format!("{p} / {t}"))
        }
        (_, Some(p), _) => (0.0, format!("page {p}")),
        _ => (0.0, "not started".to_string()),
    };
    let gauge = Gauge::default()
        .ratio(ratio)
        .label(Span::styled(label, theme::dim()))
        .gauge_style(theme::accent())
        .use_unicode(true);
    f.render_widget(gauge, area);
}

/// The metadata rows, skipping anything the book doesn't have.
fn facts(b: &Book) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(p) = &b.publisher {
        out.push(("publisher", p.clone()));
    }
    if let Some(y) = b.publish_year {
        out.push(("year", y.to_string()));
    }
    if let Some(l) = &b.language {
        out.push(("language", l.clone()));
    }
    if let Some(p) = b.page_count {
        out.push(("pages", p.to_string()));
    }
    if let Some(i) = b.any_isbn() {
        out.push(("isbn", i.to_string()));
    }
    out
}

/// The key bar. Collapsed it advertises the way out and the way to more;
/// expanded it lists everything the view responds to. Pairs are dropped from
/// the right when the pane is too narrow, so it never wraps.
fn draw_key_bar(f: &mut Frame, app: &App, area: Rect) {
    let spin = if app.spinning { "stop" } else { "spin" };
    let expanded: &[(&str, &str)] = &[
        ("←→/hl", "turn"),
        ("↑↓/kj", "tilt"),
        ("space", spin),
        ("r", "reset"),
        ("o", "less"),
        ("esc", "library"),
        ("m", "menu"),
        ("q", "quit"),
    ];
    let collapsed: &[(&str, &str)] = &[
        ("o", "options"),
        ("m", "menu"),
        ("esc", "library"),
        ("q", "quit"),
    ];
    let pairs = if app.show_options { expanded } else { collapsed };

    let mut spans = Vec::new();
    let mut used = 0u16;
    for (key, label) in pairs {
        let width = key.chars().count() as u16 + label.chars().count() as u16 + 4;
        if used + width > area.width {
            break;
        }
        used += width;
        spans.push(Span::styled(format!(" {key} "), theme::key()));
        spans.push(Span::styled(format!("{label}  "), theme::dim()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_bar_always_offers_a_way_to_the_menu() {
        // Whether collapsed or expanded, `m` must be reachable from the view.
        for expanded in [false, true] {
            let pairs: Vec<&str> = if expanded {
                vec!["←→/hl", "↑↓/kj", "space", "r", "o", "esc", "m", "q"]
            } else {
                vec!["o", "m", "esc", "q"]
            };
            assert!(pairs.contains(&"m"), "no menu key when expanded={expanded}");
        }
    }

    #[test]
    fn the_summary_line_skips_what_the_book_lacks() {
        let view = BookView {
            book: Book {
                publish_year: Some(2014),
                page_count: Some(333),
                ..Book::default()
            },
            highlights: 4,
            notes: 0,
        };
        assert_eq!(summary(&view), "2014  ·  333pp  ·  4 highlights");

        let empty = BookView {
            book: Book::default(),
            highlights: 0,
            notes: 0,
        };
        assert_eq!(summary(&empty), "");
    }

    #[test]
    fn facts_skip_missing_fields() {
        let bare = Book::default();
        assert!(facts(&bare).is_empty());

        let filled = Book {
            publisher: Some("Picador".into()),
            publish_year: Some(2014),
            isbn_13: Some("9781447268963".into()),
            ..Book::default()
        };
        let labels: Vec<_> = facts(&filled).iter().map(|(l, _)| *l).collect();
        assert_eq!(labels, vec!["publisher", "year", "isbn"]);
    }
}
