//! The ambient layer: a slow, quiet animation behind the non-book screens.
//!
//! At full size the menu and the list screens are mostly empty — a small box of
//! text marooned in a few thousand blank cells. This fills that space with
//! something that moves, without ever asking to be looked at.
//!
//! Three rules shape everything here:
//!
//! - **It never runs on the book screen.** The object is the centrepiece; a
//!   second moving thing beside it is noise. It also keeps the whole kitty
//!   hybrid — and its "no images while animating" invariant — out of scope.
//! - **It stays sparse.** Sparsity is what makes it cheap (see
//!   [`blit_sparse`](crate::render3d::blit::blit_sparse), whose cost argument
//!   rests on it) *and* what keeps it reading as texture rather than as a wall.
//!   Asserted by [`tests::both_motifs_stay_sparse`], not hoped for.
//! - **It redraws at its own rate, not the tick rate.** See [`AMBIENT_FPS`].

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::render3d::blit::{GlyphSet, RgbBuf, blit_sparse};
use crate::render3d::math::{Vec3, vec3};
use crate::theme;

/// How often the layer actually produces a new frame.
///
/// The app ticks at 20fps for the book; the ambient layer has no business
/// redrawing that often. A mote drifts about one cell every four seconds, and a
/// cell is four subpixel rows at octant density — so one subpixel step takes
/// ~1s and 4fps already gives four frames per step. Anything faster renders
/// frames the quantization cannot tell apart.
///
/// [`Ambient::advance`] reports a change only when the quantized frame index
/// moves, which is the same trick `present.rs` plays with pose buckets: quantize
/// whatever keys the output, and the ticks in between cost nothing at all.
const AMBIENT_FPS: f32 = 4.0;

/// Which ambient animation is running, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Motif {
    /// Nothing is drawn and no redraw is ever requested. The default: a reading
    /// app should not start animating at someone unasked.
    #[default]
    Off,
    /// Sparse points drifting slowly, winking in and out.
    Motes,
    /// Slowly morphing isolines through a scrolling noise field.
    Contours,
}

impl Motif {
    /// Cycle order for the settings screen.
    pub const ALL: [Motif; 3] = [Motif::Off, Motif::Motes, Motif::Contours];

    pub fn label(self) -> &'static str {
        match self {
            Motif::Off => "off",
            Motif::Motes => "motes",
            Motif::Contours => "contours",
        }
    }

    /// Parse a persisted label. An unknown value is not an error anywhere —
    /// callers fall back to [`Motif::Off`].
    pub fn from_label(s: &str) -> Option<Motif> {
        Motif::ALL.into_iter().find(|m| m.label() == s)
    }

    pub fn next(self) -> Motif {
        let i = Motif::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Motif::ALL[(i + 1) % Motif::ALL.len()]
    }
}

/// The layer's state: which motif, and how far into it we are.
#[derive(Debug, Clone, Default)]
pub struct Ambient {
    pub motif: Motif,
    /// Elapsed seconds. Wrapped (see [`Ambient::advance`]) so a session left
    /// open for days keeps its float precision.
    clock: f32,
    /// The quantized frame index the buffer currently reflects.
    bucket: u32,
}

/// The clock wraps here. Every motif is periodic in a few seconds, so the exact
/// value only has to be long enough that the seam is never noticed and short
/// enough that `clock` keeps subpixel precision as an f32.
const CLOCK_WRAP: f32 = 3600.0;

impl Ambient {
    pub fn new(motif: Motif) -> Ambient {
        Ambient {
            motif,
            clock: 0.0,
            bucket: 0,
        }
    }

    /// Advance by `dt` seconds. Returns true **only when the quantized frame
    /// changed** — this is the layer's dirty source, and it is the reason 20fps
    /// ticks do not become 20fps redraws.
    pub fn advance(&mut self, dt: f32) -> bool {
        if self.motif == Motif::Off {
            return false;
        }
        self.clock = (self.clock + dt) % CLOCK_WRAP;
        let bucket = (self.clock * AMBIENT_FPS) as u32;
        if bucket == self.bucket {
            return false;
        }
        self.bucket = bucket;
        true
    }

