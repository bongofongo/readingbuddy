//! The home screen: what you are currently reading.
//!
//! One row per **open** reading, in the order the engine's
//! `Engine::currently_reading` hands them over (most-recently-touched first).
//! It is the screen the app opens to, and it is deliberately a shelf rather
//! than a to-do list: every row is a place you can go — the book, its
//! reflection, its review — and there is **no count of anything** on it. A
//! number that greets you is task-completion framing, which `docs/decisions.md`
//! rules out by name, and it is the easy thing to write on a screen like this.
//!
//! The empty state is a place too. Nothing open must still say where to go
//! next, so the box names the two moves that end with a book on this shelf and
//! keeps their keys in the border.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding, Paragraph};
use readingbuddy::{Book, Reading};

use crate::app::App;
use crate::theme;

/// What the empty shelf says. Both halves are moves, not apologies: the point
/// of the axiom's "nothing is a dead end" is that a screen with nothing on it
/// still tells you where the somethings come from.
const EMPTY: &str = "nothing open right now — / to search for a book, m for the menu";

/// The box title. No count: the library screen can say how many books it holds
/// because that is inventory, but "3 books" on the screen that greets you reads
/// as a tally of what you have not finished.
const TITLE: &str = " reading ";

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.reading_state.selected();
    let rows: Vec<Line> = app
        .reading
        .iter()
        .enumerate()
        .map(|(i, (b, r))| row(b, r, Some(i) == selected))
        .collect();
    let keys = key_bar(app.reading.is_empty());

    // Shrink-wrapped and centred, like the library and the device shelf. The
    // key bar rides in the bottom border, so it is part of what the box has to
    // be wide enough for — the screen is only "never a dead end" if the way
    // onward is actually on screen.
    let widest = rows
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(EMPTY.chars().count() as u16)
        .max(TITLE.chars().count() as u16)
        .max(keys.width() as u16);
    let area = super::list_box(area, widest, rows.len() as u16);
    // `Clear` first: this screen inherits the ambient layer (it is not the book
    // view), and a `Block` styles the cells it does not draw without blanking
    // them, so the field would otherwise drift between the rows.
    f.render_widget(ratatui::widgets::Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim())
        .padding(Padding::horizontal(1))
        .title(Span::styled(TITLE, theme::accent()))
        .title_bottom(keys.centered());

    if rows.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        f.render_widget(Paragraph::new(EMPTY).style(theme::dim()), inner);
        return;
    }

    let items: Vec<ListItem> = rows.into_iter().map(ListItem::new).collect();
    // No `highlight_style`: the reverse is scoped to the title span in `row`,
    // matching every other list here, so the author and the progress keep their
    // hues instead of the row becoming a solid bar.
    let list = List::new(items).block(block).highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.reading_state);
}

/// One shelf row: `Title  Authors  42%`.
fn row(b: &Book, r: &Reading, selected: bool) -> Line<'static> {
    let title_style = if selected {
        theme::title().patch(theme::selected())
    } else {
        theme::title()
    };
    let mut spans = vec![Span::styled(b.display_title().to_string(), title_style)];
    spans.push(Span::styled(
        format!("  {}", b.display_authors()),
        theme::dim(),
    ));
    let tag = progress(b, r);
    if !tag.is_empty() {
        spans.push(Span::styled(format!("  {tag}"), theme::accent()));
    }
    Line::from(spans)
}

/// How far in this reading is: the library's own tag, and the device's own
/// percentage when the library has no page to show.
///
/// This is what the [`Reading`] beside the [`Book`] is for. `Book`'s progress
/// fields are projections of the *current* reading, and a book pulled from a
/// KOReader sidecar routinely has `ko_percent` and no `current_page` at all —
/// so without the second half of the pair the commonest row on this screen
/// would show nothing.
fn progress(b: &Book, r: &Reading) -> String {
    let tag = super::library::progress_tag(b);
    if !tag.is_empty() {
        return tag;
    }
    match r.ko_percent {
        Some(p) => format!("{:.0}%", (p * 100.0).clamp(0.0, 100.0)),
        None => String::new(),
    }
}

/// The keys, in the bottom border. `e` / `w` are the reflection and the review;
/// see `event::map_key` for why the pair is not `r` / `v`.
///
/// An empty shelf drops the three keys that act on a row. They would still be
/// *safe* — they say what to do instead of doing nothing quietly — but a key
/// bar advertising three keys with nothing to act on is the screen agreeing to
/// be a dead end in four words rather than none.
fn key_bar(empty: bool) -> Line<'static> {
    let mut spans = Vec::new();
    if !empty {
        spans.extend([
            Span::styled(" enter", theme::key()),
            Span::styled(" open  ", theme::dim()),
            Span::styled("e", theme::key()),
            Span::styled(" reflect  ", theme::dim()),
            Span::styled("w", theme::key()),
            Span::styled(" review  ", theme::dim()),
        ]);
    }
    spans.extend([
        Span::styled(if empty { " /" } else { "/" }, theme::key()),
        Span::styled(" search  ", theme::dim()),
        Span::styled("m", theme::key()),
        Span::styled(" menu ", theme::dim()),
    ]);
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    fn reading(ko_percent: Option<f64>) -> Reading {
        Reading {
            id: 1,
            book_id: 1,
            started_at: None,
            finished_at: None,
            status: "reading".into(),
            source: "manual".into(),
            current_page: None,
            ko_status: None,
            ko_percent,
            ko_rating: None,
            created_at: 0,
            last_modified: 0,
        }
    }

    fn book(current: Option<i64>, total: Option<i64>) -> Book {
        Book {
            title: Some("Station Eleven".into()),
            current_page: current,
            page_count: total,
            ..Book::default()
        }
    }

    #[test]
    fn a_page_beats_the_devices_percentage() {
        assert_eq!(
            progress(&book(Some(50), Some(200)), &reading(Some(0.9))),
            "25%"
        );
    }

    /// The commonest row on this screen: a book pulled off a device, which has
    /// a percentage and no page at all.
    #[test]
    fn the_devices_percentage_fills_in_for_a_missing_page() {
        assert_eq!(
            progress(&book(None, Some(200)), &reading(Some(0.42))),
            "42%"
        );
        assert_eq!(progress(&book(None, None), &reading(None)), "");
    }

    #[test]
    fn selection_reverses_the_title_only() {
        let line = row(&book(Some(50), Some(200)), &reading(None), true);
        let reversed: Vec<bool> = line
            .spans
            .iter()
            .map(|s| s.style.add_modifier.contains(Modifier::REVERSED))
            .collect();
        // title, authors, progress
        assert_eq!(reversed, vec![true, false, false]);
    }

    /// The screen carries no tally of anything — the axiom's "no
    /// task-completion framing", asserted rather than remembered.
    #[test]
    fn nothing_on_the_screen_counts_anything() {
        assert!(!TITLE.chars().any(|c| c.is_ascii_digit()));
        assert!(!EMPTY.chars().any(|c| c.is_ascii_digit()));
        // And the empty state names moves rather than reporting a lack.
        assert!(EMPTY.contains('/') && EMPTY.contains('m'));
    }
}
