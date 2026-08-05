//! The library list — pick a book to open in the viewing mode.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding};
use readingbuddy::{Book, Progress, names};

use crate::app::App;
use crate::theme;

/// What the library list is ordered by. Cycled with `s`, persisted in
/// `tui.toml`.
///
/// **Applied in the frontend, not by `ORDER BY`**, and that is the decision
/// here. `refresh_library` fetches a fixed page of the library by recency and
/// then narrows and orders it in Rust — so changing the sort re-orders *the
/// same books* rather than swapping which ones are on screen, which is what a
/// SQL `ORDER BY … LIMIT` would do.
///
/// This is a **policy about a fixed page**, and item 17 was careful not to
/// delete it. `BookSort` now has `Author` and `Year` too, and its contract is
/// the other one — the first N *by that key*, which is what `LIMIT` means and
/// what a paginated shelf needs. The two coexist; what must not happen is one
/// being mistaken for the other. The name parsing itself is no longer here:
/// [`readingbuddy::names`] owns it, so a GUI sorting by author gets the same
/// answer as this list rather than filing *The Overstory* under nothing.
///
/// The variant order is the cycle order, and it starts at the default so one
/// press moves away from it and four come back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    /// Most recently touched first — the order the engine hands them over, and
    /// what a library screen is usually opened to look at.
    #[default]
    Recent,
    /// By title, case-insensitively.
    ///
    /// **Leading articles are not stripped**, so "The Overstory" files under T.
    /// A library catalogue would file it under O, but this list shows the title
    /// it is sorting by: a row that reads "The Overstory" sitting between "Sea
    /// of Tranquility" and "Trust" looks like a bug, and no amount of being
    /// right about cataloguing makes it look less like one.
    Title,
    /// By the first author's last name.
    Author,
    /// Newest first, undated last.
    ///
    /// Descending, like every other list in this app — a year sort ascending
    /// would be the one place the newest thing is at the bottom.
    Year,
}

impl Sort {
    const ALL: [Sort; 4] = [Sort::Recent, Sort::Title, Sort::Author, Sort::Year];

    /// What the box title calls it, and what `tui.toml` stores.
    pub fn label(self) -> &'static str {
        match self {
            Sort::Recent => "recent",
            Sort::Title => "title",
            Sort::Author => "author",
            Sort::Year => "year",
        }
    }

    /// Parse a persisted label. An unknown value is not an error — the caller
    /// falls back to the default, the same as the ambient motif's.
    pub fn from_label(s: &str) -> Option<Sort> {
        Sort::ALL.into_iter().find(|o| o.label() == s)
    }

    pub fn next(self) -> Sort {
        let i = Sort::ALL.iter().position(|o| *o == self).unwrap_or(0);
        Sort::ALL[(i + 1) % Sort::ALL.len()]
    }

    /// Order `books` in place.
    ///
    /// [`Sort::Recent`] is a **no-op**, not a re-sort: the engine already
    /// returned the list that way, and re-deriving it here from
    /// `Book::last_modified` would be a second opinion about an order we were
    /// given.
    ///
    /// Every other arm sorts by a key that can tie — two books by one author,
    /// two from one year — and `sort_by_key` is stable, so a tie keeps the
    /// recency order underneath. That is a better tie-break than any second key
    /// could be, and it costs nothing: within an author, newest first.
    pub fn apply(self, books: &mut [Book]) {
        match self {
            Sort::Recent => {}
            Sort::Title => books.sort_by_key(|b| b.display_title().to_lowercase()),
            Sort::Author => books.sort_by_key(|b| names::sort_key(&b.authors)),
            // `Reverse` on the year alone: the `None` arm must stay *last*, so
            // it cannot ride along inside the reversal.
            Sort::Year => books.sort_by_key(|b| match b.publish_year {
                Some(y) => (0, std::cmp::Reverse(y)),
                None => (1, std::cmp::Reverse(0)),
            }),
        }
    }
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.library_state.selected();
    let rows: Vec<Line> = app
        .library
        .iter()
        .enumerate()
        .map(|(i, b)| row(b, Some(i) == selected))
        .collect();
    // A narrowed list says so in its own border. `app.library` is already the
    // filtered set, so the count is the count of what is on screen — and the
    // words are shown because a list that is short for a reason must not look
    // like a library that has lost books.
    // The sort is named in the border, always — including the default. "State
    // persists and is visible": a list ordered by author with nothing saying so
    // is a library that has quietly rearranged itself, and the one moment the
    // user needs to be told is the one after they pressed the key.
    let title = match &app.library_filter {
        Some(q) => format!(
            " library · “{q}” · by {} · {} ",
            app.library_sort.label(),
            app.library.len()
        ),
        None => format!(
            " library · by {} · {} ",
            app.library_sort.label(),
            app.library.len()
        ),
    };

    // Shrink-wrapped and centred: the box is the size of the library, not the
    // size of the terminal. The title sits in the top border, so it is part of
    // what the box has to be wide enough for. `Clear` because a `Block` styles
    // the cells it doesn't draw but never blanks them, so the ambient layer
    // would otherwise show through between the rows.
    let keys = key_bar(app.library_filter.is_some());
    let widest = rows.iter().map(|l| l.width() as u16).max().unwrap_or(0);
    let area = super::list_box(
        area,
        widest
            .max(title.chars().count() as u16)
            .max(keys.width() as u16),
        rows.len() as u16,
    );
    f.render_widget(ratatui::widgets::Clear, area);

    let items: Vec<ListItem> = rows.into_iter().map(ListItem::new).collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim())
        .padding(Padding::horizontal(1))
        .title(Span::styled(title, theme::accent()))
        .title_bottom(keys.centered());
    // No `highlight_style`: the reverse is scoped to the title span in `row`,
    // like the main menu, so the colored author/year/progress keep their hues.
    let list = List::new(items).block(block).highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.library_state);
}

