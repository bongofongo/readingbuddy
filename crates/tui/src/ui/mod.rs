//! Screen drawing and the responsive breakpoints.

pub mod apikey;
pub mod book;
pub mod input;
pub mod library;
pub mod menu;
pub mod search;
pub mod settings;
pub mod textedit;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Borders, Paragraph};

use crate::app::{App, Screen};
use crate::theme;

/// How the book view arranges itself for the space it has. A big pane splits
/// into the object + the section panel along one divider; [`PaneOrientation`]
/// says which side the object takes (the user rotates it). A small pane
/// collapses to the object alone, with sections reachable in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookLayout {
    /// Object + section panel, split along one divider at the given orientation.
    Split(PaneOrientation),
    /// Small: the object fills the pane (title in the border); opening a
    /// section swaps its content in where the object was.
    Compact,
}

/// Where the object sits relative to the section panel. With two panes this is
/// the one divider's placement; rotating the layout advances it clockwise
/// (Top → Right → Bottom → Left → Top).
// The shared `Book` prefix is the point — each names where the book sits.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneOrientation {
    BookTop,
    BookRight,
    BookBottom,
    BookLeft,
}

impl PaneOrientation {
    const CW: [PaneOrientation; 4] = [
        PaneOrientation::BookTop,
        PaneOrientation::BookRight,
        PaneOrientation::BookBottom,
        PaneOrientation::BookLeft,
    ];

    /// Advance `steps` quarter-turns clockwise.
    pub fn rotate_cw(self, steps: u8) -> PaneOrientation {
        let base = Self::CW.iter().position(|o| *o == self).unwrap_or(0);
        Self::CW[(base + steps as usize) % 4]
    }

    /// A vertical divider (object and panel side by side) vs a horizontal one.
    fn is_vertical_divider(self) -> bool {
        matches!(self, PaneOrientation::BookLeft | PaneOrientation::BookRight)
    }
}

/// The user's per-view layout tweaks. `rotation` offsets the aspect-derived
/// orientation clockwise; `divider_bias` slides the one divider (positive grows
/// the object's share). Both reset implicitly by returning to defaults.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutPrefs {
    /// Quarter-turns clockwise applied on top of the aspect default (0..=3).
    pub rotation: u8,
    /// Divider offset in fraction of the split axis, added to the default.
    pub divider_bias: f32,
}

/// The smallest pane that still splits into object + panel.
const MIN_SPLIT_WIDTH: u16 = 26;
const MIN_SPLIT_HEIGHT: u16 = 8;

/// The object column stops growing here by default; extra width goes to the
/// panel. (Bumped ~20% over the original 42 to give the object more presence.)
const BOOK_MAX_COLS: u16 = 50;
/// The info panel never gets narrower than this at the default divider.
const PANEL_MIN: u16 = 18;
/// The object band stops growing here (rows) by default. (~20% over 22.)
pub const BOOK_MAX_ROWS: u16 = 26;

/// Neither pane may shrink below this many cells along the divider axis.
const MIN_PANE: u16 = 6;
/// Divider bias is clamped so the object keeps between these fractions.
const DIVIDER_MIN_FRAC: f32 = 0.18;
const DIVIDER_MAX_FRAC: f32 = 0.82;
/// One grow/shrink keypress moves the divider this fraction of the axis.
pub const DIVIDER_STEP: f32 = 0.06;

/// Which layout the space affords, at the user's current rotation.
pub fn book_layout(area: Rect, rotation: u8) -> BookLayout {
    if area.height < MIN_SPLIT_HEIGHT || area.width < MIN_SPLIT_WIDTH {
        BookLayout::Compact
    } else {
        BookLayout::Split(effective_orientation(area, rotation))
    }
}

/// The orientation to use: the aspect default, rotated by the user's offset.
pub fn effective_orientation(area: Rect, rotation: u8) -> PaneOrientation {
    aspect_default(area).rotate_cw(rotation)
}

/// The natural orientation for the pane's shape: object on top when portrait,
/// object on the left when landscape.
fn aspect_default(area: Rect) -> PaneOrientation {
    if portrait(area) {
        PaneOrientation::BookTop
    } else {
        PaneOrientation::BookLeft
    }
}

