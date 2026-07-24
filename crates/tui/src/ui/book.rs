//! The single-book viewing mode: a tab strip (Info / Notes / Highlights /
//! Cards), the 3D object, and a key bar that is always present — collapsed to
//! the few keys worth advertising, expanded to the full set with `o`.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use readingbuddy::{Book, FlashcardRow, Highlight, NoteRecord};

use super::{BookLayout, book_layout, panel_width};
use crate::app::{App, BOOK_TABS, BookTab, BookView};
use crate::render3d::blit;
use crate::theme;

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
        BookLayout::Split => {
            // Object on the left (full height), section pane on the right. The
            // title + progress float over the top rows of the object rather than
            // taking a reserved pane, so the book gets the whole height.
            let [object, panel] = Layout::horizontal([
                Constraint::Min(0),
                Constraint::Length(panel_width(main.width)),
            ])
            .areas(main);
            draw_object(f, app, object, None);
            draw_header(f, app.view.as_ref().expect("checked above"), object);
            draw_panel(f, app, panel);
        }
        BookLayout::Bare => {
            // Too short for tabs; show the object with the title in the border.
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

/// Trace the book into the area. The renderer works in cells; it returns the
/// glyph family's subpixels per cell (octant: 2x4) for [`blit`] to quantize.
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
    blit::blit(fb, area, f.buffer_mut(), params.glyphs);
}

/// The title + progress header, floated over the top two rows of the object.
/// Text spans carry a `Color::Reset` background, so they read cleanly over the
/// block-glyph render beneath them.
fn draw_header(f: &mut Frame, view: &BookView, object: Rect) {
    // Leave a row of book showing above nothing — only float when there's room.
    if object.height < 3 || object.width < 4 {
        return;
    }
    let area = Rect {
        height: 2,
        ..object
    };
    let [title, prog] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let b = &view.book;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(b.display_title().to_string(), theme::title()),
            Span::styled("  ·  ", theme::dim()),
            Span::styled(b.display_authors().to_string(), theme::dim()),
        ])),
        title,
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(progress_text(b), theme::accent()))),
        prog,
    );
}

/// Progress as a compact one-liner for the floating header.
fn progress_text(b: &Book) -> String {
    match (b.finished, b.current_page, b.page_count) {
        (true, _, _) => "finished".to_string(),
        (_, Some(p), Some(t)) if t > 0 => {
            let pct = (p * 100 / t).min(100);
            format!("{p} / {t} · {pct}%")
        }
        (_, Some(p), _) => format!("page {p}"),
        _ => "not started".to_string(),
    }
}

/// The right pane: the section menu, or an open section's content. Separated
/// from the object by a left rule.
fn draw_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(theme::dim());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let inner = inner.inner(Margin::new(1, 0));
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.in_section {
        draw_section(f, app, inner);
    } else {
        draw_section_menu(f, app.book_tab, inner);
    }
}

/// The single-column section menu (Info / Notes / Highlights / Cards).
fn draw_section_menu(f: &mut Frame, active: BookTab, area: Rect) {
    let mut lines = Vec::new();
    for (tab, label) in BOOK_TABS {
        if tab == active {
            lines.push(Line::from(Span::styled(format!("› {label}"), theme::selected())));
        } else {
            lines.push(Line::from(Span::styled(format!("  {label}"), theme::primary())));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("enter ›", theme::dim())));
    f.render_widget(Paragraph::new(lines), area);
}

/// An open section: a header line naming it (with the way back), then content.
fn draw_section(f: &mut Frame, app: &mut App, area: Rect) {
    let label = BOOK_TABS
        .iter()
        .find(|(t, _)| *t == app.book_tab)
        .map(|(_, l)| *l)
        .unwrap_or("");
    let [head, content] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("‹ ", theme::dim()),
            Span::styled(label, theme::key()),
        ])),
        head,
    );

    match app.book_tab {
        BookTab::Info => {
            let view = app.view.as_ref().expect("checked");
            draw_info(f, view, content);
        }
        BookTab::Notes => {
            let items: Vec<ListItem> = app
                .view
                .as_ref()
                .expect("checked")
                .notes
                .iter()
                .map(|n| ListItem::new(note_line(n)))
                .collect();
            draw_list(f, app, content, items, "no notes yet — n to write one");
        }
        BookTab::Highlights => {
            let items: Vec<ListItem> = app
                .view
                .as_ref()
                .expect("checked")
                .highlights
                .iter()
                .map(|h| ListItem::new(highlight_line(h)))
                .collect();
            draw_list(f, app, content, items, "no highlights — import from KOReader");
        }
        BookTab::Cards => {
            let items: Vec<ListItem> = app
                .view
                .as_ref()
                .expect("checked")
                .cards
                .iter()
                .map(|c| ListItem::new(card_line(c)))
                .collect();
            draw_list(f, app, content, items, "no flashcards for this book");
        }
    }
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect, items: Vec<ListItem>, empty: &str) {
    if items.is_empty() {
        f.render_widget(Paragraph::new(empty).style(theme::dim()), area);
        return;
    }
    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.tab_state);
}