/// A library row: colors differentiate the fields, but the selection reverse is
/// kept on the title alone (matching the main menu), so the other colors read.
fn row(b: &Book, selected: bool) -> Line<'static> {
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
    if let Some(year) = b.publish_year {
        spans.push(Span::styled(format!("  {year}"), theme::dim()));
    }
    let tag = progress_tag(b);
    if !tag.is_empty() {
        spans.push(Span::styled(format!("  {tag}"), theme::accent()));
    }
    Line::from(spans)
}

/// The keys, in the bottom border — the device shelf's and the home shelf's
/// arrangement, and here because `/` is otherwise a key nothing on the screen
/// mentions. Filtered, `esc` widens rather than leaves, so it says so: the two
/// meanings are one keypress apart and guessing wrong hides the list.
fn key_bar(filtered: bool) -> Line<'static> {
    Line::from(vec![
        Span::styled(" enter", theme::key()),
        Span::styled(" open  ", theme::dim()),
        Span::styled("/", theme::key()),
        Span::styled(" find  ", theme::dim()),
        Span::styled("s", theme::key()),
        Span::styled(" sort  ", theme::dim()),
        Span::styled("d", theme::key()),
        Span::styled(" remove  ", theme::dim()),
        Span::styled("esc", theme::key()),
        Span::styled(if filtered { " all " } else { " back " }, theme::dim()),
    ])
}

/// Short right-hand progress marker.
///
/// The **words** are this screen's; the arithmetic is
/// [`Progress`](readingbuddy::Progress)'. Item 17b: the four implementations of
/// "how far in is this" disagreed about a zero page count and about whether the
/// device's own percentage counts, and only one of them had the fallback.
pub fn progress_tag(b: &Book) -> String {
    tag(Progress::of_book(b))
}