/// True when the pane is physically taller than wide. A cell is ~1 wide : 2
/// tall, so the height is doubled before the comparison.
fn portrait(area: Rect) -> bool {
    area.height as u32 * 2 > area.width as u32
}

/// Split the width into (object, panel) columns at the default divider. The
/// object grows only up to `BOOK_MAX_COLS`; past that the panel absorbs the
/// surplus, so on a wide terminal the info panel dominates rather than a giant
/// book. This is the *default* — the user can slide past it (see `split_rects`).
pub fn split_widths(width: u16) -> (u16, u16) {
    let object = BOOK_MAX_COLS.min(width.saturating_sub(PANEL_MIN)).max(1);
    (object, width - object)
}

/// Height of the object band at the default divider: the top half, but capped
/// so a very tall pane gives the surplus to the sections, not the book.
pub fn stacked_object_height(height: u16) -> u16 {
    (height / 2).clamp(1, BOOK_MAX_ROWS)
}

/// Carve `area` into (object, panel, panel-border-side) for an orientation and
/// the user's divider bias. The border side is the divider rule — it sits on
/// the panel edge facing the object.
pub fn split_rects(area: Rect, o: PaneOrientation, bias: f32) -> (Rect, Rect, Borders) {
    if o.is_vertical_divider() {
        let default = split_widths(area.width).0 as f32 / area.width as f32;
        let bw = biased_span(area.width, default, bias);
        let pw = area.width - bw;
        match o {
            PaneOrientation::BookLeft => {
                let book = Rect { width: bw, ..area };
                let panel = Rect {
                    x: area.x + bw,
                    width: pw,
                    ..area
                };
                (book, panel, Borders::LEFT)
            }
            // Object on the right; panel takes the left.
            _ => {
                let panel = Rect { width: pw, ..area };
                let book = Rect {
                    x: area.x + pw,
                    width: bw,
                    ..area
                };
                (book, panel, Borders::RIGHT)
            }
        }
    } else {
        let default = stacked_object_height(area.height) as f32 / area.height as f32;
        let bh = biased_span(area.height, default, bias);
        let ph = area.height - bh;
        match o {
            PaneOrientation::BookTop => {
                let book = Rect { height: bh, ..area };
                let panel = Rect {
                    y: area.y + bh,
                    height: ph,
                    ..area
                };
                (book, panel, Borders::TOP)
            }
            // Object on the bottom; panel takes the top.
            _ => {
                let panel = Rect { height: ph, ..area };
                let book = Rect {
                    y: area.y + ph,
                    height: bh,
                    ..area
                };
                (book, panel, Borders::BOTTOM)
            }
        }
    }
}

