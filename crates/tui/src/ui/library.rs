//! The library list — pick a book to open in the viewing mode.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use readingbuddy::Book;

use crate::app::App;
use crate::theme;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app.library.iter().map(|b| ListItem::new(row(b))).collect();
    let title = format!(" library · {} ", app.library.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim())
        .title(Span::styled(title, theme::accent()));
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected())
        .highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.library_state);
}

fn row(b: &Book) -> Line<'static> {
    let mut spans = vec![
        Span::styled(b.display_title().to_string(), theme::primary()),
        Span::styled(format!("  {}", b.display_authors()), theme::dim()),
    ];
    if let Some(year) = b.publish_year {
        spans.push(Span::styled(format!("  {year}"), theme::dim()));
    }
    spans.push(Span::styled(format!("  {}", progress_tag(b)), theme::accent()));
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
