//! Software renderer for the book object.
//!
//! Pipeline: [`Scene::frame`] → ray-traced cuboid ([`scene`]) → an [`RgbBuf`]
//! of subpixels → half-block terminal cells ([`blit`]). Nothing here knows
//! about ratatui except the blit step, so the presentation layer is a swap
//! point if a pixel protocol ever becomes viable.

pub mod blit;
pub mod caps;
pub mod kitty;
pub mod math;
pub mod present;
pub mod raster;
pub mod scene;
pub mod texture;

use std::path::{Path, PathBuf};

use readingbuddy::{Book, EditionShape};

pub use blit::{GlyphSet, RgbBuf};
pub use caps::Caps;
pub use kitty::ImageWire;
pub use present::presenter_for;
pub use scene::Pose;

/// Which presentation path the single-book view uses.
///
/// `Glyph` is the block-glyph raytrace: it works anywhere truecolor does, tmux
/// included, and is the fallback for every situation the probe can't improve
/// on. `Rich` is the **hybrid**: block glyphs while the book animates, true
/// pixels (kitty graphics) the moment it parks — chosen when
/// [`Caps::supports_pixels`] says the terminal can take them, which, contrary to
/// the original design, includes tmux.
///
/// The hybrid is not a compromise, it is the finding: a byte of image costs a
/// terminal far more than a byte of text (decompress, re-upload the whole
/// texture, recomposite), so no resolution or throttle setting makes an animated
/// pixel book as cheap as glyphs. The only way to be light while animating is to
/// send no images while animating. See `docs/rich-renderer.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    #[default]
    Glyph,
    Rich,
    /// Pixels even while the book is moving — **the known-bad configuration**.
    ///
    /// Kept deliberately, and reachable only by an explicit flag, because the
    /// terminal-latency instrument needs something that is definitely pathological
    /// to be calibrated against. An instrument that has never seen the failure it
    /// exists to catch is not evidence of anything.
    RichAlways,
}

impl RenderMode {
    /// Whether this mode may put pixels on screen at all.
    pub fn is_rich(self) -> bool {
        matches!(self, RenderMode::Rich | RenderMode::RichAlways)
    }

    /// Short name for the perf log and the bench table.
    pub fn label(self) -> &'static str {
        match self {
            RenderMode::Glyph => "glyph",
            RenderMode::Rich => "rich",
            RenderMode::RichAlways => "rich-always",
        }
    }
}

use math::{Vec3, vec3};
use texture::Cover;

/// Knobs the UI can turn without touching the renderer internals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderParams {
    pub pose: Pose,
    /// Supersampling factor per axis; 3 means 9 rays per subpixel.
    pub ss: u8,
    /// Block-glyph family, which fixes the subpixel resolution of a cell.
    pub glyphs: GlyphSet,
    /// Whether the book is animating right now.
    ///
    /// The pixel path can *infer* this by comparing poses between draws, and did
    /// so originally to avoid any plumbing from the event loop. That inference
    /// cannot see the case the hybrid renderer depends on: when the book parks,
    /// the app stops redrawing entirely (`App::tick` returns false, `dirty` is
    /// never set), so there is no second draw in which to notice it parked — and
    /// the crisp frame would never be transmitted.
    ///
    /// `None` keeps the old inference, which is what `--dump-frame`, the bench
    /// and the unit tests use.
    pub moving: Option<bool>,
}

impl Default for RenderParams {
    fn default() -> Self {
        RenderParams {
            pose: Pose::default(),
            // Octants double the vertical subpixel density; a slightly higher
            // supersample keeps the finer edges from aliasing.
            ss: 3,
            glyphs: GlyphSet::Octant,
            moving: None,
        }
    }
}

/// This scene's half-extents for one edition — [`readingbuddy::EditionShape`]
/// scaled into scene units.
///
/// The *decision* about what shape a book is moved to the engine in item 19, so
/// that a WebGL shelf and this ray tracer agree about how fat *Infinite Jest*
/// is. What is left here is the scaling, which is the renderer's alone:
/// `EditionShape` states proportions with height fixed at 1.0, and
/// [`scene::HALF_HEIGHT`] is this camera rig's idea of how tall that is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Model {
    pub half: Vec3,
}

