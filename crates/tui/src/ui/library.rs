//! The library list — pick a book to open in the viewing mode.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use readingbuddy::Book;

use crate::app::App;
use crate::theme;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.library_state.selected();
    let items: Vec<ListItem> = app
        .library
        .iter()
        .enumerate()
        .map(|(i, b)| ListItem::new(row(b, Some(i) == selected)))
        .collect();
    let title = format!(" library · {} ", app.library.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim())
        .title(Span::styled(title, theme::accent()));
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
    spans.push(Span::styled(format!("  {}", b.display_authors()), theme::dim()));
    if let Some(year) = b.publish_year {
        spans.push(Span::styled(format!("  {year}"), theme::dim()));
    }
    let tag = progress_tag(b);
    if !tag.is_empty() {
        spans.push(Span::styled(format!("  {tag}"), theme::accent()));
    }
    Line::from(spans)
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
