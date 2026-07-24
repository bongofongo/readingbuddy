//! The book presentation seam.
//!
//! [`BookPresenter`] is the one place the single-book view talks to a rendering
//! backend, so the backend is a real swap point. Today there is one working
//! backend, [`GlyphPresenter`] (the block-glyph raytrace that survives tmux);
//! [`RichPresenter`] is the stub the future out-of-tmux path grows into. Both
//! borrow the shared [`Scene`], so the caller reads what it needs out of `App`
//! first and hands us `&mut Scene`.

use ratatui::Frame;
use ratatui::layout::Rect;
use readingbuddy::Book;

use super::{RenderMode, RenderParams, Scene, blit};

/// Paints the book into a cell rect. `area` is the final inner region — any
/// border has already been drawn by the caller.
pub trait BookPresenter {
    fn draw_book(&mut self, f: &mut Frame, area: Rect, book: &Book, params: RenderParams);
}

/// The block-glyph raytrace: trace the cuboid into an [`super::RgbBuf`] and
/// quantize it to terminal cells. Works everywhere truecolor does, tmux
/// included — this is the only path selected inside tmux.
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

/// The higher-quality out-of-tmux backend.
///
/// STUB: for now it delegates to the glyph path, so `--render rich` degrades
/// gracefully. The kitty-graphics implementation lands here — it will reuse the
/// same cached [`Scene`] frames (raytraced at a higher supersample, emitted as
/// RGBA via the kitty graphics protocol) and place the image at `area`. See
/// `docs/rich-renderer.md` before fleshing it out.
pub struct RichPresenter<'a> {
    inner: GlyphPresenter<'a>,
}

impl<'a> RichPresenter<'a> {
    pub fn new(scene: &'a mut Scene) -> Self {
        RichPresenter {
            inner: GlyphPresenter::new(scene),
        }
    }
}

impl BookPresenter for RichPresenter<'_> {
    fn draw_book(&mut self, f: &mut Frame, area: Rect, book: &Book, params: RenderParams) {
        // Rich backend not implemented yet — fall back to the glyph path.
        self.inner.draw_book(f, area, book, params);
    }
}

/// Pick the presenter for a mode, borrowing the shared scene.
pub fn presenter_for(mode: RenderMode, scene: &mut Scene) -> Box<dyn BookPresenter + '_> {
    match mode {
        RenderMode::Glyph => Box::new(GlyphPresenter::new(scene)),
        RenderMode::Rich => Box::new(RichPresenter::new(scene)),
    }
}
