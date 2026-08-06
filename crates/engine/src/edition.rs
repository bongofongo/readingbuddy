//! What shape is this edition — as proportions, once.
//!
//! Item 19, and the second instance of the rule item 17 established: *a
//! [`crate::Progress`] enum is not terminal I/O; `"p.42"` is.* Its version:
//! **proportions are not rendering; a Bézier spine highlight is.**
//!
//! Four lines of arithmetic lived in `crates/tui/src/render3d/mod.rs`'s
//! `Model::new` and answered a question no ray tracer asks — *how fat is
//! Infinite Jest*. A WebGL shelf of three hundred spines needs the same answer,
//! and if it computes its own, the shelf and the book view disagree about the
//! same edition with nothing on either screen looking wrong. That is precisely
//! the failure item 17 was written about, so the derivation lives here.
//!
//! **Edition, not work.** Page count and cover art belong to a printing. Two
//! editions of the same novel are different objects on a shelf, and this type
//! is the only thing that distinguishes them.
//!
//! # Everything here is a ratio, and that is the whole design
//!
//! The renderer's `HALF_HEIGHT` is a *scene* constant — one ratatui camera rig's
//! idea of how big a book is. If this module handed back a number that only
//! meant something inside that rig, the arithmetic would have moved and the
//! decision would not have: a WebGL shelf would have inherited a terminal's
//! scene constant, which is worse than the duplication it replaced.
//!
//! So every number here is **a multiple of the book's own height**. Height is
//! `1.0` by definition and does not appear as a field. Each frontend picks how
//! tall a book is in its own units — `scene::HALF_HEIGHT` for the ray tracer,
//! a CSS pixel or a WebGL unit for the shelf — and multiplies.
//!
//! Millimetres were the alternative and were rejected: we do not know an
//! edition's real dimensions. Deriving "152mm" from a cover *image's* aspect
//! ratio would be inventing a measurement, and a number wearing a physical unit
//! it did not come by is a worse lie than an honest ratio.
//!
//! # Proportions are the engine's; look is the frontend's
//!
//! The clamps below are open to the charge that they are aesthetic decisions,
//! and aesthetics are a frontend's business. The line drawn here, and recorded
//! in `docs/decisions.md`, is:
//!
//! - **The object's proportions are the engine's.** They are what makes an
//!   edition *that* edition, and two frontends clamping differently is a shelf
//!   that contradicts a book view.
//! - **Everything about how it looks is the frontend's** — colour, lighting,
//!   bevels, shadow, spine typography, whether it is drawn at all.
//!
//! # Absence is not zero, and it is not silently 320 either
//!
//! Item 17 spent real effort establishing that a `NULL` page count must not
//! become a drawn empty track. A bare `unwrap_or(320)` is the same class of
//! thing in better clothes: it invents a length for a book whose length nobody
//! recorded.
//!
//! A renderer, unlike a progress bar, has no `None` to draw — a solid has to be
//! *some* thickness. So absence is filled, but it is never hidden:
//! [`ShapeSource`] marks each number as recorded or assumed, exactly as
//! [`crate::FractionSource`] marks where a fraction came from. A shelf that
//! wants to treat an invented thickness differently can; a shelf that does not
//! care ignores the field. What no caller can do is mistake a guess for a
//! measurement.
//!
//! `Some(0)` and negative page counts are absence too, on item 17's own
//! reasoning — and that is a **bug fix**, not just tidiness. `make dev-db` has
//! real `page_count = 0` rows; the renderer's `unwrap_or(320).clamp(48, 1400)`
//! mapped every one of them to 48 and drew a book of unknown length as the
//! thinnest pamphlet the model allows. Unknown is not short.

use crate::book::Book;

/// Narrowest a book is drawn, as a multiple of its height.
///
/// Roughly a tall, thin poetry hardback. Cover *images* are cropped, scanned
/// and jacketed at whatever aspect a provider felt like, so a 1:2 image is not
/// evidence of a 1:2 object — the clamp corrects an unreliable proxy back onto
/// a plausible physical book, which is a data judgement and not a look.
pub const NARROWEST: f32 = 0.55;

/// Widest a book is drawn, as a multiple of its height. Roughly a squarish art
/// book. Above this a "book" reads as a box.
pub const WIDEST: f32 = 0.85;

/// Thinnest a book is drawn, as a multiple of its height — one fifteenth.
///
/// Below this the spine stops reading as a solid and starts reading as a card,
/// which is a different object.
pub const THINNEST: f32 = 1.0 / 15.0;

