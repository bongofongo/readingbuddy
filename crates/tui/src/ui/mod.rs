//! Screen drawing and the responsive breakpoints.

pub mod book;
pub mod library;
pub mod menu;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;

use crate::app::{App, Screen};
use crate::theme;

/// How the book view arranges itself for the space it has. The eventual target
/// is a small square tmux pane, so `Compact` is the interesting branch — it
/// stacks a short header over the object instead of splitting side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookLayout {
    /// Info panel beside the object.
    Wide,
    /// Three-line header above the object.
    Compact,
    /// Object only; the title moves into the border.
    Bare,
}

pub fn book_layout(area: Rect) -> BookLayout {
    if area.height < 14 {
        BookLayout::Bare
    } else if area.width >= 80 && area.width >= area.height * 2 {
        BookLayout::Wide
    } else {
        BookLayout::Compact
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let status_h = if app.status.is_some() { 1 } else { 0 };
    let [body, status] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(status_h)]).areas(area);

    match app.screen {
        Screen::Menu => menu::draw(f, app, body),
        Screen::Library => library::draw(f, app, body),
        Screen::Book => book::draw(f, app, body),
    }

    if let Some(msg) = &app.status {
        f.render_widget(Paragraph::new(msg.as_str()).style(theme::dim()), status);
    }
}

/// Center a `width` x `height` box inside `area`, shrinking to fit.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_terminals_get_the_side_panel() {
        assert_eq!(book_layout(Rect::new(0, 0, 120, 40)), BookLayout::Wide);
    }

    #[test]
    fn narrow_and_square_panes_stack_vertically() {
        assert_eq!(book_layout(Rect::new(0, 0, 60, 30)), BookLayout::Compact);
        // The intended tmux pane: a small square.
        assert_eq!(book_layout(Rect::new(0, 0, 40, 30)), BookLayout::Compact);
        // Wide enough for a panel in columns, but too tall for the split to
        // leave the object a sensible shape.
        assert_eq!(book_layout(Rect::new(0, 0, 90, 50)), BookLayout::Compact);
    }

    #[test]
    fn very_short_panes_drop_the_info_block() {
        assert_eq!(book_layout(Rect::new(0, 0, 120, 10)), BookLayout::Bare);
    }

    #[test]
    fn centering_clamps_to_the_available_area() {
        let r = centered(Rect::new(0, 0, 10, 4), 40, 20);
        assert_eq!(r, Rect::new(0, 0, 10, 4));
        let r = centered(Rect::new(0, 0, 20, 10), 10, 4);
        assert_eq!(r, Rect::new(5, 3, 10, 4));
    }
}