/// A note row: anchor tag (page / location / highlight) + title. Homogeneous
/// color + `·` separator so a REVERSED selection inverts uniformly.
fn note_line(n: &NoteRecord) -> Line<'static> {
    let mut spans = Vec::new();
    let tag = anchor_tag(n);
    if !tag.is_empty() {
        spans.push(Span::styled(tag, theme::primary()));
        spans.push(Span::styled(" · ", theme::primary()));
    }
    // Expand any tabs: a raw \t in a rendered cell desyncs the terminal.
    spans.push(Span::styled(n.title.replace('\t', "    "), theme::primary()));
    Line::from(spans)
}

fn anchor_tag(n: &NoteRecord) -> String {
    let mut parts = Vec::new();
    if let Some(p) = n.page {
        parts.push(format!("p.{p}"));
    }
    if let Some(l) = &n.location {
        parts.push(l.clone());
    }
    if n.page.is_none() && n.location.is_none() && n.highlight_id.is_some() {
        parts.push("↳hl".to_string());
    }
    parts.join(" ")
}

fn highlight_line(h: &Highlight) -> Line<'static> {
    let mut spans = Vec::new();
    if let Some(p) = h.page {
        spans.push(Span::styled(format!("p.{p}"), theme::primary()));
        spans.push(Span::styled(" · ", theme::primary()));
    }
    spans.push(Span::styled(h.text.clone(), theme::primary()));
    Line::from(spans)
}

fn card_line(c: &FlashcardRow) -> Line<'static> {
    let mark = if c.exported { "✓ " } else { "  " };
    let mut spans = vec![
        Span::styled(mark, theme::primary()),
        Span::styled(c.word.clone(), theme::primary()),
    ];
    if let Some(ctx) = &c.context {
        spans.push(Span::styled(" · ", theme::primary()));
        spans.push(Span::styled(ctx.clone(), theme::primary()));
    }
    Line::from(spans)
}

/// The Info section: the facts (title/author/progress live in the header now),
/// then the highlight/note counts.
fn draw_info(f: &mut Frame, view: &BookView, area: Rect) {
    let b = &view.book;
    let mut lines = Vec::new();
    for (label, value) in facts(b) {
        lines.push(Line::from(vec![
            Span::styled(format!("{label:<10}"), theme::dim()),
            Span::styled(value, theme::primary()),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(view.highlights.len().to_string(), theme::accent()),
        Span::styled(" highlights   ", theme::dim()),
        Span::styled(view.notes.len().to_string(), theme::accent()),
        Span::styled(" notes", theme::dim()),
    ]));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
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
        ("↑↓", "move"),
        ("↵/→", "open"),
        ("esc/b/←", "back"),
        ("n", "note"),
        ("d", "delete"),
        ("p", "page"),
        ("f", "finish"),
        ("x", "export"),
        ("space", spin),
        ("o", "less"),
        ("m", "menu"),
        ("q", "quit"),
    ];
    let collapsed: &[(&str, &str)] = &[
        ("↑↓", "move"),
        ("↵", "open"),
        ("n", "note"),
        ("o", "options"),
        ("m", "menu"),
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
                vec!["↑↓", "↵/→", "esc/b/←", "n", "d", "p", "f", "x", "space", "o", "m", "q"]
            } else {
                vec!["↑↓", "↵", "n", "o", "m", "q"]
            };
            assert!(pairs.contains(&"m"), "no menu key when expanded={expanded}");
        }
    }

    #[test]
    fn anchor_tag_prefers_page_then_location() {
        let mut n = NoteRecord {
            id: 1,
            book_id: Some(1),
            highlight_id: None,
            page: Some(42),
            location: None,
            file_path: "x.md".into(),
            title: "t".into(),
            kind: "note".into(),
            created_at: None,
        };
        assert_eq!(anchor_tag(&n), "p.42");
        n.location = Some("Ch 3".into());
        assert_eq!(anchor_tag(&n), "p.42 Ch 3");
        n.page = None;
        n.location = None;
        n.highlight_id = Some(9);
        assert_eq!(anchor_tag(&n), "↳hl");
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
