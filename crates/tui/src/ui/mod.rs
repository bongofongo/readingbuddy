//! Screen drawing and the responsive breakpoints.

pub mod book;
pub mod input;
pub mod library;
pub mod menu;
pub mod search;
pub mod settings;
pub mod textedit;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;

use crate::app::{App, Screen};
use crate::theme;

/// How the book view arranges itself for the space it has. The object always
/// sits on the left with the info/tabs panel on the right — down to a small
/// square pane; only a pane too narrow or too short to split falls back to
/// showing the object alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookLayout {
    /// Object on the left, info/tabs panel on the right.
    Split,
    /// Object only; the title moves into the border.
    Bare,
}

/// The smallest pane that still splits into object + panel.
const MIN_SPLIT_WIDTH: u16 = 26;
const MIN_SPLIT_HEIGHT: u16 = 8;

pub fn book_layout(area: Rect) -> BookLayout {
    if area.height < MIN_SPLIT_HEIGHT || area.width < MIN_SPLIT_WIDTH {
        BookLayout::Bare
    } else {
        BookLayout::Split
    }
}

/// Width of the right-hand info panel: scales with the pane but is capped, so
/// the object keeps the rest (≥10 cols given the `MIN_SPLIT_WIDTH` gate).
pub fn panel_width(width: u16) -> u16 {
    (width * 2 / 5).clamp(16, 34)
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    // The status line doubles as the confirm prompt.
    let status_line = confirm_prompt(app).or_else(|| app.status.clone());
    let status_h = if status_line.is_some() { 1 } else { 0 };
    let [body, status] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(status_h)]).areas(area);

    match app.screen {
        Screen::Menu => menu::draw(f, app, body),
        Screen::Library => library::draw(f, app, body),
        Screen::Book => book::draw(f, app, body),
        Screen::Search => search::draw(f, app, body),
        Screen::Settings => settings::draw(f, app, body),
    }

    if let Some(msg) = &status_line {
        let style = if app.confirm.is_some() { theme::accent() } else { theme::dim() };
        f.render_widget(Paragraph::new(msg.as_str()).style(style), status);
    }

    // A text input floats over whatever screen is underneath.
    if let Some(inp) = &app.input {
        let box_area = centered(body, 50.min(body.width), 3);
        f.render_widget(ratatui::widgets::Clear, box_area);
        input::render(f, box_area, inp.prompt, &inp.state);
    }

    // The note editor floats above everything else.
    if let Some(draft) = &app.note_editor {
        let w = 54.min(body.width);
        let h = 10.min(body.height).max(3);
        let box_area = centered(body, w, h);
        textedit::render(f, box_area, draft.title(), &draft.editor);
    }
}

fn confirm_prompt(app: &App) -> Option<String> {
    app.confirm.as_ref().map(|c| match c {
        crate::app::Confirm::RemoveBook { title, .. } => format!("remove {title}?  y / n"),
        crate::app::Confirm::DeleteNote(n) => format!("delete “{}”?  y / n", n.title),
        crate::app::Confirm::DiscardDraft => "discard note?  y / n".to_string(),
    })
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
    fn large_terminals_split() {
        assert_eq!(book_layout(Rect::new(0, 0, 120, 40)), BookLayout::Split);
    }

    #[test]
    fn small_squares_still_show_the_panel() {
        // A small square tmux pane keeps the info/tabs panel beside the object.
        assert_eq!(book_layout(Rect::new(0, 0, 40, 30)), BookLayout::Split);
        assert_eq!(book_layout(Rect::new(0, 0, 30, 26)), BookLayout::Split);
        assert_eq!(book_layout(Rect::new(0, 0, 26, 8)), BookLayout::Split);
    }

    #[test]
    fn too_narrow_or_too_short_falls_back_to_object_only() {
        assert_eq!(book_layout(Rect::new(0, 0, 25, 30)), BookLayout::Bare);
        assert_eq!(book_layout(Rect::new(0, 0, 120, 6)), BookLayout::Bare);
    }

    #[test]
    fn panel_width_scales_and_caps() {
        assert_eq!(panel_width(26), 16); // floor
        assert_eq!(panel_width(60), 24);
        assert_eq!(panel_width(200), 34); // cap
    }

    #[test]
    fn centering_clamps_to_the_available_area() {
        let r = centered(Rect::new(0, 0, 10, 4), 40, 20);
        assert_eq!(r, Rect::new(0, 0, 10, 4));
        let r = centered(Rect::new(0, 0, 20, 10), 10, 4);
        assert_eq!(r, Rect::new(5, 3, 10, 4));
    }
}