    /// The quantized frame the layer is currently showing. Development aid for
    /// `print_ambient`, which needs to drive the real clock to a known frame.
    #[cfg(test)]
    pub fn frame_index(&self) -> u32 {
        self.bucket
    }

    /// Composite the current frame into `area`. Draws nothing for
    /// [`Motif::Off`], and leaves every uncovered cell exactly as it found it.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, glyphs: GlyphSet) {
        if self.motif == Motif::Off || area.width == 0 || area.height == 0 {
            return;
        }
        // Snap to the quantized frame rather than the raw clock, so the drawn
        // frame matches the one `advance` reported — otherwise the layer would
        // creep between redraws and the quantization would buy nothing.
        let t = self.bucket as f32 / AMBIENT_FPS;
        let fb = match self.motif {
            Motif::Off => return,
            Motif::Motes => motes(area, glyphs, t),
            Motif::Contours => contours(area, glyphs, t),
        };
        blit_sparse(&fb, area, buf, glyphs);
    }
}

// ---- colour ---------------------------------------------------------------

/// The mid-tone everything is pulled toward.
const MID: Vec3 = vec3(0.5, 0.5, 0.5);

/// Ambient ink: the accent pulled most of the way to a mid-tone.
///
/// [`blit_sparse`] writes absolute RGB against a background it does not own,
/// and the app deliberately does not know whether the terminal is light or dark
/// — the whole `theme.rs` discipline (`Color::Reset`, `DarkGray`, `REVERSED`)
/// exists to avoid needing to. A tone near 0.5 luminance reads as a faint tint
/// on *both*; anything near black or white vanishes against one of them.
///
/// The consequence is worth stating plainly: **brightness cannot fade a mote to
/// invisible here**, because "invisible" is the terminal background and we have
/// no idea what it is. Fading toward the mid-tone just lands on a still-visible
/// grey. So `level` only modulates gently for depth, and motifs make things
/// disappear by dropping *coverage* instead.
fn ink(level: f32) -> Vec3 {
    let rgb = theme::accent_rgb();
    let accent = vec3(
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    );
    // A faint accent tint, then a gentle ±15% of depth around it.
    let tinted = MID + (accent - MID) * 0.35;
    let depth = 0.85 + 0.30 * level.clamp(0.0, 1.0);
    MID + (tinted - MID) * depth
}

// ---- hashing and noise ----------------------------------------------------

/// Integer avalanche hash. Deterministic and dependency-free: the layer must
/// look the same on every run so a resize or a screen change does not reshuffle
/// the whole field.
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// `hash` mapped into 0.0..1.0.
fn hash01(x: u32) -> f32 {
    hash(x) as f32 / u32::MAX as f32
}

fn hash2(x: i32, y: i32) -> f32 {
    hash01((x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B))
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise: hash the lattice corners and smoothstep between them.
fn noise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (smoothstep(x - ix), smoothstep(y - iy));
    let (ix, iy) = (ix as i32, iy as i32);
    let a = hash2(ix, iy);
    let b = hash2(ix + 1, iy);
    let c = hash2(ix, iy + 1);
    let d = hash2(ix + 1, iy + 1);
    let top = a + (b - a) * fx;
    let bottom = c + (d - c) * fx;
    top + (bottom - top) * fy
}

/// Two octaves — enough to stop the isolines reading as a regular lattice,
/// cheap enough to run per subpixel.
fn fbm(x: f32, y: f32) -> f32 {
    noise(x, y) * 0.65 + noise(x * 2.1 + 11.0, y * 2.1 + 7.0) * 0.35
}

// ---- motifs ---------------------------------------------------------------