/// The object's span (cells) along the divider axis: the default fraction plus
/// the user's bias, clamped to keep both panes usable.
fn biased_span(axis: u16, default_frac: f32, bias: f32) -> u16 {
    let frac = (default_frac + bias).clamp(DIVIDER_MIN_FRAC, DIVIDER_MAX_FRAC);
    let span = (frac * axis as f32).round() as u16;
    span.clamp(MIN_PANE, axis.saturating_sub(MIN_PANE).max(1))
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
        let style = if app.confirm.is_some() {
            theme::accent()
        } else {
            theme::dim()
        };
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

    // The API-key modal floats over the settings screen.
    if let Some(modal) = &app.api_key {
        apikey::render(f, body, modal);
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
    fn landscape_defaults_to_book_left() {
        // Wider than tall (cells doubled): object left, panel right.
        assert_eq!(
            book_layout(Rect::new(0, 0, 120, 40), 0),
            BookLayout::Split(PaneOrientation::BookLeft)
        );
        assert_eq!(
            book_layout(Rect::new(0, 0, 60, 20), 0),
            BookLayout::Split(PaneOrientation::BookLeft)
        );
    }

    #[test]
    fn portrait_defaults_to_book_top() {
        // Taller than wide once cell aspect is accounted for: object on top.
        assert_eq!(
            book_layout(Rect::new(0, 0, 80, 50), 0),
            BookLayout::Split(PaneOrientation::BookTop)
        );
        assert_eq!(
            book_layout(Rect::new(0, 0, 30, 40), 0),
            BookLayout::Split(PaneOrientation::BookTop)
        );
    }

    #[test]
    fn rotation_advances_clockwise_from_the_aspect_default() {
        // Landscape default is BookLeft; each turn goes Left→Top→Right→Bottom.
        let a = Rect::new(0, 0, 120, 40);
        assert_eq!(effective_orientation(a, 0), PaneOrientation::BookLeft);
        assert_eq!(effective_orientation(a, 1), PaneOrientation::BookTop);
        assert_eq!(effective_orientation(a, 2), PaneOrientation::BookRight);
        assert_eq!(effective_orientation(a, 3), PaneOrientation::BookBottom);
        // Four turns returns to the default.
        assert_eq!(effective_orientation(a, 4 % 4), PaneOrientation::BookLeft);
    }

    #[test]
    fn small_panes_are_compact() {
        assert_eq!(book_layout(Rect::new(0, 0, 25, 30), 0), BookLayout::Compact);
        assert_eq!(book_layout(Rect::new(0, 0, 120, 6), 0), BookLayout::Compact);
    }

    #[test]
    fn split_caps_the_book_and_grows_the_panel() {
        // Book column stops at BOOK_MAX_COLS; the panel absorbs extra width.
        assert_eq!(split_widths(200), (BOOK_MAX_COLS, 200 - BOOK_MAX_COLS));
        assert_eq!(split_widths(120), (BOOK_MAX_COLS, 120 - BOOK_MAX_COLS));
        // Narrow: the panel keeps its floor, the book takes the rest.
        assert_eq!(split_widths(26), (26 - PANEL_MIN, PANEL_MIN));
    }

    #[test]
    fn stacked_object_band_is_capped() {
        assert_eq!(stacked_object_height(30), 15); // half
        assert_eq!(stacked_object_height(120), BOOK_MAX_ROWS); // capped
    }

    #[test]
    fn split_rects_place_panes_and_the_divider_rule() {
        let a = Rect::new(0, 0, 120, 40);
        // BookLeft: object on the left, panel to its right, rule on panel left.
        let (book, panel, border) = split_rects(a, PaneOrientation::BookLeft, 0.0);
        assert_eq!(book.x, 0);
        assert_eq!(panel.x, book.width);
        assert_eq!(book.width + panel.width, 120);
        assert_eq!(border, Borders::LEFT);
        // BookRight mirrors it: panel on the left, rule on panel right.
        let (book, panel, border) = split_rects(a, PaneOrientation::BookRight, 0.0);
        assert_eq!(panel.x, 0);
        assert_eq!(book.x, panel.width);
        assert_eq!(border, Borders::RIGHT);
        // BookTop / BookBottom divide the height, rule on top / bottom.
        let (book, panel, border) = split_rects(a, PaneOrientation::BookTop, 0.0);
        assert_eq!(book.y, 0);
        assert_eq!(panel.y, book.height);
        assert_eq!(border, Borders::TOP);
        let (book, panel, border) = split_rects(a, PaneOrientation::BookBottom, 0.0);
        assert_eq!(panel.y, 0);
        assert_eq!(book.y, panel.height);
        assert_eq!(border, Borders::BOTTOM);
    }

    #[test]
    fn divider_bias_slides_the_split_within_bounds() {
        let a = Rect::new(0, 0, 120, 40);
        let base = split_rects(a, PaneOrientation::BookLeft, 0.0).0.width;
        let grown = split_rects(a, PaneOrientation::BookLeft, 0.2).0.width;
        let shrunk = split_rects(a, PaneOrientation::BookLeft, -0.2).0.width;
        assert!(grown > base, "positive bias grows the object");
        assert!(shrunk < base, "negative bias shrinks it");
        // Extreme bias is clamped so both panes survive.
        let huge = split_rects(a, PaneOrientation::BookLeft, 5.0).0.width;
        assert!(huge <= 120 - MIN_PANE);
        let tiny = split_rects(a, PaneOrientation::BookLeft, -5.0).0.width;
        assert!(tiny >= MIN_PANE);
    }

    #[test]
    fn centering_clamps_to_the_available_area() {
        let r = centered(Rect::new(0, 0, 10, 4), 40, 20);
        assert_eq!(r, Rect::new(0, 0, 10, 4));
        let r = centered(Rect::new(0, 0, 20, 10), 10, 4);
        assert_eq!(r, Rect::new(5, 3, 10, 4));
    }
}
