//! The book presentation seam.
//!
//! [`BookPresenter`] is the one place the single-book view talks to a rendering
//! backend, so the backend is a real swap point. Two backends now exist:
//! [`GlyphPresenter`] (block glyphs, works anywhere truecolor does) and
//! [`RichPresenter`] (true pixels via the kitty graphics protocol, chosen when
//! the startup probe says the terminal can take them — tmux included).
//!
//! The caller reads what it needs out of `App` first and hands us borrows,
//! because `ui::book::present_book` depends on *disjoint direct field* borrows
//! of `App`. See the note there before changing these signatures.

use std::io::Write;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use readingbuddy::Book;

use super::raster::{Quality, Target};
use super::{Caps, Model, RenderMode, RenderParams, Scene, blit, kitty, raster};

/// Paints the book into a cell rect. `area` is the final inner region — any
/// border has already been drawn by the caller.
pub trait BookPresenter {
    fn draw_book(&mut self, f: &mut Frame, area: Rect, book: &Book, params: RenderParams);
}

/// The block-glyph raytrace: trace the cuboid into an [`super::RgbBuf`] and
/// quantize it to terminal cells. Works everywhere truecolor does, tmux
/// included — the fallback for every situation the probe can't improve on.
pub struct GlyphPresenter<'a> {
    scene: &'a mut Scene,
}

impl<'a> GlyphPresenter<'a> {
    pub fn new(scene: &'a mut Scene) -> Self {
        GlyphPresenter { scene }
    }
}

impl BookPresenter for GlyphPresenter<'_> {
    fn draw_book(&mut self, f: &mut Frame, area: Rect, book: &Book, params: RenderParams) {
        let fb = self.scene.frame(book, area.width, area.height, params);
        blit::blit(fb, area, f.buffer_mut(), params.glyphs);
    }
}

/// Pose quantum used to tell "moving" from "parked". Matches `Scene`'s, and is
/// fine enough that one spin tick always crosses several buckets.
const FINE_Q: f32 = 512.0;

/// Pose quantum used for the transmit cache while the book is moving — the
/// throttle on how often a spinning book is re-sent.
///
/// `1/32 rad` is ~1.8 degrees. At the spin's 0.01164 rad/tick that is a new
/// frame roughly every 2.7 ticks, so a 20fps spin retransmits at ~7fps. The
/// book turns 13 degrees/second, so 1.8-degree steps still read as motion
/// rather than as stepping, and the byte rate drops by the same 2.7x on top of
/// the reduced [`raster::MOTION_MAX_PX`].
const MOTION_Q: f32 = 32.0;

/// Identifies a transmitted rich frame, so an unchanged one is never re-sent.
///
/// `Copy` and hash-based rather than holding the cover key's `String`s, which
/// keeps comparison to a few integer compares on the hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RichKey {
    cover: u64,
    yaw_q: i32,
    pitch_q: i32,
    px_w: u32,
    px_h: u32,
    cols: u16,
    rows: u16,
    quality: Quality,
}

fn cover_hash(book: &Book) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    book.id.hash(&mut h);
    book.cover_path.hash(&mut h);
    book.display_title().hash(&mut h);
    book.page_count.hash(&mut h);
    h.finish()
}

/// Everything the pixel path remembers between draws.
///
/// Lives as its own field on `App` (not behind a method) so `present_book` can
/// borrow it alongside `app.scene` and `app.view`.
pub struct RichState {
    caps: Caps,
    id: u32,
    /// The frame currently transmitted, if any.
    sent: Option<RichKey>,
    /// Pose at the previous draw, which is how a settled book is recognised
    /// without any extra plumbing from the event loop.
    last_pose: Option<(i32, i32)>,
    /// Cached placeholder symbols for the current span, rebuilt only when the
    /// rect changes — otherwise this would allocate a String per cell per frame.
    span: (u16, u16),
    placeholders: Vec<String>,
    /// Where escapes go. A field so tests can capture them instead of spraying
    /// control bytes through the test harness's stdout.
    out: Box<dyn Write + Send>,
}

impl Default for RichState {
    fn default() -> Self {
        RichState {
            // The safe floor: no pixels until a probe says otherwise, so tests
            // and non-tty runs can never emit graphics escapes.
            caps: Caps::default(),
            id: kitty::image_id(),
            sent: None,
            last_pose: None,
            span: (0, 0),
            placeholders: Vec::new(),
            out: Box::new(std::io::stdout()),
        }
    }
}

impl RichState {
    pub fn set_caps(&mut self, caps: Caps) {
        self.caps = caps;
    }

    /// Route escapes somewhere other than stdout (tests).
    #[cfg(test)]
    pub fn with_writer(caps: Caps, out: Box<dyn Write + Send>) -> Self {
        RichState {
            caps,
            out,
            ..RichState::default()
        }
    }

