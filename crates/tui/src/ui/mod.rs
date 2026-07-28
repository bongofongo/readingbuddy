//! Screen drawing and the responsive breakpoints.

pub mod apikey;
pub mod book;
pub mod device;
pub mod home;
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
/// says which side the object takes, from the pane's own shape. A small pane
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
    const ALL: [PaneOrientation; 4] = [
        PaneOrientation::BookTop,
        PaneOrientation::BookRight,
        PaneOrientation::BookBottom,
        PaneOrientation::BookLeft,
    ];

    /// Advance `steps` quarter-turns clockwise.
    pub fn rotate_cw(self, steps: u8) -> PaneOrientation {
        let base = Self::ALL.iter().position(|o| *o == self).unwrap_or(0);
        Self::ALL[(base + steps as usize) % 4]
    }

    /// A vertical divider (object and panel side by side) vs a horizontal one.
    fn is_vertical_divider(self) -> bool {
        matches!(self, PaneOrientation::BookLeft | PaneOrientation::BookRight)
    }
}

/// The user's per-view layout tweaks. `panel` shows or hides the section pane
/// (tab); `rotation` offsets the aspect-derived orientation clockwise (`t`);
/// `divider_bias` slides the one divider (positive grows the object's share).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutPrefs {
    /// Whether the section pane is on screen at all.
    pub panel: bool,
    /// Quarter-turns clockwise applied on top of the aspect default (0..=3).
    pub rotation: u8,
    /// Divider offset in fraction of the split axis, added to the default.
    pub divider_bias: f32,
}

impl Default for LayoutPrefs {
    fn default() -> Self {
        Self {
            // The view opens with its sections in reach; tab takes them away.
            panel: true,
            rotation: 0,
            divider_bias: 0.0,
        }
    }
}

/// The smallest pane that still splits into object + panel.
const MIN_SPLIT_WIDTH: u16 = 26;
const MIN_SPLIT_HEIGHT: u16 = 8;

/// The object column stops growing here by default; extra width goes to the
/// panel. (Bumped ~20% over the original 42 to give the object more presence.)
const BOOK_MAX_COLS: u16 = 50;
/// The info panel never gets narrower than this at the default divider.
const PANEL_MIN: u16 = 18;
/// The info panel stops growing here. Past this it is a wall of prose while the
/// object sits marooned against the edge of a wide terminal — so surplus width
/// becomes margin instead of panel. Chosen so `BOOK_MAX_COLS + PANEL_MAX_COLS`
/// is exactly the width at which `split_widths` already hands out `(50, 48)`:
/// the two caps agree rather than one quietly overriding the other.
const PANEL_MAX_COLS: u16 = 48;
/// A stacked layout (horizontal divider) gives the panel the *whole* width, so
/// its cap is a line-length measure rather than a sum of two panes. Height is
/// deliberately left uncapped: what hurts on a big terminal is line length, and
/// `stacked_object_height` already caps the object's band.
const STACK_MAX_COLS: u16 = 84;
/// A shrink-wrapped list box never gets narrower/shorter than this — a box that
/// hugged two short rows would read as a stray tooltip rather than a panel —
/// and never wider than the max, so long rows truncate instead of sprawling.
/// The row floor is deliberately modest: it is there to stop the box collapsing,
/// not to reserve space nothing is using.
const LIST_MIN_COLS: u16 = 34;
const LIST_MAX_COLS: u16 = 100;
const LIST_MIN_ROWS: u16 = 7;
/// Columns a list row needs around it: two borders, the two-column gutter
/// `List` reserves for its highlight symbol, and a column of padding each side
/// so rows are not pressed against the frame.
const LIST_CHROME: u16 = 6;
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

