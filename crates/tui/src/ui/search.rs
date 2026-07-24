//! The search screen: a results list over a federated book search. The query
//! itself is entered through the shared input overlay (see `super::input`); this
//! draws whatever results the last query produced.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use readingbuddy::RankedResult;

use crate::app::App;
use crate::theme;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let title = format!(" search · {} ", app.search_results.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim())
        .title(Span::styled(title, theme::accent()));

    if app.search_results.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let hint = "press / to search — enter a title, author, or ISBN";
        f.render_widget(
            ratatui::widgets::Paragraph::new(hint).style(theme::dim()),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = app.search_results.iter().map(|r| ListItem::new(row(r))).collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected())
        .highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.search_state);
}

fn row(r: &RankedResult) -> Line<'static> {
    let b = &r.book;
    let mut spans = vec![
        Span::styled(b.display_title().to_string(), theme::primary()),
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