/// Framebuffer sized to `area` at this glyph set's subpixel density.
fn buffer_for(area: Rect, glyphs: GlyphSet) -> RgbBuf {
    RgbBuf::new(area.width * 2, area.height * glyphs.cell_h())
}

/// One mote per ~70 cells, so the density reads the same on a small pane and a
/// full terminal instead of thinning out as the window grows. Roughly half are
/// winked out at any moment, so a 110x32 pane carries ~25 visible motes and a
/// 200x50 one ~70 — a dusting that catches the eye when it moves, nowhere near
/// enough to read as texture.
fn mote_count(area: Rect) -> u32 {
    ((area.width as u32 * area.height as u32) / 70).clamp(12, 160)
}

/// Sparse points drifting up and to the right, wrapping, each winking in and out
/// on its own slow cycle.
fn motes(area: Rect, glyphs: GlyphSet, t: f32) -> RgbBuf {
    let mut fb = buffer_for(area, glyphs);
    let (w, h) = (fb.width as f32, fb.height as f32);
    if w <= 0.0 || h <= 0.0 {
        return fb;
    }

    for i in 0..mote_count(area) {
        let seed = i.wrapping_mul(3);
        // Drift is in subpixels/second. A cell is 2 subpixels wide and
        // `cell_h()` tall, so this works out at roughly one cell every 4-8s.
        let vx = 0.15 + hash01(seed) * 0.35;
        let vy = 0.10 + hash01(seed + 1) * 0.30;
        let x = (hash01(seed + 2) * w + t * vx) % w;
        let y = (hash01(seed + 3) * h + h - (t * vy) % h) % h;

        // Wink in and out by coverage, not by brightness — see `ink`.
        let period = 9.0 + hash01(seed + 4) * 14.0;
        let phase = ((t / period) + hash01(seed + 5)) % 1.0;
        let wave = (phase * std::f32::consts::TAU).sin();
        if wave <= 0.0 {
            continue;
        }
        fb.set(x as u16, y as u16, Some(ink(wave)));
    }
    fb
}

/// How many isolines the field is sliced into.
const LEVELS: f32 = 5.0;

/// Spatial frequency of the field, per horizontal subpixel. Low on purpose:
/// this puts about three noise features across a wide terminal, so the curves
/// are long and lazy. Raise it and the screen fills with busy scribble.
const FREQ: f32 = 0.014;

/// How fast the field evolves, in noise units per second. One feature drifts
/// past over a couple of minutes.
const DRIFT: f32 = 0.010;

/// Half-width of a contour line, **in subpixels**.
///
/// Measuring the band in subpixels rather than in field units is what makes
/// these read as lines at all. A fixed threshold on `|f*LEVELS - round(...)|`
/// gives a band whose on-screen width is inversely proportional to the local
/// gradient: thin to the point of dashing where the field is steep, a blob
/// where it is flat. The first cut did exactly that and rendered as speckle.
/// Dividing by the gradient normalizes it, so every line is the same weight
/// wherever it runs.
const LINE_HALF_WIDTH: f32 = 0.7;

/// Cheap pre-filter, in field units: anything further than this from a level
/// cannot possibly be within `LINE_HALF_WIDTH` subpixels of one, for any
/// plausible gradient. Subpixels that fail it cost a single noise evaluation
/// instead of the three the gradient needs — which is most of them.
const NEAR_LEVEL: f32 = 0.25;

