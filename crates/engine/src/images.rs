//! Cover files on disk: where the bytes land, and what we measured about them.
//!
//! # Named by content, for the same reason `book_files` is
//!
//! Until item 20 a cover was named from the URL's last path segment. A Google
//! Books thumbnail URL is `.../books/content?id=…` — the last segment is the
//! literal string `content` for **every book Google has ever had a cover for**
//! — so every GB-sourced cover in a library wrote `images_dir/content` and the
//! last import won. Two books rendered each other's cover, permanently, with
//! nothing on either screen looking wrong. Epub extraction collided the same
//! way on `slugify(title)` (two editions of one book), and the fallback to the
//! literal `"cover.jpg"` when a URL had no last segment made a third collision
//! reachable rather than merely possible.
//!
//! The fix is the sha256 pattern [`crate::files`] already established for book
//! bytes (migration `0010`): **the name is the hash of the content**. Two
//! things fall out of that and both are load-bearing rather than incidental —
//! the write becomes **idempotent** (same bytes, same path, and re-storing them
//! is a no-op), and a caller can therefore ask "is this already here?" before
//! spending a request on it, which is what [`crate::Engine::fetch_cover`]'s
//! skip-if-present is.
//!
//! One consequence has to be handled rather than enjoyed: two books may now
//! legitimately share a file. `Storage::delete_book` and `Storage::merge_books`
//! both hand a caller a cover path to unlink, and under URL naming no other row
//! could be pointing at it. Under content naming one can, so both ask the
//! database first — see `a_shared_cover_survives_deleting_one_of_its_books`.
//!
//! # Two tiers
//!
//! Providers are asked for the largest cover they publish (item 20c), which is
//! the right file for a detail page and the wrong one for a shelf of three
//! hundred tiles. So a cover wider or taller than [`THUMB_MAX`] also gets a
//! downscaled sibling, named from the same hash. A cover already small enough
//! gets none, and [`crate::Book::shelf_cover_path`] is where "thumb, else the
//! original" is decided once rather than in each frontend.

use std::path::{Path, PathBuf};

use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use reqwest::Client;
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{EngineError, Result};

/// The longest edge a shelf tier keeps. A cover at or below this in both
/// dimensions is its own thumbnail and no second file is written.
///
/// 400 is two 200pt tiles on a 2× display, which is the largest a grid cell has
/// any use for; the hero shot reads the full-size file.
pub const THUMB_MAX: u32 = 400;

/// JPEG quality for the shelf tier. High enough that a downscaled jacket has no
/// visible ringing, low enough that the tier is worth writing.
const THUMB_QUALITY: u8 = 82;

/// What the engine measured about a cover file, at the moment it wrote it.
///
/// Not a provider's claim about the book and not attributable to one — see
/// migration `0014` for why these are not `MERGE_RULES` columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverMetrics {
    pub width: i64,
    pub height: i64,
    /// Median RGB of a 2px frame around the image, packed `0xRRGGBB`.
    ///
    /// **Unclamped.** `render3d` pushes it into a legible luma band so a white
    /// jacket still reads as a board against the page edges; that is a
    /// renderer's policy about its own lighting and not a fact about the file,
    /// so it stays in the renderer. This is the measurement.
    pub accent: i64,
}

/// A cover on disk, plus the shelf tier and the measurements — everything one
/// write produced, so a caller persists it in one statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverFile {
    pub path: PathBuf,
    /// The downscaled sibling, or `None` when the cover was already small
    /// enough to be its own — never a promise that decoding failed.
    pub thumb_path: Option<PathBuf>,
    /// `None` when the bytes are not an image this build can decode. The file
    /// is still stored: refusing to keep a cover because we could not measure
    /// it would lose the user's data over a diagnostic.
    pub metrics: Option<CoverMetrics>,
}

/// Where the shelf tier of `cover` lives, derived rather than stored.
///
/// The cover's name is a hash and carries exactly one `.`, so this is total and
/// reversible. It exists so a caller holding only a `cover_path` — the one
/// `Storage::delete_book` returns, the one `MergeReport::orphaned_cover`
/// carries — can clean up both files without the report growing a second field
/// for a name it can compute.
pub fn thumb_path_of(cover: &Path) -> PathBuf {
    let mut p = cover.to_path_buf();
    p.set_extension("thumb.jpg");
    p
}

