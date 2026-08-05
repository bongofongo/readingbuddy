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
use readingbuddy::{Book, Progress, Reading};

use crate::app::App;
use crate::theme;

/// What the empty shelf says. Both halves are moves, not apologies: the point
/// of the axiom's "nothing is a dead end" is that a screen with nothing on it
/// still tells you where the somethings come from.
const EMPTY: &str = "nothing open right now — / to find a book, m for the menu";

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
    // **This** reading's progress, not the book's current one — the row is about
    // the open read it is listing. The device-percentage fallback that used to
    // live here as `fn progress` is `Progress`' now (item 17b), so the library
    // list gets it too instead of showing nothing for the commonest row in a
    // KOReader-sourced library.
    let tag = super::library::tag(Progress::of_reading(r, b.page_count));
    if !tag.is_empty() {
        spans.push(Span::styled(format!("  {tag}"), theme::accent()));
    }
    Line::from(spans)
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
        // "find", not "search": this key looks in the library, and the provider
        // search is what it offers when the library has nothing.
        Span::styled(if empty { " /" } else { "/" }, theme::key()),
        Span::styled(" find  ", theme::dim()),
        Span::styled("m", theme::key()),
        Span::styled(" menu ", theme::dim()),
    ]);
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    /// The page lives on the **reading** here, not on the book: this row is
    /// about one open read, and `Book`'s own `current_page` is a projection of
    /// whichever read is current. On real data they agree; a fixture that let
    /// them disagree would be testing the fixture.
    fn reading(page: Option<i64>, ko_percent: Option<f64>) -> Reading {
        Reading {
            id: 1,
            book_id: 1,
            started_at: None,
            finished_at: None,
            status: "reading".into(),
            source: "manual".into(),
            current_page: page,
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

    /// The rule itself now lives in `Progress` and is tested there. What these
    /// two pin is that **this screen still asks the question it used to** —
    /// item 17b deleted a local `fn progress` and the way to break that quietly
    /// is to stop consulting the device at all.
    #[test]
    fn a_page_beats_the_devices_percentage() {
        assert_eq!(
            super::super::library::tag(Progress::of_reading(
                &reading(Some(50), Some(0.9)),
                Some(200)
            )),
            "25%"
        );
    }

    /// The commonest row on this screen: a book pulled off a device, which has
    /// a percentage and no page at all.
    #[test]
    fn the_devices_percentage_fills_in_for_a_missing_page() {
        assert_eq!(
            super::super::library::tag(Progress::of_reading(&reading(None, Some(0.42)), Some(200))),
            "42%"
        );
        assert_eq!(
            super::super::library::tag(Progress::of_reading(&reading(None, None), None)),
            ""
        );
    }

    #[test]
    fn selection_reverses_the_title_only() {
        let line = row(&book(Some(50), Some(200)), &reading(Some(50), None), true);
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