/// Thickest a book is drawn, as a multiple of its height — four fifteenths.
///
/// Past here extra pages stop being informative: the eye reads "doorstop" and a
/// 2,000-page reference and a 5,000-page one look identical anyway.
pub const THICKEST: f32 = 4.0 / 15.0;

/// A 6x9in trade paperback: 2:3. The stand-in when nothing is recorded, and
/// deliberately the *ordinary* case rather than a distinctive one — an invented
/// shape should not draw attention to itself.
const PAPERBACK_ASPECT: f32 = 2.0 / 3.0;

/// The length assumed for a book whose length nobody recorded. A paperback.
const PAPERBACK_PAGES: i64 = 320;

/// Page counts outside this range stop changing the thickness. The floor is
/// there because a pamphlet still has covers; the ceiling because of
/// [`THICKEST`].
const FEWEST_PAGES: i64 = 48;
const MOST_PAGES: i64 = 1400;

/// How thick a book with no pages at all would be, as a multiple of its height.
///
/// Not physical — real boards are far thinner than 6% of a book's height. It is
/// the floor that keeps a short book a *solid* at the sizes these renderers
/// work at (a spine forty terminal cells tall), and the reason the thickness
/// curve is affine rather than proportional.
const BOARDS: f32 = 0.06;

/// How many pages stack to the book's own height.
///
/// The slope of the thickness curve, stated as the number it actually means.
/// At trade-paperback scale (229mm tall) it works out at about 0.034mm a page —
/// 0.07mm a leaf, against 0.09mm for real book stock. So the model is thinner
/// per page than paper and thicker at the base than boards; between them it
/// lands slightly fatter than life, which is what makes a spine legible.
const PAGES_PER_HEIGHT: f32 = 6750.0;

/// Where one of [`EditionShape`]'s numbers came from.
///
/// The sibling of [`crate::FractionSource`], and there for the same reason:
/// callers may say so or not, but what they may not do is compute a number and
/// forget whether anybody actually recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeSource {
    /// Derived from something recorded about this edition — its page count, its
    /// cover's real dimensions.
    Recorded,
    /// Nobody recorded it, so a trade paperback stood in.
    Assumed,
}

impl ShapeSource {
    pub fn is_assumed(self) -> bool {
        matches!(self, ShapeSource::Assumed)
    }
}

/// The physical shape of one edition, in multiples of its own height.
///
/// Height is `1.0` and is not a field — see the module header. Both ratios are
/// finite and inside their stated ranges for every possible input, including a
/// `NaN` aspect and a negative page count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditionShape {
    /// Width as a multiple of height, within [`NARROWEST`]..=[`WIDEST`].
    pub width_over_height: f32,
    /// Whether [`Self::width_over_height`] came from a real cover.
    pub width_source: ShapeSource,
    /// Thickness (spine depth) as a multiple of height, within
    /// [`THINNEST`]..=[`THICKEST`].
    pub thickness_over_height: f32,
    /// Whether [`Self::thickness_over_height`] came from a real page count.
    pub thickness_source: ShapeSource,
}

/// A page count that describes a physical object, or nothing.
///
/// The twin of `progress::denominator`, and the same rule: zero is a false
/// number rather than a small one.
fn usable_pages(page_count: Option<i64>) -> Option<i64> {
    page_count.filter(|p| *p > 0)
}

/// An aspect ratio that could have come off an image, or nothing.
fn usable_aspect(aspect: Option<f32>) -> Option<f32> {
    aspect.filter(|a| a.is_finite() && *a > 0.0)
}

impl EditionShape {
    /// From the two facts it needs, for a caller that has them loose — a shelf
    /// row, a DTO, a test.
    ///
    /// See [`EditionShape::of_book`] for what `cover_aspect` is and why it is an
    /// `Option`.
    pub fn new(page_count: Option<i64>, cover_aspect: Option<f32>) -> EditionShape {
        let (aspect, width_source) = match usable_aspect(cover_aspect) {
            Some(a) => (a, ShapeSource::Recorded),
            None => (PAPERBACK_ASPECT, ShapeSource::Assumed),
        };
        let (pages, thickness_source) = match usable_pages(page_count) {
            Some(p) => (p, ShapeSource::Recorded),
            None => (PAPERBACK_PAGES, ShapeSource::Assumed),
        };
        // Clamped twice on purpose. The page clamp is the decision — past 1,400
        // pages thickness stops being informative. The output clamp is what
        // makes "the answer is always in range" true of the *type* rather than
        // of today's curve, so a later slope cannot quietly widen it.
        let pages = pages.clamp(FEWEST_PAGES, MOST_PAGES) as f32;
        EditionShape {
            width_over_height: aspect.clamp(NARROWEST, WIDEST),
            width_source,
            thickness_over_height: (BOARDS + pages / PAGES_PER_HEIGHT).clamp(THINNEST, THICKEST),
            thickness_source,
        }
    }