/// Contour lines are drawn in **one flat ink**, deliberately.
///
/// This is a cost control, not an aesthetic preference, and it is load-bearing.
/// ratatui's diff only skips a cell that is byte-identical to last frame. A
/// brightness ramp along the band gives each subpixel its own shade, and
/// `blit_sparse` averages a cell's lit subpixels — so as the field drifts a hair
/// each frame, every lit cell's averaged colour moves a fraction and the whole
/// terminal repaints. Measured at 200x50:
///
/// | ink | MB/s at 200x50 |
/// |---|---|
/// | continuous ramp | 0.419 |
/// | 3 quantized steps | 0.231 |
/// | **one flat ink** | **0.044** |
///
/// (All three at the original spatial tuning, so the comparison is like for
/// like; `FREQ` was lowered afterwards, which brought the shipped figure to
/// 0.019.) Comfortably under a glyph book spin, which is 0.131 MB/s on a full
/// pane. Re-measure with the ignored `ambient_wire_rate` sweep before touching
/// any of this.
///
/// With a single ink the mean of *any* subset of lit subpixels is that same
/// ink, so a cell's colour is exactly stable and only its glyph mask can
/// change — and at this drift rate a contour moves ~0.04 subpixels per frame,
/// so the masks are stable too. The shapes were never the cost; the colour was.
const FLAT_INK: f32 = 0.75;