/// The on-disk name for `bytes`: their sha256, and the extension of whatever
/// image format they actually are.
///
/// The guard below is kept, and kept reachable from a test, even though the
/// name is now a hash. Its original reason — *the name is attacker-influenced
/// (it comes from whatever URL a metadata provider handed us) and is about to
/// be joined onto a directory we own* — is weaker but not gone: what changed is
/// that both halves of the name are now closed sets (64 hex characters, and one
/// of a fixed list of extensions), so the property the guard asserts is
/// genuinely true rather than true because of what `Url::parse` happens to
/// normalize. "Safe because of what a dependency happens to do" was not a
/// property to leave unasserted on a path write, and "safe because a hash
/// cannot contain a slash" is not either.
fn filename_for(bytes: &[u8]) -> Result<String> {
    let hash = hex(Sha256::digest(bytes).as_slice());
    // A closed vocabulary: `guess_format` reads magic bytes and returns one of
    // its own variants, so nothing here is derived from a provider's URL. `img`
    // is the honest name for bytes we could not identify at all.
    let ext = image::guess_format(bytes)
        .ok()
        .and_then(|f| f.extensions_str().first().copied())
        .unwrap_or("img");
    let fname = format!("{hash}.{ext}");

    // Must be exactly one plain component — no separators, no `..`, no root.
    let p = Path::new(&fname);
    let plain = p.components().count() == 1 && p.file_name().is_some_and(|n| n == fname.as_str());
    if !plain {
        return Err(EngineError::InvalidInput(format!(
            "refusing image filename: {fname:?}"
        )));
    }
    Ok(fname)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Store cover bytes under their content hash, measure them, and write the
/// shelf tier where one is worth having.
///
/// **Idempotent.** The same bytes produce the same path, and a file already
/// there is not rewritten — so a re-import, a re-download and a merge of two
/// rows that fetched the same jacket all converge rather than racing.
pub fn store_cover(bytes: &[u8], images_dir: &Path) -> Result<CoverFile> {
    if bytes.is_empty() {
        return Err(EngineError::InvalidInput("empty image bytes".into()));
    }
    let fname = filename_for(bytes)?;
    std::fs::create_dir_all(images_dir)?;
    let path = images_dir.join(fname);
    if !path.exists() {
        std::fs::write(&path, bytes)?;
    }

    // A cover we cannot decode is still a cover. It is stored, it is unmeasured,
    // and `cover_width IS NULL` is the state the back-fill and every frontend
    // already have to handle.
    let Some(img) = decode(bytes) else {
        return Ok(CoverFile {
            path,
            thumb_path: None,
            metrics: None,
        });
    };
    let metrics = CoverMetrics {
        width: i64::from(img.width()),
        height: i64::from(img.height()),
        accent: accent_from_border(&img),
    };
    let thumb_path = write_thumb(&img, &path)?;
    Ok(CoverFile {
        path,
        thumb_path,
        metrics: Some(metrics),
    })
}

fn decode(bytes: &[u8]) -> Option<RgbImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgb8();
    (img.width() > 0 && img.height() > 0).then_some(img)
}

/// The shelf tier, or `None` when the cover is already no bigger than one.
fn write_thumb(img: &RgbImage, cover: &Path) -> Result<Option<PathBuf>> {
    if img.width() <= THUMB_MAX && img.height() <= THUMB_MAX {
        return Ok(None);
    }
    let dest = thumb_path_of(cover);
    if dest.exists() {
        return Ok(Some(dest));
    }
    // Lanczos on the way down, for `render3d/texture.rs`'s reason: a 1200px
    // jacket point-sampled to 400 aliases into noise exactly where the type is.
    let scale = f64::from(THUMB_MAX) / f64::from(img.width().max(img.height()));
    let w = ((f64::from(img.width()) * scale).round() as u32).max(1);
    let h = ((f64::from(img.height()) * scale).round() as u32).max(1);
    let small = image::imageops::resize(img, w, h, FilterType::Lanczos3);

    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, THUMB_QUALITY)
        .encode(small.as_raw(), w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| EngineError::Other(format!("encoding cover thumbnail: {e}")))?;
    std::fs::write(&dest, &out)?;
    Ok(Some(dest))
}