    /// Take the image off the screen right now.
    ///
    /// Needed when leaving rich mode: the placeholder cells stop being written
    /// but the image itself would otherwise stay composited under the glyphs.
    pub fn drop_image(&mut self) {
        if self.sent.is_some() {
            let esc = kitty::delete(self.id, self.caps.in_tmux);
            let _ = self.out.write_all(esc.as_bytes());
            let _ = self.out.flush();
            kitty::forget_live();
        }
        self.sent = None;
        self.last_pose = None;
        self.span = (0, 0);
        self.placeholders.clear();
    }

    /// Rebuild the placeholder lookup table if the rect changed. Returns false
    /// when the span is larger than the diacritic scheme can address.
    fn ensure_placeholders(&mut self, cols: u16, rows: u16) -> bool {
        if cols > kitty::MAX_SPAN || rows > kitty::MAX_SPAN {
            return false;
        }
        if self.span == (cols, rows) && !self.placeholders.is_empty() {
            return true;
        }
        self.placeholders.clear();
        self.placeholders.reserve(cols as usize * rows as usize);
        for r in 0..rows {
            for c in 0..cols {
                match kitty::placeholder_symbol(r, c) {
                    Some(s) => self.placeholders.push(s),
                    None => return false,
                }
            }
        }
        self.span = (cols, rows);
        true
    }
}

/// The pixel backend.
///
/// Splits cleanly in two: escapes (the image itself) go straight to the writer
/// from here, and placement is nothing but ordinary buffer cells. Writing the
/// escapes inside `draw_book` is correct ordering, not a hack — `Terminal::draw`
/// only mutates the `Buffer` and does not touch the wire until it flushes
/// afterwards, so the image always lands before the cells that reference it.
pub struct RichPresenter<'a> {
    scene: &'a mut Scene,
    state: &'a mut RichState,
}

impl<'a> RichPresenter<'a> {
    pub fn new(scene: &'a mut Scene, state: &'a mut RichState) -> Self {
        RichPresenter { scene, state }
    }

    /// Trace, transmit and place — or do nothing but repaint the placeholder
    /// cells when the cached frame still matches. Returns false to ask the
    /// caller for a glyph fallback.
    fn draw_pixels(
        &mut self,
        f: &mut Frame,
        area: Rect,
        book: &Book,
        params: RenderParams,
    ) -> bool {
        if !self.state.caps.supports_pixels() {
            return false;
        }
        if !self.state.ensure_placeholders(area.width, area.height) {
            // Rect is larger than the placeholder scheme can address; glyphs
            // handle any size, so fall back rather than clip the image.
            return false;
        }

        // Two quantizations of the same pose, for two different jobs.
        //
        // The *fine* one decides whether the book is still moving: while the
        // spin runs, a tick advances yaw by ~6 fine quanta, so an unchanged
        // fine pose means the animation has genuinely stopped.
        let fine = (
            (params.pose.yaw * FINE_Q).round() as i32,
            (params.pose.pitch * FINE_Q).round() as i32,
        );
        let quality = match self.state.last_pose {
            Some(p) if p == fine => Quality::Settle,
            _ => Quality::Motion,
        };
        self.state.last_pose = Some(fine);

        // The *coarse* one goes in the cache key while moving, which is how the
        // retransmit rate is throttled below the 20fps tick: consecutive ticks
        // usually land in the same bucket and reuse the frame already on
        // screen. Since the trace happens inside the transmit, a skipped frame
        // costs neither bytes nor CPU. A settled pose uses the fine key so the
        // one crisp frame is never deduped away.
        let key_pose = if quality == Quality::Settle {
            fine
        } else {
            (
                (params.pose.yaw * MOTION_Q).round() as i32,
                (params.pose.pitch * MOTION_Q).round() as i32,
            )
        };

        let target = raster::target_for(area.width, area.height, self.state.caps.cell_px, quality);
        let key = RichKey {
            cover: cover_hash(book),
            yaw_q: key_pose.0,
            pitch_q: key_pose.1,
            px_w: target.width,
            px_h: target.height,
            cols: area.width,
            rows: area.height,
            quality,
        };

        if self.state.sent != Some(key) {
            if !self.transmit(book, target, params, area) {
                return false;
            }
            self.state.sent = Some(key);
        }

        self.place(f, area);
        true
    }

    fn transmit(&mut self, book: &Book, target: Target, params: RenderParams, area: Rect) -> bool {
        // Cap the texture: past the render's own width it buys nothing, and a
        // huge cover is slow to decode and rescale.
        let texels = target.width.clamp(24, 2048);
        let img = {
            let cover = self.scene.cover(book, texels);
            let model = Model::new(book, cover);
            raster::render_rgba(target, &model, cover, params)
        };
        let esc = kitty::transmit(
            &img,
            self.state.id,
            area.width,
            area.height,
            self.state.caps.in_tmux,
        );
        if esc.is_empty() {
            return false;
        }
        if self.state.out.write_all(esc.as_bytes()).is_err() || self.state.out.flush().is_err() {
            return false;
        }
        kitty::register_live(self.state.id, self.state.caps.in_tmux);
        true
    }