impl Model {
    /// Ask the engine for the edition's proportions, then scale them by this
    /// scene's height.
    ///
    /// The aspect comes from item 20's **stored** cover dimensions where they
    /// exist, and falls back to `cover.aspect` — a *decoded* image's
    /// width/height — where they do not. That fallback is not vestigial: the
    /// column is `None` for every cover written before `0014` and for every
    /// cover a back-fill has not reached, and this renderer already holds the
    /// decoded image for the one book it draws. A shelf holds three hundred and
    /// must not decode any of them, which is what the column is for.
    pub fn new(book: &Book, cover: &Cover) -> Model {
        let aspect = book
            .cover_aspect()
            .map(|a| a as f32)
            .unwrap_or(cover.aspect);
        let shape = EditionShape::of_book(book, Some(aspect));
        let h = scene::HALF_HEIGHT;
        Model {
            half: vec3(
                h * shape.width_over_height,
                h,
                h * shape.thickness_over_height,
            ),
        }
    }
}

/// Identifies which cover is loaded, so the cache knows when to rebuild. The
/// last field is the texture's target width in texels — the pixel path wants a
/// much larger texture than the glyph path for the same cell rect.
type CoverKey = (Option<i64>, Option<String>, String, u32);

/// `cover_path` as written by the engine, falling back to the images dir when
/// the stored (relative) path doesn't resolve from the current cwd.
///
/// A free function, and the **single authority** on whether a book has a real
/// cover. The bench used to answer that question with its own
/// `images_dir.join(stored)` and got it wrong: the engine stores paths relative
/// to the *data root* (`./database/images/x.jpg`), so joining them onto the
/// images dir yields `database/images/./database/images/x.jpg`. The bench then
/// warned "no cover image on disk" for books the renderer was loading fine.
pub fn resolve_cover(images_dir: &Path, stored: &str) -> Option<PathBuf> {
    let direct = Path::new(stored);
    if direct.exists() {
        return Some(direct.to_path_buf());
    }
    let by_name = images_dir.join(direct.file_name()?);
    by_name.exists().then_some(by_name)
}

/// Texture width the glyph path asks for: four subpixels of texture per cell,
/// enough detail for the front face at any pose without paying for the full
/// JPEG. Kept as a function so the key stays a pure function of `cols`.
fn glyph_texels(cols: u16) -> u32 {
    (cols as u32 * 4).max(24)
}

/// Owns the cover texture and the last rendered frame. Both are cached: a
/// still book costs nothing per tick, and rotating only re-runs the tracer.
pub struct Scene {
    /// Cover paths are stored relative to the data root, so they are resolved
    /// against the engine's images directory before being opened.
    images_dir: PathBuf,
    cover: Option<(CoverKey, Cover)>,
    frame: Option<(FrameKey, RgbBuf)>,
}

/// Everything a cached glyph frame depends on. `ss` and `glyphs` belong here as
/// much as the pose does: `glyphs` fixes the framebuffer's height, so without it
/// the runtime glyph toggle can serve a frame of the wrong cell height for one
/// draw.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameKey {
    cover: CoverKey,
    /// Pose quantized to 1/512 rad, so sub-degree jitter can't force redraws.
    yaw_q: i32,
    pitch_q: i32,
    cols: u16,
    rows: u16,
    ss: u8,
    glyphs: GlyphSet,
}

impl Scene {
    pub fn new(images_dir: impl Into<PathBuf>) -> Scene {
        Scene {
            images_dir: images_dir.into(),
            cover: None,
            frame: None,
        }
    }

    fn resolve_cover(&self, stored: &str) -> Option<PathBuf> {
        resolve_cover(&self.images_dir, stored)
    }

    fn cover_key(book: &Book, texels: u32) -> CoverKey {
        (
            book.id,
            book.cover_path.clone(),
            book.display_title().to_string(),
            texels,
        )
    }

    /// The cover texture for `book`, decoded (or synthesized) at roughly
    /// `texels` wide unless the cached one already matches.
    ///
    /// Public so the pixel path can ask for a bigger texture than the glyph
    /// path does: a 1000px-wide render wants far more than four texels per
    /// cell. Both paths share the one cache slot, so switching renderers
    /// reloads the cover — cheap, and it keeps `Scene` to a single texture.
    pub fn cover(&mut self, book: &Book, texels: u32) -> &Cover {
        let key = Self::cover_key(book, texels);
        let stale = self.cover.as_ref().map(|(k, _)| k != &key).unwrap_or(true);
        if stale {
            let loaded = book
                .cover_path
                .as_deref()
                .and_then(|p| self.resolve_cover(p))
                .and_then(|p| texture::load_cover(&p, texels))
                .unwrap_or_else(|| texture::procedural_cover(book.display_title()));
            self.cover = Some((key, loaded));
        }
        &self.cover.as_ref().expect("just populated").1
    }