/// The natural orientation for the pane's shape: object on top when portrait
/// (the panel reads as a caption under it), object on the *right* when landscape
/// — the section pane on the left is the composition that looks best, and it is
/// the side the panel comes in from until `t` moves it.
fn aspect_default(area: Rect) -> PaneOrientation {
    if portrait(area) {
        PaneOrientation::BookTop
    } else {
        PaneOrientation::BookRight
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

/// The rect the book view actually occupies inside `area`: capped along the
/// width and centred, so a wide terminal becomes symmetric margin rather than a
/// giant panel with the object pinned to the left edge.
///
/// [`split_rects`] and [`biased_span`] run against *this* rect rather than the
/// raw pane, so `[` and `]` keep exactly the meaning they had; they simply act
/// on a smaller stage. Composing the cap outside `split_rects` (instead of
/// teaching it to stop tiling) is what keeps that function's contract, and its
/// tests, intact. [`book_rects`] then decides where the two rects actually sit.
///
/// The orientation is decided from the **uncapped** pane by
/// [`effective_orientation`]: the user's sense of the shape is about the
/// terminal they actually have, and deriving the cap from the orientation while
/// deriving the orientation from the cap would be circular.
pub fn content_block(area: Rect, o: PaneOrientation) -> Rect {
    let max = if o.is_vertical_divider() {
        BOOK_MAX_COLS + PANEL_MAX_COLS
    } else {
        STACK_MAX_COLS
    };
    let width = area.width.min(max);
    Rect {
        x: area.x + (area.width - width) / 2,
        width,
        ..area
    }
}

/// The book view's two rects: the object slid toward the centre of `area`, and
/// the section panel beside it.
///
/// [`content_block`] centres the *block* — object plus panel — which leaves the
/// object itself sitting left of centre on a wide terminal. The object is the
/// centrepiece, so it is the thing that wants the centre line: this slides it
/// there and lets the panel take the side it was already on.
///
/// The block is still what *sizes* the object ([`split_rects`] runs against it),
/// so `[` / `]` keep the step they had rather than scaling with the terminal —
/// only the position is recomputed. The slide stops as soon as it would starve
/// the panel below its floor, so a cramped pane degrades continuously back to
/// exactly the old tiling instead of stepping off a cliff at some size.
///
/// Horizontal dividers are passed straight through: the stacked layout keeps its
/// half-and-half spacing, which is what it looks best as. The book still finds
/// the middle of the window there whenever the section pane is dismissed, since
/// the object then has the whole thing.
pub fn book_rects(area: Rect, o: PaneOrientation, bias: f32) -> (Rect, Rect, Borders) {
    // The block sizes the object; where it and the panel *sit* is decided below.
    let (object, panel, border) = split_rects(content_block(area, o), o, bias);
    if !o.is_vertical_divider() {
        return (object, panel, border);
    }

    let w = object.width;
    let rest = area.width.saturating_sub(w);
    // The floor we can actually afford: on a cramped pane the panel's usual
    // minimum simply isn't there to reserve.
    let floor = PANEL_MIN.min(rest);
    let ideal = area.x + rest / 2;

    match o {
        PaneOrientation::BookLeft => {
            let x = ideal.min(area.x + rest - floor).max(area.x);
            let pw = (area.x + area.width - (x + w)).min(PANEL_MAX_COLS);
            let object = Rect {
                x,
                width: w,
                ..area
            };
            let panel = Rect {
                x: x + w,
                width: pw,
                ..area
            };
            (object, panel, Borders::LEFT)
        }
        // Object on the right of its panel; the rule stays against the object.
        _ => {
            let x = ideal.max(area.x + floor).min(area.x + rest);
            let pw = (x - area.x).min(PANEL_MAX_COLS);
            let object = Rect {
                x,
                width: w,
                ..area
            };
            let panel = Rect {
                x: x - pw,
                width: pw,
                ..area
            };
            (object, panel, Borders::RIGHT)
        }
    }
}

/// A centred box just big enough to hold `rows` list rows of `content` columns.
///
/// The list screens shrink-wrap rather than filling the pane: a library of three
/// books is a small box in the middle of the terminal, not a mostly-empty frame
/// stretched to the edges. Both axes are clamped — see [`LIST_MIN_COLS`] — and
/// `centered` clamps again to whatever the pane actually affords, so a long
/// library still grows to fill the height and scrolls from there.
///
/// `content` is the widest row's own width; [`LIST_CHROME`] is what has to fit
/// around it.
pub fn list_box(area: Rect, content: u16, rows: u16) -> Rect {
    let width = content
        .saturating_add(LIST_CHROME)
        .clamp(LIST_MIN_COLS, LIST_MAX_COLS);
    let height = rows.saturating_add(2).max(LIST_MIN_ROWS);
    centered(area, width, height)
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

    // The ambient layer goes down first, under everything. Each screen's
    // content box `Clear`s itself, so nothing shows through the text — a
    // `Block` only styles the cells it doesn't draw, it doesn't blank them.
    if app.ambient_visible() {
        let glyphs = app.params.glyphs;
        app.ambient.draw(f.buffer_mut(), body, glyphs);
    }

    match app.screen {
        Screen::Home => home::draw(f, app, body),
        Screen::Menu => menu::draw(f, app, body),
        Screen::Library => library::draw(f, app, body),
        Screen::Book => book::draw(f, app, body),
        Screen::Search => search::draw(f, app, body),
        Screen::Settings => settings::draw(f, app, body),
        Screen::Device => device::draw(f, app, body),
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
    fn landscape_puts_the_section_pane_on_the_left() {
        // Wider than tall (cells doubled): panel left, object right.
        assert_eq!(
            book_layout(Rect::new(0, 0, 120, 40), 0),
            BookLayout::Split(PaneOrientation::BookRight)
        );
        assert_eq!(
            book_layout(Rect::new(0, 0, 60, 20), 0),
            BookLayout::Split(PaneOrientation::BookRight)
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
        // Landscape starts at BookRight (panel left); each `t` goes
        // Right→Bottom→Left→Top.
        let a = Rect::new(0, 0, 120, 40);
        assert_eq!(effective_orientation(a, 0), PaneOrientation::BookRight);
        assert_eq!(effective_orientation(a, 1), PaneOrientation::BookBottom);
        assert_eq!(effective_orientation(a, 2), PaneOrientation::BookLeft);
        assert_eq!(effective_orientation(a, 3), PaneOrientation::BookTop);
        // Four turns returns to the default.
        assert_eq!(effective_orientation(a, 4 % 4), PaneOrientation::BookRight);
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
    fn content_block_caps_and_centres() {
        // At 200 wide the block is 98 (50 book + 48 panel), centred, so the
        // gutters are equal and the panel stops swallowing the surplus.
        let a = Rect::new(0, 0, 200, 40);
        let block = content_block(a, PaneOrientation::BookLeft);
        assert_eq!(block.width, BOOK_MAX_COLS + PANEL_MAX_COLS);
        assert_eq!(block.x, (200 - 98) / 2);
        assert_eq!(block.height, 40, "height is never capped");

        // And the two caps agree exactly: splitting the capped block hands out
        // precisely BOOK_MAX_COLS and PANEL_MAX_COLS.
        let (object, panel, _) = split_rects(block, PaneOrientation::BookLeft, 0.0);
        assert_eq!(object.width, BOOK_MAX_COLS);
        assert_eq!(panel.width, PANEL_MAX_COLS);
    }

    #[test]
    fn content_block_is_inert_on_a_narrow_pane() {
        // Below the cap nothing moves — small and medium panes are unchanged.
        for width in [26u16, 60, 80, 98] {
            let a = Rect::new(0, 0, width, 30);
            let block = content_block(a, PaneOrientation::BookLeft);
            assert_eq!(block, a, "the cap bit at {width} columns");
        }
    }

    #[test]
    fn a_stacked_layout_is_capped_by_line_length() {
        let a = Rect::new(0, 0, 200, 60);
        let block = content_block(a, PaneOrientation::BookTop);
        assert_eq!(block.width, STACK_MAX_COLS);
        assert_eq!(block.x, (200 - STACK_MAX_COLS) / 2);
        // Height is deliberately uncapped: the panel wants rows for its lists.
        assert_eq!(block.height, 60);
    }

    /// The cap's real guarantee, swept over every width, orientation and slide.
    ///
    /// Note what is *not* claimed: that the panel is always within
    /// `PANEL_MAX_COLS`. On a 61-column pane with the divider slid hard left the
    /// panel legitimately takes 50 — there is no surplus to give away, and
    /// capping it would open a gutter inside an already-cramped pane. The cap
    /// governs surplus width, so it binds at the default divider and the block
    /// bounds everything else.
    #[test]
    fn the_content_block_bounds_both_panes() {
        for width in (26..=400).step_by(7) {
            for o in PaneOrientation::ALL {
                let area = Rect::new(0, 0, width, 40);
                let block = content_block(area, o);
                assert!(
                    block.x >= area.x && block.x + block.width <= area.x + area.width,
                    "the block escaped the pane at {width}/{o:?}"
                );

                // At the default divider, the cap binds.
                let (_, panel, _) = split_rects(block, o, 0.0);
                if o.is_vertical_divider() {
                    assert!(
                        panel.width <= PANEL_MAX_COLS,
                        "panel {} > cap at {width}/{o:?}",
                        panel.width
                    );
                }

                // At any slide, both panes stay inside the block and alive.
                for bias in [-1.0f32, -0.3, 0.0, 0.3, 1.0] {
                    let (object, panel, _) = split_rects(block, o, bias);
                    for pane in [object, panel] {
                        assert!(pane.width > 0 && pane.height > 0);
                        assert!(
                            pane.x >= block.x && pane.x + pane.width <= block.x + block.width,
                            "a pane escaped the block at {width}/{o:?}/{bias}"
                        );
                    }
                }
            }
        }
    }

    /// The point of the whole exercise: on a wide window the *object* — not the
    /// seam between the panes — is what sits on the centre line.
    #[test]
    fn the_book_sits_at_the_centre_of_a_wide_pane() {
        // 180 columns, the width the layout dev-aid prints: object 50 wide at
        // x=65, so its centre is 90 — exactly the middle of the window.
        let a = Rect::new(0, 0, 180, 44);
        let (object, panel, border) = book_rects(a, PaneOrientation::BookLeft, 0.0);
        assert_eq!((object.x, object.width), (65, BOOK_MAX_COLS));
        assert_eq!(panel.x, 115);
        assert_eq!(panel.width, PANEL_MAX_COLS);
        assert_eq!(border, Borders::LEFT);

        // Mirroring the divider moves the panel, not the book.
        let (mirrored, panel, border) = book_rects(a, PaneOrientation::BookRight, 0.0);
        assert_eq!(
            mirrored, object,
            "the book jumped when the panel swapped side"
        );
        assert_eq!(panel.x + panel.width, object.x);
        assert_eq!(border, Borders::RIGHT);

        // And it holds across widths, to within the odd leftover column.
        for width in (86..=400).step_by(7) {
            for o in [PaneOrientation::BookLeft, PaneOrientation::BookRight] {
                let a = Rect::new(0, 0, width, 40);
                let (object, _, _) = book_rects(a, o, 0.0);
                let off = (2 * object.x + object.width).abs_diff(width);
                assert!(off <= 1, "book off centre by {off} at {width}/{o:?}");
            }
        }
    }

    /// Below the width that affords the panel its floor beside a centred object,
    /// the book slides as far as it can and no further — so the layout degrades
    /// continuously into the old edge-to-edge tiling instead of stepping off a
    /// cliff at some width.
    #[test]
    fn a_cramped_pane_slides_the_book_only_as_far_as_the_panel_allows() {
        // 60 columns: the panel is already at its floor, so nothing moves and
        // the result is exactly what the tiled layout gave.
        let a = Rect::new(0, 0, 60, 30);
        for o in PaneOrientation::ALL {
            assert_eq!(
                book_rects(a, o, 0.0),
                split_rects(content_block(a, o), o, 0.0),
                "the cramped layout changed at {o:?}"
            );
        }

        // 80 columns: part of the way there, and the panel keeps its floor.
        let (object, panel, _) =
            book_rects(Rect::new(0, 0, 80, 30), PaneOrientation::BookLeft, 0.0);
        assert_eq!(object.x, 12);
        assert_eq!(panel.width, PANEL_MIN);
    }

    /// The stacked layout keeps its half-and-half spacing: it is what the
    /// composition looks best as, so `book_rects` leaves those layouts exactly
    /// as `split_rects` tiled them.
    #[test]
    fn a_stacked_layout_keeps_its_half_and_half_spacing() {
        for width in [40u16, 98, 200] {
            for height in [12u16, 30, 50, 90] {
                for o in [PaneOrientation::BookTop, PaneOrientation::BookBottom] {
                    for bias in [-0.3f32, 0.0, 0.3] {
                        let a = Rect::new(0, 0, width, height);
                        assert_eq!(
                            book_rects(a, o, bias),
                            split_rects(content_block(a, o), o, bias),
                            "a stacked layout moved at {width}x{height}/{o:?}/{bias}"
                        );
                    }
                }
            }
        }
    }

    /// The same guarantee `the_content_block_bounds_both_panes` makes, for the
    /// centred composition: whatever the width, orientation or slide, both panes
    /// are alive, inside the pane, and do not overlap.
    #[test]
    fn centring_the_book_keeps_both_panes_alive() {
        for width in (26..=400).step_by(7) {
            for o in PaneOrientation::ALL {
                for bias in [-1.0f32, -0.3, 0.0, 0.3, 1.0] {
                    let area = Rect::new(0, 0, width, 40);
                    let (object, panel, _) = book_rects(area, o, bias);
                    let case = format!("{width}/{o:?}/{bias}");
                    for pane in [object, panel] {
                        assert!(pane.width > 0 && pane.height > 0, "empty pane at {case}");
                        assert!(
                            pane.x >= area.x && pane.x + pane.width <= area.x + area.width,
                            "a pane escaped the window at {case}"
                        );
                    }
                    // The divider's axis is the one they must not share.
                    if o.is_vertical_divider() {
                        assert!(
                            object.x + object.width <= panel.x || panel.x + panel.width <= object.x,
                            "the panes overlap at {case}"
                        );
                    } else {
                        assert!(
                            object.y + object.height <= panel.y
                                || panel.y + panel.height <= object.y,
                            "the panes overlap at {case}"
                        );
                    }
                    // The panel keeps its floor wherever the span affords one.
                    if o.is_vertical_divider() {
                        let afforded = PANEL_MIN.min(width - object.width);
                        assert!(
                            panel.width >= afforded,
                            "panel {} starved at {case}",
                            panel.width
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_list_box_shrink_wraps_its_contents() {
        // Three short rows on a big terminal: a small centred box, not a frame
        // stretched to the edges.
        let area = Rect::new(0, 0, 200, 50);
        let r = list_box(area, 40, 3);
        assert_eq!(r.width, 40 + LIST_CHROME);
        assert_eq!(r.height, LIST_MIN_ROWS, "3 rows is under the floor");
        assert_eq!(r.x, (200 - (40 + LIST_CHROME)) / 2);
        assert_eq!(r.y, (50 - LIST_MIN_ROWS) / 2);

        // More rows: the box grows with them.
        let r = list_box(area, 40, 20);
        assert_eq!(r.height, 22);
        assert_eq!(r.y, (50 - 22) / 2);
    }

    #[test]
    fn a_list_box_stays_between_its_floor_and_its_ceiling() {
        let area = Rect::new(0, 0, 200, 50);
        // A tiny library does not produce a tooltip-sized box.
        assert_eq!(list_box(area, 2, 0).width, LIST_MIN_COLS);
        // A very long row truncates rather than sprawling across the terminal.
        assert_eq!(list_box(area, 400, 5).width, LIST_MAX_COLS);
        // A huge library is clamped by the pane, and still centred.
        let r = list_box(area, 40, 500);
        assert_eq!(r.height, 50);
        assert_eq!(r.y, 0);
    }

    #[test]
    fn a_list_box_survives_a_pane_smaller_than_its_floor() {
        // `every_screen_draws_at_every_size` goes to 1x1; the floors must not
        // push the box outside the pane.
        for (w, h) in [(1u16, 1u16), (4, 2), (20, 8), (30, 6)] {
            let area = Rect::new(0, 0, w, h);
            let r = list_box(area, 60, 40);
            assert!(r.width <= w && r.height <= h);
            assert!(r.x + r.width <= w && r.y + r.height <= h);
        }
    }

    #[test]
    fn centering_clamps_to_the_available_area() {
        let r = centered(Rect::new(0, 0, 10, 4), 40, 20);
        assert_eq!(r, Rect::new(0, 0, 10, 4));
        let r = centered(Rect::new(0, 0, 20, 10), 10, 4);
        assert_eq!(r, Rect::new(5, 3, 10, 4));
    }
}