    /// Paint the placeholder grid. These are ordinary cells, which is what
    /// makes every other part of the UI work unchanged: the header floats over
    /// the image simply by overwriting some of them, and tmux redraws them
    /// without knowing an image is involved.
    fn place(&mut self, f: &mut Frame, area: Rect) {
        let fg = kitty::id_color(self.state.id);
        let buf = f.buffer_mut();
        for row in 0..area.height {
            for col in 0..area.width {
                let symbol =
                    &self.state.placeholders[row as usize * area.width as usize + col as usize];
                buf[(area.x + col, area.y + row)]
                    .set_symbol(symbol)
                    .set_fg(fg)
                    // Reset, not black: transparent parts of the image must
                    // show the user's terminal background, exactly as the
                    // glyph path's `Color::Reset` subpixels do.
                    .set_bg(Color::Reset);
            }
        }
    }
}

impl BookPresenter for RichPresenter<'_> {
    fn draw_book(&mut self, f: &mut Frame, area: Rect, book: &Book, params: RenderParams) {
        if self.draw_pixels(f, area, book, params) {
            return;
        }
        // Any refusal above lands here: an incapable terminal, an oversized
        // rect, or a write that failed. Glyphs always work.
        GlyphPresenter::new(self.scene).draw_book(f, area, book, params);
    }
}