/// Median of the pixels in a 2px frame around the cover, packed `0xRRGGBB`.
///
/// The median rather than the mean because a jacket's border is usually one
/// flat colour interrupted by whatever bleeds off the artwork, and a mean lets
/// the interruption move the answer.
///
/// **The only copy.** Item 20 wrote this beside an identical loop in
/// `render3d/texture.rs` and said item 19 would delete that one; item 19
/// shipped and did not, and the duplicate then survived three handoffs behind a
/// second justification — that the two measured different images — which was
/// also false: the renderer measured its full-resolution decode of the same
/// file, before it scaled it. **Item 39 deleted it.** The renderer reads
/// `books.cover_accent` now.
///
/// What genuinely does not live here is the luma clamp, which is a renderer's
/// policy about its own lighting rather than a fact about the file — see
/// [`CoverMetrics::accent`] and `render3d/texture.rs`'s `ACCENT_LUMA`.
fn accent_from_border(img: &RgbImage) -> i64 {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let border = 2i64.min(w / 4).max(1);
    let mut chans: [Vec<u8>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for y in 0..h {
        for x in 0..w {
            let edge = x < border || y < border || x >= w - border || y >= h - border;
            if !edge {
                continue;
            }
            let p = img.get_pixel(x as u32, y as u32);
            for c in 0..3 {
                chans[c].push(p[c]);
            }
        }
    }
    let mut packed: i64 = 0;
    for chan in &mut chans {
        chan.sort_unstable();
        let med = if chan.is_empty() {
            128
        } else {
            chan[chan.len() / 2]
        };
        packed = (packed << 8) | i64::from(med);
    }
    packed
}

/// Split an accent back into its channels. The one place the packing is
/// undone, so two frontends cannot unpack it differently.
pub fn accent_channels(accent: i64) -> [u8; 3] {
    [
        ((accent >> 16) & 0xff) as u8,
        ((accent >> 8) & 0xff) as u8,
        (accent & 0xff) as u8,
    ]
}

/// Download an image URL into `images_dir`, named by content hash. Returns
/// everything the write produced.
pub async fn image_from_url(
    client: &Client,
    url_str: &str,
    images_dir: &Path,
) -> Result<CoverFile> {
    let url = Url::parse(url_str)?;
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if bytes.is_empty() {
        return Err(EngineError::Other(format!(
            "empty image response from {url_str}"
        )));
    }
    store_cover(&bytes, images_dir)
}

/// Re-measure a cover already on disk — the back-fill's per-file half.
///
/// Reads the bytes rather than trusting the name, because a file that predates
/// content naming is not named after its own hash. It writes nothing beside the
/// original except the shelf tier, and deliberately does **not** rename: the
/// stored `cover_path` is what a webview resolves, `docs/gui` documents its
/// shape, and a migration that moved every image would be a destructive change
/// dressed as a measurement.
pub fn measure_stored(cover: &Path) -> Result<CoverFile> {
    let bytes = std::fs::read(cover)?;
    let Some(img) = decode(&bytes) else {
        return Ok(CoverFile {
            path: cover.to_path_buf(),
            thumb_path: None,
            metrics: None,
        });
    };
    let metrics = CoverMetrics {
        width: i64::from(img.width()),
        height: i64::from(img.height()),
        accent: accent_from_border(&img),
    };
    let thumb_path = write_thumb(&img, cover)?;
    Ok(CoverFile {
        path: cover.to_path_buf(),
        thumb_path,
        metrics: Some(metrics),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb};

    /// A real PNG, so `guess_format` and the decoder both have something honest
    /// to read. `seed` changes the pixels, which changes the hash.
    fn png(w: u32, h: u32, seed: u8) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, y| {
            let edge = x < 2 || y < 2 || x + 2 >= w || y + 2 >= h;
            if edge {
                Rgb([160, 32, 32])
            } else {
                Rgb([seed, 250, 250])
            }
        });
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn a_name_is_the_hash_and_the_real_format() {
        let bytes = png(8, 8, 7);
        let name = filename_for(&bytes).unwrap();
        let (stem, ext) = name.split_once('.').unwrap();
        assert_eq!(stem.len(), 64, "{name}");
        assert!(stem.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
        // The extension follows the bytes, never the URL — a Google Books cover
        // arrives from a path segment called `content` with no extension at all.
        assert_eq!(ext, "png");
        assert_eq!(filename_for(b"not an image at all").unwrap().len(), 64 + 4);
    }

    /// The guard's reason has changed shape but not gone away: whatever a
    /// provider's URL says, the result must be a single filename that cannot
    /// climb out of `images_dir`. It is now provable rather than incidental —
    /// nothing in the name comes from the URL.
    #[test]
    fn a_remote_url_can_never_escape_the_images_dir() {
        let dir = Path::new("/data/images");
        for u in [
            "https://x.test/../../etc/passwd",
            "https://x.test/a/../../../../etc/passwd",
            "https://x.test/%2e%2e%2f%2e%2e%2fetc/passwd",
            "https://x.test/foo/%2Fetc%2Fpasswd",
            "https://books.google.com/books/content?id=../../../etc/passwd",
        ] {
            // The URL is parsed and then contributes nothing to the name; the
            // bytes do. Asserted rather than argued, since the argument is the
            // part that rots.
            assert!(Url::parse(u).is_ok());
            let name = filename_for(&png(4, 4, 1)).unwrap();
            let joined = dir.join(&name);
            assert_eq!(joined.parent(), Some(dir), "{u} produced {name:?}");
            assert!(!name.contains('/'), "{u} produced a separator: {name:?}");
            assert!(!name.contains(".."), "{u} produced a climb: {name:?}");
        }
    }

    /// **The bug this item exists to fix.** Two Google Books cover URLs
    /// differing only in query string — which is every pair of GB covers there
    /// has ever been — used to write one file, and the second book silently
    /// rendered the first one's jacket.
    ///
    /// Offline by construction: the collision was in the *naming*, so the test
    /// is about what two payloads are called, not about fetching them.
    #[test]
    fn two_google_books_covers_are_two_files() {
        let dir = tempfile::tempdir().unwrap();
        let images = dir.path().join("images");

        // The last path segment of both is the literal `content`.
        let one = "https://books.google.com/books/content?id=AAA&printsec=frontcover&img=1";
        let two = "https://books.google.com/books/content?id=BBB&printsec=frontcover&img=1";
        assert_eq!(
            Url::parse(one)
                .unwrap()
                .path_segments()
                .unwrap()
                .next_back(),
            Url::parse(two)
                .unwrap()
                .path_segments()
                .unwrap()
                .next_back(),
            "the premise: these two URLs used to name the same file"
        );

        let a = store_cover(&png(8, 8, 10), &images).unwrap();
        let b = store_cover(&png(8, 8, 200), &images).unwrap();
        assert_ne!(a.path, b.path, "two covers, one filename");
        assert!(a.path.exists() && b.path.exists());
        assert_eq!(
            std::fs::read_dir(&images).unwrap().count(),
            2,
            "two books, two covers on disk"
        );

        // …and the URL-less collision too: an epub whose title slugifies to the
        // same stem as another edition's.
        let c = store_cover(&png(9, 9, 10), &images).unwrap();
        assert_ne!(a.path, c.path);
    }

    #[test]
    fn storing_the_same_bytes_twice_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let images = dir.path().join("images");
        let bytes = png(600, 900, 3);

        let first = store_cover(&bytes, &images).unwrap();
        let mtime = std::fs::metadata(&first.path).unwrap().modified().unwrap();
        let second = store_cover(&bytes, &images).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            std::fs::metadata(&second.path).unwrap().modified().unwrap(),
            mtime,
            "the file was rewritten, so the write is not idempotent"
        );
    }

    #[test]
    fn a_measured_cover_carries_its_size_and_its_border_colour() {
        let dir = tempfile::tempdir().unwrap();
        let stored = store_cover(&png(60, 90, 0), &dir.path().join("images")).unwrap();
        let m = stored.metrics.expect("a png is decodable");
        assert_eq!((m.width, m.height), (60, 90));

        // The frame is (160, 32, 32) and the middle is near-white: the accent
        // must follow the border, not the field.
        let [r, g, b] = accent_channels(m.accent);
        assert_eq!([r, g, b], [160, 32, 32]);
        // Unclamped — the luma band belongs to the renderer.
        assert_eq!(m.accent, 0x00a0_2020);
    }

    #[test]
    fn only_a_cover_bigger_than_the_shelf_gets_a_second_tier() {
        let dir = tempfile::tempdir().unwrap();
        let images = dir.path().join("images");

        let small = store_cover(&png(120, 180, 1), &images).unwrap();
        assert_eq!(small.thumb_path, None, "a small cover is its own thumbnail");

        let big = store_cover(&png(800, 1200, 2), &images).unwrap();
        let thumb = big.thumb_path.expect("a large cover gets a shelf tier");
        assert_eq!(thumb, thumb_path_of(&big.path));
        assert!(thumb.exists());
        // Fits the box, keeps the aspect, and is genuinely smaller on disk.
        let t = image::open(&thumb).unwrap();
        assert_eq!((t.width(), t.height()), (267, 400));
        assert!(
            std::fs::metadata(&thumb).unwrap().len() < std::fs::metadata(&big.path).unwrap().len()
        );
    }

    #[test]
    fn bytes_that_are_not_an_image_are_still_kept() {
        let dir = tempfile::tempdir().unwrap();
        let stored = store_cover(b"<svg/>", &dir.path().join("images")).unwrap();
        assert!(
            stored.path.exists(),
            "a cover we cannot read is not deleted"
        );
        assert_eq!(stored.metrics, None);
        assert_eq!(stored.thumb_path, None);
    }

    /// The back-fill's per-file half, on a file named the old way: it measures
    /// what is there and does not rename it.
    #[test]
    fn measuring_a_file_in_place_leaves_its_name_alone() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("content");
        std::fs::write(&legacy, png(500, 700, 4)).unwrap();

        let measured = measure_stored(&legacy).unwrap();
        assert_eq!(measured.path, legacy, "the stored path must not move");
        assert_eq!(measured.metrics.unwrap().width, 500);
        assert!(measured.thumb_path.unwrap().exists());
    }
}