    /// Render `book` at `pose` for a `cols` x `rows` region of terminal cells.
    /// The returned buffer is twice that on both axes — four subpixels per
    /// cell, which [`blit`] quantizes into one block glyph.
    pub fn frame(&mut self, book: &Book, cols: u16, rows: u16, params: RenderParams) -> &RgbBuf {
        let key = FrameKey {
            cover: Self::cover_key(book, glyph_texels(cols)),
            yaw_q: (params.pose.yaw * 512.0).round() as i32,
            pitch_q: (params.pose.pitch * 512.0).round() as i32,
            cols,
            rows,
            ss: params.ss,
            glyphs: params.glyphs,
        };
        let hit = self.frame.as_ref().map(|(k, _)| k == &key).unwrap_or(false);
        if !hit {
            // Scoped so the cover borrow ends before `frame` is reassigned.
            let fb = {
                let cover = self.cover(book, glyph_texels(cols));
                render(cols, rows, &Model::new(book, cover), cover, params)
            };
            self.frame = Some((key, fb));
        }
        &self.frame.as_ref().expect("just populated").1
    }
}

/// Trace one frame for a `cols` x `rows` cell region. Public so `--dump-frame`
/// and tests can call it directly.
pub fn render(cols: u16, rows: u16, model: &Model, cover: &Cover, params: RenderParams) -> RgbBuf {
    // Two subpixel columns per cell; the row count follows the glyph family
    // (octants pack twice as many, which is where the extra resolution comes
    // from). The physical aspect below stays cols : rows*2 either way.
    let (width, height) = (cols * 2, rows * params.glyphs.cell_h());
    let mut fb = RgbBuf::new(width, height);
    if cols == 0 || rows == 0 {
        return fb;
    }
    let rot = params.pose.rotation();
    // A cell is one unit wide and two tall, so the image's physical aspect is
    // cols : rows*2 even though the sample grid is square in count.
    let aspect = cols as f32 / (rows as f32 * 2.0);
    let origin = scene::camera_origin(aspect, params.pose.pitch, model.half, rows);
    let ss = params.ss.max(1) as u16;
    let samples = (ss * ss) as f32;
    let (fw, fh) = (width as f32, height as f32);

    for y in 0..height {
        for x in 0..width {
            let mut sum = Vec3::ZERO;
            let mut hits = 0.0f32;
            for sy in 0..ss {
                for sx in 0..ss {
                    let u = (x as f32 + (sx as f32 + 0.5) / ss as f32) / fw;
                    let v = (y as f32 + (sy as f32 + 0.5) / ss as f32) / fh;
                    let dir = scene::primary_ray(u, v, aspect);
                    if let Some(c) = scene::shade(origin, dir, rot, model.half, cover) {
                        sum = sum + c;
                        hits += 1.0;
                    }
                }
            }
            // The terminal background is unknown, so partial coverage can't be
            // alpha-blended — a majority of hits claims the subpixel. The
            // glyph chooser then resolves coverage at quarter-cell precision.
            if hits * 2.0 >= samples {
                fb.set(x, y, Some(sum / hits));
            }
        }
    }
    fb
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled_subpixels(fb: &RgbBuf) -> usize {
        (0..fb.height)
            .flat_map(|y| (0..fb.width).map(move |x| (x, y)))
            .filter(|(x, y)| fb.get(*x, *y).is_some())
            .count()
    }

    fn test_book() -> Book {
        Book {
            id: Some(1),
            title: Some("Station Eleven".into()),
            ..Book::default()
        }
    }

    /// Item 19 moved the derivation into the engine and the renderer is frozen,
    /// so the numbers must not have moved with it. This reproduces the four
    /// lines `Model::new` used to hold, byte for byte, and compares.
    ///
    /// It is also the only place the scene-unit ↔ height-ratio conversion is
    /// checked: `EditionShape` speaks in multiples of height, this scene's
    /// half-height is 0.75, and getting that factor wrong would change every
    /// spine's thickness by a third with nothing failing.
    #[test]
    fn the_engines_shape_reproduces_the_renderers_old_arithmetic() {
        fn historical(book: &Book, cover: &Cover) -> Vec3 {
            let h = scene::HALF_HEIGHT;
            let width = h * cover.aspect.clamp(0.55, 0.85);
            // The old `unwrap_or(320)`, kept here as it was: the *only* input
            // where the two disagree is a non-positive page count, which the
            // engine now reads as absence rather than as a 48-page pamphlet.
            let pages = book.page_count.unwrap_or(320).clamp(48, 1400) as f32;
            let depth = (0.045 + pages / 9000.0).clamp(0.05, 0.20);
            vec3(width, h, depth)
        }
        let cover = texture::procedural_cover("Station Eleven");
        for pages in [None, Some(1), Some(48), Some(320), Some(1408), Some(9999)] {
            let book = Book {
                page_count: pages,
                ..test_book()
            };
            let old = historical(&book, &cover);
            let new = Model::new(&book, &cover).half;
            for (a, b, axis) in [
                (old.x, new.x, "width"),
                (old.y, new.y, "height"),
                (old.z, new.z, "depth"),
            ] {
                assert!(
                    (a - b).abs() < 1e-6,
                    "{axis} moved for {pages:?}: {a} -> {b}"
                );
            }
        }
        // `scene::DEFAULT_HALF_EXTENTS` is the reference trade paperback the
        // scene tests are written against; the procedural cover is 48x72, so
        // this is the same object.
        let paperback = Model::new(
            &Book {
                page_count: Some(320),
                ..test_book()
            },
            &cover,
        );
        assert!((paperback.half.x - scene::DEFAULT_HALF_EXTENTS.x).abs() < 1e-6);
        assert!((paperback.half.y - scene::DEFAULT_HALF_EXTENTS.y).abs() < 1e-6);
    }

    /// The item 19 → item 20 rewire: where the columns exist, the *stored*
    /// measurement decides the shape and the decoded texture does not get a
    /// vote. Asserted with the two deliberately disagreeing, because agreeing
    /// inputs cannot tell which one was read — and the decode is what a shelf
    /// of three hundred spines cannot afford.
    #[test]
    fn a_measured_cover_is_not_decoded_to_find_its_aspect() {
        let cover = texture::procedural_cover("Station Eleven");
        let measured = Book {
            // Deliberately unlike the 48x72 procedural cover, and inside the
            // 0.55..0.85 clamp at both ends so neither collapses onto it.
            cover_width: Some(170),
            cover_height: Some(200),
            ..test_book()
        };
        let stored = Model::new(&measured, &cover).half.x;
        let decoded = Model::new(&test_book(), &cover).half.x;
        assert!(
            (stored - scene::HALF_HEIGHT * 0.85).abs() < 1e-6,
            "the stored 170x200 should decide the width, got {stored}"
        );
        assert!(
            (stored - decoded).abs() > 1e-3,
            "the two inputs must disagree or this test proves nothing"
        );
    }

    /// An unmeasured cover — every row written before `0014`, and every one a
    /// back-fill has not reached — still decodes. The fallback is the state of
    /// the library, not a leftover.
    #[test]
    fn an_unmeasured_cover_still_falls_back_to_the_decoded_image() {
        let cover = texture::procedural_cover("Station Eleven");
        let half_measured = Book {
            cover_width: Some(170),
            cover_height: None,
            ..test_book()
        };
        assert_eq!(
            Model::new(&half_measured, &cover).half.x,
            Model::new(&test_book(), &cover).half.x,
        );
    }

    /// The behaviour item 19 *did* change, isolated so it cannot be mistaken
    /// for drift. `page_count = 0` is a real row in `make dev-db`; the old
    /// arithmetic clamped it to 48 and drew a book of unknown length as the
    /// thinnest pamphlet the model allows.
    #[test]
    fn a_zero_page_count_no_longer_draws_the_thinnest_possible_book() {
        let cover = texture::procedural_cover("Station Eleven");
        let with = |pages| {
            Model::new(
                &Book {
                    page_count: pages,
                    ..test_book()
                },
                &cover,
            )
            .half
            .z
        };
        assert_eq!(with(Some(0)), with(None), "zero is absence, not a length");
        assert!(with(Some(0)) > with(Some(48)));
    }

    #[test]
    fn renders_a_silhouette_with_empty_margins() {
        let book = test_book();
        let cover = texture::procedural_cover("Station Eleven");
        let model = Model::new(&book, &cover);
        let fb = render(60, 40, &model, &cover, RenderParams::default());
        // Octant default: 2 subpixels wide, 4 tall per cell.
        assert_eq!(
            (fb.width, fb.height),
            (120, 160),
            "eight subpixels per cell"
        );
        assert!(fb.get(60, 80).is_some(), "centre should be the book");
        assert!(fb.get(0, 0).is_none(), "corner should be empty");
        let filled = filled_subpixels(&fb);
        let total = fb.width as usize * fb.height as usize;
        assert!(
            filled > total / 25 && filled < total / 2,
            "silhouette covered {filled}/{total}"
        );
    }

    #[test]
    fn missing_cover_falls_back_without_touching_the_disk() {
        let mut scene = Scene::new("database/images");
        let mut book = test_book();
        book.cover_path = Some("/nonexistent/cover.jpg".into());
        let fb = scene.frame(&book, 40, 30, RenderParams::default());
        assert!(
            filled_subpixels(fb) > 200,
            "fallback cover rendered nothing"
        );
    }

    #[test]
    fn frames_are_cached_until_the_pose_moves() {
        let mut scene = Scene::new("database/images");
        let book = test_book();
        let params = RenderParams::default();
        scene.frame(&book, 40, 30, params);
        let first = scene.frame.as_ref().unwrap().0.yaw_q;
        scene.frame(&book, 40, 30, params);
        assert_eq!(scene.frame.as_ref().unwrap().0.yaw_q, first);

        let moved = RenderParams {
            pose: Pose {
                yaw: params.pose.yaw + 0.5,
                ..params.pose
            },
            ..params
        };
        scene.frame(&book, 40, 30, moved);
        assert_ne!(scene.frame.as_ref().unwrap().0.yaw_q, first);
    }

    #[test]
    fn switching_glyph_family_invalidates_the_cached_frame() {
        // The framebuffer's height is `rows * glyphs.cell_h()`, so a cached
        // octant frame is the wrong shape for a quadrant blit. Before `glyphs`
        // joined the key, toggling families served one frame at the old height.
        let mut scene = Scene::new("database/images");
        let book = test_book();
        let octant = RenderParams {
            glyphs: GlyphSet::Octant,
            ..RenderParams::default()
        };
        let tall = scene.frame(&book, 40, 30, octant).height;
        let quadrant = RenderParams {
            glyphs: GlyphSet::Quadrant,
            ..octant
        };
        let short = scene.frame(&book, 40, 30, quadrant).height;
        assert_eq!(tall, 30 * 4);
        assert_eq!(short, 30 * 2, "cache served a frame of the wrong height");
    }

    /// What the block-glyph raster costs to trace, the counterpart to
    /// `raster::raster_cost`.
    ///
    /// Worth having next to it because the two are *not* comparable line for
    /// line: this path is still single-threaded, where the pixel raster splits
    /// across scoped threads. That gap is the headroom any future glyph quality
    /// work (higher supersample, edge refine) would spend, so it is the number
    /// to look at before starting.
    ///
    /// Only meaningful on a **release** build — debug is ~30x slower.
    ///
    /// `cargo test --release -p readingbuddy-tui -- --ignored --nocapture glyph_cost`
    #[test]
    #[ignore = "timing, not correctness; run on a release build"]
    fn glyph_cost() {
        let cover = texture::procedural_cover("Station Eleven");
        let model = Model::new(&test_book(), &cover);
        println!(
            "{:<22} {:>9} {:>12} {:>10}",
            "case", "cells", "framebuffer", "trace"
        );
        for (label, cols, rows, set, ss) in [
            ("book rect, octant", 50u16, 26u16, GlyphSet::Octant, 3u8),
            ("book rect, quadrant", 50, 26, GlyphSet::Quadrant, 3),
            ("book rect, octant ss4", 50, 26, GlyphSet::Octant, 4),
            ("full pane, octant", 120, 40, GlyphSet::Octant, 3),
        ] {
            let params = RenderParams {
                glyphs: set,
                ss,
                ..RenderParams::default()
            };
            // Best of N, not the mean: this competes with everything else on the
            // machine, and the floor is the honest figure for "what does this
            // cost when it gets the CPU".
            let mut best = std::time::Duration::MAX;
            for _ in 0..5 {
                let start = std::time::Instant::now();
                let fb = render(cols, rows, &model, &cover, params);
                let each = start.elapsed();
                std::hint::black_box(&fb);
                best = best.min(each);
            }
            println!(
                "{label:<22} {:>9} {:>12} {:>10?}",
                format!("{cols}x{rows}"),
                format!("{}x{}", cols as u32 * 2, rows as u32 * set.cell_h() as u32),
                best
            );
        }
        println!("(budget is 50ms/frame at the 20fps tick)");
    }
}
