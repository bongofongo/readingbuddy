//! The search screen: a results list over a federated book search. The query
//! itself is entered through the shared input overlay (see `super::input`); this
//! draws whatever results the last query produced.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding};
use readingbuddy::RankedResult;

use crate::app::App;
use crate::theme;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    const HINT: &str = "press / to search — enter a title, author, or ISBN";

    let selected = app.search_state.selected();
    let rows: Vec<Line> = app
        .search_results
        .iter()
        .enumerate()
        .map(|(i, r)| row(r, Some(i) == selected))
        .collect();
    let title = format!(" search · {} ", app.search_results.len());

    // Shrink-wrapped and centred — see `library::draw`. With no results the box
    // still has to hold the hint, which is wider than any of the chrome.
    let widest = rows
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(HINT.chars().count() as u16);
    let area = super::list_box(
        area,
        widest.max(title.chars().count() as u16),
        rows.len() as u16,
    );
    f.render_widget(ratatui::widgets::Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim())
        .padding(Padding::horizontal(1))
        .title(Span::styled(title, theme::accent()));

    if rows.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            ratatui::widgets::Paragraph::new(HINT).style(theme::dim()),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = rows.into_iter().map(ListItem::new).collect();
    // No `highlight_style`: the reverse is scoped to the title span in `row`,
    // matching the library and the main menu, so the dim author/year/isbn keep
    // their hues instead of the whole line inverting into a solid bar.
    let list = List::new(items).block(block).highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.search_state);
}

/// A result row: colors differentiate the fields, but the selection reverse is
/// kept on the title alone (matching the library), so the other colors read.
fn row(r: &RankedResult, selected: bool) -> Line<'static> {
    let b = &r.book;
    // Keeps the row's existing weight — only the reverse is added, so this is
    // the highlight fix and nothing else.
    let title_style = if selected {
        theme::primary().patch(theme::selected())
    } else {
        theme::primary()
    };
    let mut spans = vec![
        Span::styled(b.display_title().to_string(), title_style),
        Span::styled(format!("  {}", b.display_authors()), theme::dim()),
    ];
    if let Some(y) = b.publish_year {
        spans.push(Span::styled(format!("  {y}"), theme::dim()));
    }
    if let Some(i) = b.any_isbn() {
        spans.push(Span::styled(format!("  {i}"), theme::dim()));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;
    use readingbuddy::Book;

    fn result() -> RankedResult {
        RankedResult {
            book: Book {
                title: Some("Station Eleven".into()),
                authors: vec!["Emily St. John Mandel".into()],
                publish_year: Some(2014),
                isbn_13: Some("9781447268963".into()),
                ..Book::default()
            },
            sources: Vec::new(),
            score: 1.0,
        }
    }

    /// The selection reverse must cover the title and nothing else. Reversing
    /// the whole line turns a result into a solid bar and throws away the
    /// colour that distinguishes author from year from ISBN.
    #[test]
    fn selection_reverses_the_title_only() {
        let r = result();
        let line = row(&r, true);
        let reversed: Vec<bool> = line
            .spans
            .iter()
            .map(|s| s.style.add_modifier.contains(Modifier::REVERSED))
            .collect();
        assert_eq!(
            reversed,
            vec![true, false, false, false],
            "expected the reverse on the title span alone"
        );

        // Unselected, nothing is reversed at all.
        let plain = row(&r, false);
        assert!(
            plain
                .spans
                .iter()
                .all(|s| !s.style.add_modifier.contains(Modifier::REVERSED))
        );
    }
}