/// The same words, for a progress the caller resolved itself — the home shelf
/// has the [`Reading`](readingbuddy::Reading) beside the book and should ask
/// about *that* read rather than about the book's current one, which on a
/// reread is not always the same thing.
pub fn tag(p: Progress) -> String {
    match p {
        Progress::Finished => "done".to_string(),
        Progress::Untouched => String::new(),
        p => match (p.percent(), p.page()) {
            (Some(pct), _) => format!("{pct}%"),
            (None, Some(page)) => format!("p.{page}"),
            // A device fraction with no page is now a percentage above, so this
            // arm is a `Started` carrying neither — which the constructor
            // returns `Untouched` for. Kept rather than `unreachable!`: a panic
            // in a draw call wrecks the user's pane.
            (None, None) => String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(current: Option<i64>, total: Option<i64>, finished: bool) -> Book {
        Book {
            current_page: current,
            page_count: total,
            finished,
            ..Book::default()
        }
    }

    #[test]
    fn progress_tag_covers_every_state() {
        assert_eq!(progress_tag(&book(Some(50), Some(200), false)), "25%");
        assert_eq!(progress_tag(&book(Some(50), None, false)), "p.50");
        assert_eq!(progress_tag(&book(None, Some(200), false)), "");
        assert_eq!(progress_tag(&book(Some(10), Some(10), true)), "done");
    }

    #[test]
    fn progress_tag_survives_a_zero_page_count() {
        assert_eq!(progress_tag(&book(Some(50), Some(0), false)), "p.50");
    }

    /// **New in item 17b**, and the visible half of the unification: this list
    /// used to show nothing for a KOReader-pulled book with a device percentage
    /// and no page, because the fallback lived in `home.rs` alone. Now one row
    /// of the library and the same row of the home shelf say the same thing.
    #[test]
    fn the_device_percentage_reaches_this_list_too() {
        let mut b = book(None, None, false);
        b.ko_percent = Some(0.43);
        assert_eq!(progress_tag(&b), "43%");
    }

    fn titled(title: &str, author: &str, year: Option<i64>) -> Book {
        Book {
            title: Some(title.into()),
            authors: if author.is_empty() {
                Vec::new()
            } else {
                vec![author.into()]
            },
            publish_year: year,
            ..Book::default()
        }
    }

    fn titles(books: &[Book]) -> Vec<&str> {
        books.iter().map(|b| b.display_title()).collect()
    }

    /// The four orders, on one list, so each is compared against the others
    /// rather than against itself.
    #[test]
    fn every_order_orders_what_it_says_it_does() {
        // Deliberately given in none of the four orders.
        let original = vec![
            titled("Station Eleven", "Emily St. John Mandel", Some(2014)),
            titled("A Wizard of Earthsea", "Ursula K. Le Guin", Some(1968)),
            titled("Pachinko", "Min Jin Lee", Some(2017)),
            titled("The Overstory", "Richard Powers", Some(2018)),
        ];

        // Recent is the engine's own order, untouched.
        let mut books = original.clone();
        Sort::Recent.apply(&mut books);
        assert_eq!(titles(&books), titles(&original));

        let mut books = original.clone();
        Sort::Title.apply(&mut books);
        assert_eq!(
            titles(&books),
            // Articles are not stripped, so "The Overstory" files under T.
            vec![
                "A Wizard of Earthsea",
                "Pachinko",
                "Station Eleven",
                "The Overstory"
            ]
        );

        let mut books = original.clone();
        Sort::Author.apply(&mut books);
        // Le Guin, Lee, Mandel, Powers — and "Le Guin" before "Lee" is the
        // particle being part of the surname rather than "Guin" filing under G.
        assert_eq!(
            titles(&books),
            vec![
                "A Wizard of Earthsea",
                "Pachinko",
                "Station Eleven",
                "The Overstory"
            ]
        );

        let mut books = original.clone();
        Sort::Year.apply(&mut books);
        assert_eq!(
            titles(&books),
            vec![
                "The Overstory",
                "Pachinko",
                "Station Eleven",
                "A Wizard of Earthsea"
            ]
        );
    }

    /// The two rows that have nothing to sort by go **last**, not first under
    /// an empty key — an unknown is not the beginning of the alphabet.
    #[test]
    fn a_missing_author_or_year_sinks_rather_than_floats() {
        let mut books = vec![
            titled("No One", "", None),
            titled("Pachinko", "Min Jin Lee", Some(2017)),
        ];
        Sort::Author.apply(&mut books);
        assert_eq!(titles(&books), vec!["Pachinko", "No One"]);

        let mut books = vec![
            titled("No One", "", None),
            titled("Pachinko", "Min Jin Lee", Some(2017)),
        ];
        Sort::Year.apply(&mut books);
        assert_eq!(titles(&books), vec!["Pachinko", "No One"]);
    }

    /// Ties keep the order underneath, which is recency — so within one author
    /// or one year the newest book is still first, for free.
    #[test]
    fn a_tie_keeps_the_order_it_arrived_in() {
        let mut books = vec![
            titled("Second", "Min Jin Lee", Some(2017)),
            titled("First", "Min Jin Lee", Some(2017)),
        ];
        Sort::Author.apply(&mut books);
        assert_eq!(titles(&books), vec!["Second", "First"]);
        Sort::Year.apply(&mut books);
        assert_eq!(titles(&books), vec!["Second", "First"]);
    }

    /// The cycle visits all four and comes back, and every label round-trips
    /// through the config file's spelling of it.
    #[test]
    fn the_cycle_is_closed_and_every_label_round_trips() {
        let mut seen = Vec::new();
        let mut s = Sort::default();
        for _ in 0..Sort::ALL.len() {
            seen.push(s);
            assert_eq!(Sort::from_label(s.label()), Some(s));
            s = s.next();
        }
        assert_eq!(s, Sort::default(), "the cycle does not close");
        assert_eq!(seen, Sort::ALL.to_vec());
        // A hand-edited typo is the default, never an error.
        assert_eq!(Sort::from_label("by-author"), None);
    }
}