    /// The shape of the edition a [`Book`] row describes.
    ///
    /// `cover_aspect` is the cover's `width / height`, and it is a **parameter
    /// rather than a field of `Book` because the engine does not yet store it**.
    /// Today the TUI passes `Some(cover.aspect)` from a freshly decoded image
    /// (`crates/tui/src/render3d/texture.rs`), which is fine for one book and
    /// absurd for a shelf of three hundred spines. Item 20 adds the stored
    /// `width`/`height` columns; when it lands, **that call site changes from an
    /// image decode to a division of two columns and this signature does not
    /// change at all**. That is the entire reason the parameter is shaped this
    /// way.
    ///
    /// `None` is the honest answer for a book with no cover, and it is a
    /// different answer from a cover we have not looked at yet — but from here
    /// they land in the same place, a trade paperback, and say so through
    /// [`EditionShape::width_source`].
    pub fn of_book(book: &Book, cover_aspect: Option<f32>) -> EditionShape {
        EditionShape::new(book.page_count, cover_aspect)
    }

    /// True when neither number came from anything recorded — the shape is
    /// entirely a stand-in.
    pub fn is_wholly_assumed(&self) -> bool {
        self.width_source.is_assumed() && self.thickness_source.is_assumed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(page_count: Option<i64>) -> Book {
        Book {
            page_count,
            ..Book::default()
        }
    }

    /// The reference the whole model is pinned to, and the numbers
    /// `scene::DEFAULT_HALF_EXTENTS` was written from: a 6x9in, 320-page trade
    /// paperback.
    #[test]
    fn a_trade_paperback_is_the_reference_shape() {
        let s = EditionShape::new(Some(320), Some(2.0 / 3.0));
        assert!((s.width_over_height - 0.666_666_7).abs() < 1e-6);
        assert!((s.thickness_over_height - 0.107_407_4).abs() < 1e-6);
        assert_eq!(s.width_source, ShapeSource::Recorded);
        assert_eq!(s.thickness_source, ShapeSource::Recorded);
    }

    /// The comparison `make dev-db` exists to make: a 1,408-page doorstop
    /// beside a 48-page pamphlet.
    #[test]
    fn a_doorstop_is_visibly_fatter_than_a_pamphlet() {
        let fat = EditionShape::new(Some(1408), None).thickness_over_height;
        let thin = EditionShape::new(Some(48), None).thickness_over_height;
        assert!(
            fat > thin * 3.0,
            "{fat} vs {thin} is not a visible difference"
        );
    }

    /// Item 17's rule, applied: `page_count = 0` is a real row in the dev
    /// library and it means *nobody wrote one down*. The renderer used to draw
    /// those as the thinnest book the model allows.
    #[test]
    fn a_zero_page_count_is_absence_and_not_a_pamphlet() {
        let zero = EditionShape::new(Some(0), None);
        let unknown = EditionShape::new(None, None);
        assert_eq!(zero, unknown);
        assert_eq!(zero.thickness_source, ShapeSource::Assumed);
        assert!(
            zero.thickness_over_height > EditionShape::new(Some(48), None).thickness_over_height,
            "unknown length drew thinner than the thinnest real book"
        );
    }

    #[test]
    fn a_negative_page_count_is_absence_too() {
        assert_eq!(
            EditionShape::new(Some(-40), None),
            EditionShape::new(None, None)
        );
    }

    /// The condition that makes filling an absence defensible at all: an
    /// invented thickness must land on an ordinary book, not on a rail. A book
    /// nobody measured must not read as remarkably thin or remarkably fat.
    #[test]
    fn an_unknown_length_lands_inside_the_range_not_at_an_edge() {
        let s = EditionShape::new(None, None);
        assert!(
            s.thickness_over_height > THINNEST && s.thickness_over_height < THICKEST,
            "{} is at an edge",
            s.thickness_over_height
        );
        assert_eq!(
            s.thickness_over_height,
            EditionShape::new(Some(320), None).thickness_over_height,
            "the invented length is a paperback"
        );
        // Same number, different provenance — which is the entire distinction
        // this type exists to keep.
        assert_ne!(s, EditionShape::new(Some(320), None));
        assert!(s.is_wholly_assumed());
    }

    #[test]
    fn a_missing_cover_is_a_trade_paperback_and_says_so() {
        let s = EditionShape::new(Some(320), None);
        assert!((s.width_over_height - PAPERBACK_ASPECT).abs() < 1e-6);
        assert_eq!(s.width_source, ShapeSource::Assumed);
        assert_eq!(s.thickness_source, ShapeSource::Recorded);
    }

    /// A cover image is not the object. Providers serve square thumbnails and
    /// full jacket wraps; neither is a square book or a book twice as wide as
    /// it is tall.
    #[test]
    fn a_square_cover_does_not_make_a_square_book() {
        assert_eq!(
            EditionShape::new(Some(300), Some(1.0)).width_over_height,
            WIDEST
        );
        assert_eq!(
            EditionShape::new(Some(300), Some(0.2)).width_over_height,
            NARROWEST
        );
    }

    /// A decode can hand back anything. `f32::clamp` passes `NaN` straight
    /// through, so a `NaN` aspect would otherwise reach a vertex buffer.
    #[test]
    fn a_nonsense_aspect_falls_back_rather_than_propagating() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.5] {
            let s = EditionShape::new(Some(300), Some(bad));
            assert!(
                (s.width_over_height - PAPERBACK_ASPECT).abs() < 1e-6,
                "{bad}"
            );
            assert_eq!(s.width_source, ShapeSource::Assumed, "{bad}");
        }
    }

    #[test]
    fn beyond_the_page_range_thickness_stops_growing() {
        let long = EditionShape::new(Some(1400), None).thickness_over_height;
        assert_eq!(
            long,
            EditionShape::new(Some(20_000), None).thickness_over_height
        );
        assert_eq!(long, THICKEST);
        let short = EditionShape::new(Some(48), None).thickness_over_height;
        assert_eq!(
            short,
            EditionShape::new(Some(1), None).thickness_over_height
        );
    }

    #[test]
    fn of_book_reads_the_length_off_the_row() {
        assert_eq!(
            EditionShape::of_book(&book(Some(900)), Some(0.7)),
            EditionShape::new(Some(900), Some(0.7))
        );
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The invariant every renderer leans on and none could guarantee for
        /// itself: whatever arrives — a `NaN` aspect, a negative length, an
        /// i64 of pages — both numbers are finite and inside the range this
        /// module publishes.
        #[test]
        fn a_shape_is_always_drawable(
            pages in proptest::option::of(any::<i64>()),
            aspect in proptest::option::of(any::<f32>()),
        ) {
            let s = EditionShape::new(pages, aspect);
            prop_assert!(s.width_over_height.is_finite());
            prop_assert!(s.thickness_over_height.is_finite());
            prop_assert!(
                (NARROWEST..=WIDEST).contains(&s.width_over_height),
                "width out of range: {}", s.width_over_height
            );
            prop_assert!(
                (THINNEST..=THICKEST).contains(&s.thickness_over_height),
                "thickness out of range: {}", s.thickness_over_height
            );
        }

        /// A longer book is never drawn thinner than a shorter one. Obvious,
        /// and the only thing that makes a shelf mean anything — the whole
        /// point of the spine widths is that they are comparable at a glance.
        #[test]
        fn more_pages_is_never_thinner(
            a in 1i64..30_000,
            b in 1i64..30_000,
        ) {
            let (lo, hi) = (a.min(b), a.max(b));
            prop_assert!(
                EditionShape::new(Some(lo), None).thickness_over_height
                    <= EditionShape::new(Some(hi), None).thickness_over_height
            );
        }

        /// The same rule on the other axis, which is what stops a future clamp
        /// from being written as a fold or a wrap.
        #[test]
        fn a_wider_cover_is_never_a_narrower_book(
            a in 0.01f32..8.0,
            b in 0.01f32..8.0,
        ) {
            let (lo, hi) = (a.min(b), a.max(b));
            prop_assert!(
                EditionShape::new(None, Some(lo)).width_over_height
                    <= EditionShape::new(None, Some(hi)).width_over_height
            );
        }

        /// The converse of the constructor's own guards: a number may only
        /// claim to be `Recorded` when something usable was actually recorded.
        /// This is what stops a later arm quietly laundering a guess.
        #[test]
        fn recorded_means_something_was_recorded(
            pages in proptest::option::of(-100i64..5000),
            aspect in proptest::option::of(-2.0f32..4.0),
        ) {
            let s = EditionShape::new(pages, aspect);
            if s.thickness_source == ShapeSource::Recorded {
                prop_assert!(pages.is_some_and(|p| p > 0), "a guessed length claimed to be recorded");
            }
            if s.width_source == ShapeSource::Recorded {
                prop_assert!(
                    aspect.is_some_and(|a| a > 0.0 && a.is_finite()),
                    "a guessed aspect claimed to be recorded"
                );
            }
        }
    }
}