/// Slowly morphing contour lines: slice a scrolling noise field at evenly
/// spaced levels and light whatever lands near a slice.
fn contours(area: Rect, glyphs: GlyphSet, t: f32) -> RgbBuf {
    let mut fb = buffer_for(area, glyphs);
    // Keep the noise isotropic on screen. A terminal cell is about twice as
    // tall as it is wide, and a cell holds 2 subpixel columns by `cell_h` rows,
    // so a subpixel's height:width ratio is `4 / cell_h` — exactly square at
    // octant, twice as tall at quadrant. The vertical frequency scales with
    // that, or the curves come out stretched (and at quadrant, badly).
    let fy = FREQ * (4.0 / glyphs.cell_h() as f32);

    // Sampling the field at a subpixel, with the slow drift folded in.
    let field = |x: f32, y: f32| fbm(x * FREQ, y * fy + t * DRIFT);

    for y in 0..fb.height {
        for x in 0..fb.width {
            let (px, py) = (x as f32, y as f32);
            let f = field(px, py);
            let v = f * LEVELS;
            let off = v - v.round();
            if off.abs() > NEAR_LEVEL {
                continue; // far from every level — one evaluation and out
            }

            // Finite differences over one subpixel, so the gradient is in the
            // same units as the distance we want.
            let gx = field(px + 1.0, py) - f;
            let gy = field(px, py + 1.0) - f;
            let grad = ((gx * gx + gy * gy).sqrt() * LEVELS).max(1e-6);
            if (off / grad).abs() < LINE_HALF_WIDTH {
                // One flat ink — see `FLAT_INK`. The line's softness comes from
                // subpixel coverage at its edges, which the glyph quantization
                // renders for free, rather than from a colour ramp that would
                // repaint the screen every frame.
                fb.set(x, y, Some(ink(FLAT_INK)));
            }
        }
    }
    fb
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lit subpixels and the framebuffer's total, for a motif at time `t`.
    fn lit(motif: Motif, area: Rect, glyphs: GlyphSet, t: f32) -> (usize, usize) {
        let fb = match motif {
            Motif::Off => return (0, 0),
            Motif::Motes => motes(area, glyphs, t),
            Motif::Contours => contours(area, glyphs, t),
        };
        let total = fb.width as usize * fb.height as usize;
        let on = (0..fb.height)
            .flat_map(|y| (0..fb.width).map(move |x| (x, y)))
            .filter(|(x, y)| fb.get(*x, *y).is_some())
            .count();
        (on, total)
    }

    /// What fraction of the framebuffer a motif lights.
    fn coverage(motif: Motif, area: Rect, glyphs: GlyphSet, t: f32) -> f32 {
        let (on, total) = lit(motif, area, glyphs, t);
        on as f32 / total as f32
    }

    #[test]
    fn the_ambient_clock_only_asks_for_a_redraw_at_its_own_rate() {
        // One second of 20fps ticks must yield exactly AMBIENT_FPS frames.
        let mut a = Ambient::new(Motif::Motes);
        let redraws = (0..20).filter(|_| a.advance(0.05)).count();
        assert_eq!(redraws, AMBIENT_FPS as usize);
    }

    #[test]
    fn the_clock_wraps_without_losing_a_beat() {
        // Land just short of the wrap and step over it: the bucket must still
        // change, so a long-lived session never stalls at the seam.
        let mut a = Ambient::new(Motif::Motes);
        a.clock = CLOCK_WRAP - 0.02;
        a.bucket = (a.clock * AMBIENT_FPS) as u32;
        assert!(a.advance(0.05));
        assert!(a.clock < 1.0, "clock should have wrapped, got {}", a.clock);
    }

    #[test]
    fn off_leaves_the_buffer_untouched() {
        let area = Rect::new(0, 0, 40, 12);
        let mut buf = Buffer::empty(area);
        for x in 0..area.width {
            buf[(x, 0)].set_symbol("X");
        }
        let before = buf.clone();

        let mut a = Ambient::new(Motif::Off);
        // Off never even asks for a redraw.
        assert!(!a.advance(1.0));
        a.draw(&mut buf, area, GlyphSet::Octant);
        assert_eq!(buf, before);
    }

    /// The invariant `blit_sparse`'s cost argument rests on. If a motif ever
    /// blankets the screen it stops being ambient and starts being a texture,
    /// and the layer stops being cheap.
    #[test]
    fn both_motifs_stay_sparse() {
        let area = Rect::new(0, 0, 200, 50);
        for motif in [Motif::Motes, Motif::Contours] {
            for glyphs in [GlyphSet::Quadrant, GlyphSet::Octant] {
                for frame in 0..200 {
                    let t = frame as f32 / AMBIENT_FPS;
                    let c = coverage(motif, area, glyphs, t);
                    assert!(
                        c < 0.15,
                        "{motif:?}/{glyphs:?} lit {:.1}% of subpixels at t={t}",
                        c * 100.0
                    );
                }
            }
        }
    }

    /// The other side of the sparsity test: a layer that lights nothing is
    /// sparse too, and useless. Counted in subpixels rather than as a fraction —
    /// motes are so few against a 400x200 subpixel field that any fraction
    /// threshold big enough to be meaningful is one they can never reach.
    #[test]
    fn both_motifs_actually_draw_something() {
        let area = Rect::new(0, 0, 200, 50);
        for motif in [Motif::Motes, Motif::Contours] {
            // Sampled across a spread of times: a motif must never go blank,
            // not merely be non-empty at one lucky instant.
            for frame in 0..40 {
                let t = frame as f32 * 0.7;
                let (on, _) = lit(motif, area, GlyphSet::Octant, t);
                assert!(on >= 8, "{motif:?} lit only {on} subpixels at t={t}");
            }
        }
    }

    #[test]
    fn motifs_survive_degenerate_areas() {
        // `every_screen_draws_at_every_size` goes down to 1x1; the layer must
        // not panic or divide by zero on the way.
        for (w, h) in [(1, 1), (2, 1), (1, 40), (0, 0)] {
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(Rect::new(0, 0, w.max(1), h.max(1)));
            for motif in [Motif::Motes, Motif::Contours] {
                Ambient::new(motif).draw(&mut buf, area, GlyphSet::Octant);
            }
        }
    }

    #[test]
    fn the_motif_cycle_returns_to_where_it_started() {
        let mut m = Motif::Off;
        for _ in 0..Motif::ALL.len() {
            m = m.next();
        }
        assert_eq!(m, Motif::Off);
        assert_eq!(Motif::from_label("contours"), Some(Motif::Contours));
        assert_eq!(Motif::from_label("nonsense"), None);
        // Every label round-trips, so a persisted setting always comes back.
        for motif in Motif::ALL {
            assert_eq!(Motif::from_label(motif.label()), Some(motif));
        }
    }
}