/// Pick the presenter for a mode, borrowing the shared scene and rich state.
pub fn presenter_for<'a>(
    mode: RenderMode,
    scene: &'a mut Scene,
    rich: &'a mut RichState,
) -> Box<dyn BookPresenter + 'a> {
    match mode {
        RenderMode::Glyph => Box::new(GlyphPresenter::new(scene)),
        RenderMode::Rich => Box::new(RichPresenter::new(scene, rich)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render3d::caps::Passthrough;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::sync::{Arc, Mutex};

    /// A writer that keeps everything, so tests can assert on the escapes.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Sink {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
        }
        fn len(&self) -> usize {
            self.0.lock().unwrap().len()
        }
    }

    fn capable() -> Caps {
        Caps {
            kitty_graphics: true,
            cell_px: (20, 38),
            cell_px_measured: true,
            in_tmux: false,
            passthrough: Passthrough::NotTmux,
        }
    }

    fn book() -> Book {
        Book {
            id: Some(1),
            title: Some("Station Eleven".into()),
            ..Book::default()
        }
    }

    fn scene() -> Scene {
        Scene::new(std::path::PathBuf::from("images"))
    }

    /// Draw the book once into a `w`x`h` terminal, returning the sink and the
    /// resulting buffer.
    fn draw_once(
        state: &mut RichState,
        scene: &mut Scene,
        w: u16,
        h: u16,
    ) -> ratatui::buffer::Buffer {
        let mut term = Terminal::new(TestBackend::new(w, h)).expect("backend");
        let area = Rect::new(0, 0, w, h);
        term.draw(|f| {
            let mut p = RichPresenter::new(scene, state);
            p.draw_book(f, area, &book(), RenderParams::default());
        })
        .expect("draw");
        term.backend().buffer().clone()
    }

    #[test]
    fn a_capable_terminal_gets_an_image_and_placeholder_cells() {
        let sink = Sink::default();
        let mut state = RichState::with_writer(capable(), Box::new(sink.clone()));
        let mut sc = scene();
        let buf = draw_once(&mut state, &mut sc, 20, 10);

        let esc = sink.text();
        assert!(esc.contains("a=T"), "no image was transmitted");
        assert!(esc.contains("U=1"), "placement was not virtual");
        assert!(esc.contains("c=20,r=10"), "wrong cell span");

        // Every cell of the rect carries the placeholder and the id colour.
        let fg = kitty::id_color(state.id);
        for y in 0..10 {
            for x in 0..20 {
                let cell = &buf[(x, y)];
                assert!(
                    cell.symbol().starts_with(kitty::PLACEHOLDER),
                    "cell {x},{y} is not a placeholder: {:?}",
                    cell.symbol()
                );
                assert_eq!(cell.fg, fg, "cell {x},{y} lost the image id");
            }
        }
    }

    #[test]
    fn an_incapable_terminal_emits_nothing_and_falls_back_to_glyphs() {
        let sink = Sink::default();
        let mut state = RichState::with_writer(Caps::default(), Box::new(sink.clone()));
        let mut sc = scene();
        let buf = draw_once(&mut state, &mut sc, 20, 10);

        assert_eq!(
            sink.len(),
            0,
            "escapes leaked to a terminal that can't take them"
        );
        let placeholders = (0..10)
            .flat_map(|y| (0..20).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol().starts_with(kitty::PLACEHOLDER))
            .count();
        assert_eq!(placeholders, 0, "placed an image with no protocol for it");
    }

    #[test]
    fn a_settled_pose_retransmits_once_then_goes_quiet() {
        let sink = Sink::default();
        let mut state = RichState::with_writer(capable(), Box::new(sink.clone()));
        let mut sc = scene();

        draw_once(&mut state, &mut sc, 20, 10);
        let after_first = sink.len();
        // Second draw at the same pose upgrades Motion -> Settle, so it costs
        // one more transmit...
        draw_once(&mut state, &mut sc, 20, 10);
        let after_settle = sink.len();
        assert!(after_settle > after_first, "settle frame never rendered");
        // ...and every draw after that is free.
        draw_once(&mut state, &mut sc, 20, 10);
        assert_eq!(
            sink.len(),
            after_settle,
            "a still book must not retransmit every tick"
        );
    }

    #[test]
    fn a_spinning_book_retransmits_well_below_the_tick_rate() {
        // The regression guard for "rich mode makes the terminal unusable":
        // every motion frame replaces the whole texture, so retransmitting on
        // every 20fps tick floods the terminal even though nothing blocks on
        // our side. Consecutive ticks must mostly reuse the frame on screen.
        const SPIN_SPEED: f32 = 0.01164; // must track app::SPIN_SPEED
        let sink = Sink::default();
        let mut state = RichState::with_writer(capable(), Box::new(sink.clone()));
        let mut sc = scene();
        let mut params = RenderParams::default();

        let ticks = 60;
        let mut transmits = 0;
        for _ in 0..ticks {
            params.pose.yaw += SPIN_SPEED;
            let before = sink.len();
            let mut term = Terminal::new(TestBackend::new(20, 10)).expect("backend");
            term.draw(|f| {
                let mut p = RichPresenter::new(&mut sc, &mut state);
                p.draw_book(f, Rect::new(0, 0, 20, 10), &book(), params);
            })
            .expect("draw");
            if sink.len() > before {
                transmits += 1;
            }
        }
        // ~1 transmit per 2.7 ticks. Allow slack, but fail loudly if it ever
        // creeps back toward one-per-tick.
        assert!(
            transmits <= ticks / 2,
            "{transmits} transmits in {ticks} ticks — the spin throttle is gone"
        );
        assert!(transmits > 0, "the spin stopped transmitting entirely");
    }

    #[test]
    fn a_resize_retransmits_with_the_new_span() {
        let sink = Sink::default();
        let mut state = RichState::with_writer(capable(), Box::new(sink.clone()));
        let mut sc = scene();
        draw_once(&mut state, &mut sc, 20, 10);
        draw_once(&mut state, &mut sc, 30, 12);
        let esc = sink.text();
        assert!(esc.contains("c=20,r=10"), "first span missing");
        assert!(esc.contains("c=30,r=12"), "resize did not re-place");
    }

    #[test]
    fn oversized_rects_fall_back_rather_than_clip() {
        // The diacritic table addresses 297 cells per axis; a wider rect has no
        // valid encoding, and a clipped image is worse than block glyphs.
        let sink = Sink::default();
        let mut state = RichState::with_writer(capable(), Box::new(sink.clone()));
        assert!(!state.ensure_placeholders(kitty::MAX_SPAN + 1, 10));
        assert!(state.ensure_placeholders(kitty::MAX_SPAN, 10));
    }

    #[test]
    fn dropping_the_image_emits_a_delete_and_forgets_it() {
        let sink = Sink::default();
        let mut state = RichState::with_writer(capable(), Box::new(sink.clone()));
        let mut sc = scene();
        draw_once(&mut state, &mut sc, 20, 10);
        state.drop_image();
        assert!(
            sink.text().contains(&format!("a=d,d=I,i={}", state.id)),
            "leaving rich mode must delete the image or it ghosts under the glyphs"
        );
        // And a second drop is a no-op rather than a second delete.
        let after = sink.len();
        state.drop_image();
        assert_eq!(sink.len(), after);
    }

    #[test]
    fn every_rect_size_draws_without_panicking() {
        // The rich equivalent of `every_screen_draws_at_every_size`: a panic in
        // the presenter wrecks the user's pane, so the degenerate sizes matter.
        let sink = Sink::default();
        let mut state = RichState::with_writer(capable(), Box::new(sink.clone()));
        let mut sc = scene();
        for (w, h) in [(1u16, 1u16), (1, 20), (20, 1), (3, 2), (40, 20)] {
            draw_once(&mut state, &mut sc, w, h);
        }
    }
}
