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

use readingbuddy::Book;

pub use blit::{GlyphSet, RgbBuf};
pub use caps::Caps;
pub use present::presenter_for;
pub use scene::Pose;

/// Which presentation path the single-book view uses.
///
/// `Glyph` is the block-glyph raytrace: it works anywhere truecolor does, tmux
/// included, and is the fallback for every situation the probe can't improve
/// on. `Rich` is the true-pixel path (kitty graphics), chosen when
/// [`Caps::supports_pixels`] says the terminal can actually take it — which,
/// contrary to the original design, includes tmux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    #[default]
    Glyph,
    Rich,
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
}

impl Default for RenderParams {
    fn default() -> Self {
        RenderParams {
            pose: Pose::default(),
            // Octants double the vertical subpixel density; a slightly higher
            // supersample keeps the finer edges from aliasing.
            ss: 3,
            glyphs: GlyphSet::Octant,
        }
    }
}

/// The physical shape of one edition. Deriving it from the book rather than
/// hard-coding a box is what stops covers being stretched onto the wrong
/// aspect, and it gives a doorstop a visibly fatter spine than a novella.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Model {
    pub half: Vec3,
}

impl Model {
    /// Width follows the cover's aspect (clamped to shapes books actually
    /// come in); thickness follows the page count, defaulting to a 320-page
    /// paperback when the count is unknown.
    pub fn new(book: &Book, cover: &Cover) -> Model {
        let h = scene::HALF_HEIGHT;
        let width = h * cover.aspect.clamp(0.55, 0.85);
        let pages = book.page_count.unwrap_or(320).clamp(48, 1400) as f32;
        let depth = (0.045 + pages / 9000.0).clamp(0.05, 0.20);
        Model {
            half: vec3(width, h, depth),
        }
    }
}

/// Identifies which cover is loaded, so the cache knows when to rebuild. The
/// last field is the texture's target width in texels — the pixel path wants a
/// much larger texture than the glyph path for the same cell rect.
type CoverKey = (Option<i64>, Option<String>, String, u32);

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

    /// `cover_path` as written by the engine, falling back to the images dir
    /// when the stored (relative) path doesn't resolve from the current cwd.
    fn resolve_cover(&self, stored: &str) -> Option<PathBuf> {
        let direct = Path::new(stored);
        if direct.exists() {
            return Some(direct.to_path_buf());
        }
        let by_name = self.images_dir.join(direct.file_name()?);
        by_name.exists().then_some(by_name)
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
}
