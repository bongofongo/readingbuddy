//! The library list — pick a book to open in the viewing mode.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding};
use readingbuddy::Book;

use crate::app::App;
use crate::theme;

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
    let title = match &app.library_filter {
        Some(q) => format!(" library · “{q}” · {} ", app.library.len()),
        None => format!(" library · {} ", app.library.len()),
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
        Span::styled("d", theme::key()),
        Span::styled(" remove  ", theme::dim()),
        Span::styled("esc", theme::key()),
        Span::styled(if filtered { " all " } else { " back " }, theme::dim()),
    ])
}

/// Short right-hand progress marker.
pub fn progress_tag(b: &Book) -> String {
    if b.finished {
        return "done".to_string();
    }
    match (b.current_page, b.page_count) {
        (Some(p), Some(t)) if t > 0 => format!("{}%", (p * 100 / t).min(100)),
        (Some(p), _) => format!("p.{p}"),
        _ => String::new(),
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
}
