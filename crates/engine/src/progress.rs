//! How far into a book a reading is — as a **value**, once.
//!
//! Item 17's centrepiece. Before it there were four implementations of this
//! arithmetic and they did not agree:
//!
//! | | rule |
//! |---|---|
//! | `crates/tui/src/ui/library.rs` | `done` / `{pct}%` / `p.{p}` / `""` — guards `total > 0` |
//! | `crates/tui/src/ui/home.rs` | delegates to the above, **then** falls back to `Reading::ko_percent` |
//! | `crates/tui/src/ui/book.rs` | `finished` / `{p} / {t} · {pct}%` / `page {p}` / `not started` |
//! | `crates/cli/src/render.rs` | `[finished]` / `[p/t]` / `[p.{p}]` — **no `total > 0` guard**, so a zero page count printed `[12/0]` |
//!
//! The words in that table are still each frontend's. What is not is *which
//! case a book is in*, and there are three real hazards behind that:
//!
//! 1. **`page_count = 0`** is a real row in `make dev-db`. Every percentage
//!    over it is a divide by zero, and the honest answer is that there is no
//!    percentage — which is a different answer from 0%. [`Progress`] normalises
//!    a zero denominator to absence rather than carrying it, so a caller cannot
//!    reach one.
//! 2. **`page_count = NULL`** is also real, and is absence rather than zero. A
//!    progress bar must not draw an empty track over an unknown length, which
//!    is what every `unwrap_or(0)` produces.
//! 3. **`ko_percent`** — the device's own fraction — is a *better* answer than
//!    pages for a book with no page count, and a book pulled from a KOReader
//!    sidecar routinely has one and no `current_page` at all. Choosing between
//!    the two is exactly the derivation that must not live in two frontends.
//!
//! **Pages win where they can answer; the device fills in where they cannot.**
//! `current_page` is ours — merged, editable, and the number printed on the
//! paper — so where there is an honest denominator it is what a reader
//! recognises. `ko_percent` is refreshed by straight assignment from the device
//! and can disagree with `current_page`; it is consulted only when pages have
//! nothing to say, which reproduces exactly what `home.rs` did by hand and is
//! now the only copy of the rule.
//!
//! **This is not reading *state*.** Whether a book is being read, was put down,
//! or was never opened is [`crate::ReadingState`]; `Progress::Untouched` says
//! only that nothing has been recorded, and an abandoned book halfway through
//! is `Started` like any other. Conflating them is how "abandoned" becomes a
//! progress bar that stopped short, which is failure framing wearing arithmetic.

use crate::book::Book;
use crate::storage::{Reading, STATUS_FINISHED};

/// Where a fraction came from. Frontends may say so or not; what they may not
/// do is compute one and forget which it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FractionSource {
    /// `current_page / page_count`, with an honest denominator.
    Pages,
    /// `readings.ko_percent` — the device's own, for a book whose length we do
    /// not know.
    Device,
}

/// A fraction of a book, `0.0..=1.0`, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fraction {
    /// Clamped to `0.0..=1.0` on construction. A device that reports 1.02, or a
    /// `current_page` past a stale `page_count`, is a real thing and a progress
    /// bar 102% wide is not.
    pub value: f64,
    pub source: FractionSource,
}

/// How far into a book a reading is.
///
/// Three cases, and the middle one carries what it knows rather than filling
/// the gaps: a book can have a page and no length, a length and no page, a
/// device percentage and neither.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Progress {
    /// Nothing has been recorded: no page, no device percentage, not finished.
    ///
    /// **Not zero**, and deliberately not called *unread*. A book nobody has
    /// opened and a book opened at page one are the same row here; the
    /// difference between them is [`crate::ReadingState`], and a name like
    /// "unread" would put the banned completion framing into a type.
    Untouched,
    /// Somewhere in it.
    Started {
        /// The current reading's page, if one is recorded.
        page: Option<i64>,
        /// The book's length. **Never `Some(0)`** — a zero page count is a
        /// false denominator and is normalised to absence here, which is the
        /// one place it can be done once.
        of: Option<i64>,
        /// The fraction, when one is computable honestly. `None` is a real
        /// answer: a page with no length has no percentage.
        fraction: Option<Fraction>,
    },
    /// The current reading is closed as finished.
    ///
    /// Carries no page: every frontend that had this case dropped the numbers
    /// for a word, and a book finished at page 200 of 200 has nothing a page
    /// number adds.
    Finished,
}

/// A page count that can be divided by, or nothing.
fn denominator(page_count: Option<i64>) -> Option<i64> {
    page_count.filter(|t| *t > 0)
}

fn clamp(v: f64) -> f64 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

impl Progress {
    /// Assemble from the parts, applying the rule the module header states.
    ///
    /// The one constructor the two public entry points share, so that "pages
    /// first, device second" cannot be spelled twice.
    fn from_parts(
        finished: bool,
        page: Option<i64>,
        page_count: Option<i64>,
        ko: Option<f64>,
    ) -> Progress {
        if finished {
            return Progress::Finished;
        }
        let of = denominator(page_count);
        let fraction = match (page, of) {
            (Some(p), Some(t)) => Some(Fraction {
                value: clamp(p as f64 / t as f64),
                source: FractionSource::Pages,
            }),
            _ => ko.map(|v| Fraction {
                value: clamp(v),
                source: FractionSource::Device,
            }),
        };
        if page.is_none() && fraction.is_none() {
            return Progress::Untouched;
        }
        Progress::Started { page, of, fraction }
    }

    /// **The current reading's** progress, off a [`Book`].
    ///
    /// Not "the book's": `current_page`, `finished` and `ko_percent` are
    /// read-only projections of the current reading (open if there is one, else
    /// the most recent), so on a reread this is the progress of the read you are
    /// in and not of the book across all of them. Every existing call site wanted
    /// exactly that, which is why this form exists at all — but a screen showing
    /// one *specific* reading of a book must use [`Progress::of_reading`], or it
    /// will show the current read's numbers under an older read's heading.
    pub fn of_book(b: &Book) -> Progress {
        Progress::from_parts(b.finished, b.current_page, b.page_count, b.ko_percent)
    }

    /// One named reading's progress.
    ///
    /// `page_count` comes from the book, because length is bibliographic and
    /// `readings` has no column for it. A reading is finished when it is
    /// *closed* as finished — `status`, not `finished_at`, since
    /// `abandon_reading` deliberately leaves the reading open.
    pub fn of_reading(r: &Reading, page_count: Option<i64>) -> Progress {
        Progress::from_parts(
            r.status == STATUS_FINISHED,
            r.current_page,
            page_count,
            r.ko_percent,
        )
    }

    /// A whole-number percentage, when there is one.
    ///
    /// Here rather than left to `(fraction * 100.0)` at four call sites, and the
    /// integer division is the reason: `29 / 100` is `0.28999999999999998` in
    /// binary, so flooring the float gives **28** where the integer division
    /// gives 29. The two frontends that already computed a percentage did it in
    /// integers and were right to; this keeps their answer byte-for-byte and
    /// gives the third one the same number.
    ///
    /// A device fraction has no integers to divide, so it rounds.
    pub fn percent(&self) -> Option<i64> {
        match self {
            Progress::Finished => Some(100),
            Progress::Untouched => None,
            Progress::Started { page, of, fraction } => {
                let f = (*fraction)?;
                match f.source {
                    FractionSource::Pages => {
                        let (p, t) = ((*page)?, (*of)?);
                        Some((p.saturating_mul(100) / t).clamp(0, 100))
                    }
                    FractionSource::Device => Some((f.value * 100.0).round() as i64),
                }
            }
        }
    }

    /// The fraction to draw a bar to, `0.0..=1.0`.
    ///
    /// [`Progress::Finished`] answers `1.0`: a finished reading is the whole
    /// book by definition of the state, not by an arithmetic we would have to
    /// invent numbers for.
    pub fn fraction(&self) -> Option<f64> {
        match self {
            Progress::Finished => Some(1.0),
            Progress::Untouched => None,
            Progress::Started { fraction, .. } => fraction.map(|f| f.value),
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, Progress::Finished)
    }

    /// True when there is nothing to show. The condition every frontend spelled
    /// as "the tag came back empty".
    pub fn is_untouched(&self) -> bool {
        matches!(self, Progress::Untouched)
    }

    /// The page to name, if any. `None` on a finished reading is deliberate —
    /// see [`Progress::Finished`].
    pub fn page(&self) -> Option<i64> {
        match self {
            Progress::Started { page, .. } => *page,
            _ => None,
        }
    }

    /// The length to name, never zero.
    pub fn of(&self) -> Option<i64> {
        match self {
            Progress::Started { of, .. } => *of,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(page: Option<i64>, count: Option<i64>, finished: bool, ko: Option<f64>) -> Book {
        Book {
            current_page: page,
            page_count: count,
            finished,
            ko_percent: ko,
            ..Book::default()
        }
    }

    #[test]
    fn finished_wins_over_every_number() {
        let p = Progress::of_book(&book(Some(12), Some(200), true, Some(0.4)));
        assert_eq!(p, Progress::Finished);
        assert_eq!(p.percent(), Some(100));
        assert_eq!(p.fraction(), Some(1.0));
    }

    #[test]
    fn a_page_and_a_length_give_a_percentage_from_pages() {
        let p = Progress::of_book(&book(Some(50), Some(200), false, None));
        assert_eq!(p.percent(), Some(25));
        assert_eq!(p.page(), Some(50));
        assert_eq!(p.of(), Some(200));
        let Progress::Started { fraction, .. } = p else {
            panic!("expected Started, got {p:?}");
        };
        assert_eq!(fraction.unwrap().source, FractionSource::Pages);
    }

    /// The hazard the CLI shipped: a real book in the dev library has
    /// `page_count = 0`. It is a false denominator, not a length of nothing.
    #[test]
    fn a_zero_page_count_is_absence_and_never_a_denominator() {
        let p = Progress::of_book(&book(Some(12), Some(0), false, None));
        assert_eq!(p.of(), None, "a zero length must not reach a caller");
        assert_eq!(p.percent(), None);
        assert_eq!(p.page(), Some(12));
    }

    #[test]
    fn a_null_page_count_is_absence_not_zero() {
        let p = Progress::of_book(&book(Some(12), None, false, None));
        assert_eq!(p.of(), None);
        assert_eq!(p.percent(), None);
        assert_eq!(
            p.fraction(),
            None,
            "no track to draw over an unknown length"
        );
    }

    /// `home.rs`'s fallback, now the only copy of it: a sidecar-pulled book has
    /// a device percentage and no page at all.
    #[test]
    fn the_device_percentage_answers_where_pages_cannot() {
        let p = Progress::of_book(&book(None, None, false, Some(0.435)));
        assert_eq!(p.percent(), Some(44));
        let Progress::Started { fraction, page, .. } = p else {
            panic!("expected Started, got {p:?}");
        };
        assert_eq!(page, None);
        assert_eq!(fraction.unwrap().source, FractionSource::Device);
    }

    /// And it does **not** override pages that can answer, even when the two
    /// disagree — which they can, since `ko_percent` is refreshed by straight
    /// assignment.
    #[test]
    fn pages_win_over_the_device_when_both_are_there() {
        let p = Progress::of_book(&book(Some(50), Some(200), false, Some(0.9)));
        assert_eq!(p.percent(), Some(25));
    }

    /// A page with no length still falls through to the device, which is the
    /// case `page_count = NULL` plus a KOReader pull produces.
    #[test]
    fn a_page_with_no_length_still_takes_the_device_fraction() {
        let p = Progress::of_book(&book(Some(12), None, false, Some(0.5)));
        assert_eq!(p.page(), Some(12));
        assert_eq!(p.percent(), Some(50));
    }

    #[test]
    fn nothing_recorded_is_untouched_rather_than_zero() {
        let p = Progress::of_book(&book(None, Some(200), false, None));
        assert_eq!(p, Progress::Untouched);
        assert_eq!(p.percent(), None);
        assert_eq!(p.fraction(), None);
    }

    #[test]
    fn out_of_range_input_is_clamped_rather_than_carried() {
        // A stale page count, and a device past the end.
        assert_eq!(
            Progress::of_book(&book(Some(300), Some(200), false, None)).percent(),
            Some(100)
        );
        assert_eq!(
            Progress::of_book(&book(None, None, false, Some(1.4))).fraction(),
            Some(1.0)
        );
        assert_eq!(
            Progress::of_book(&book(None, None, false, Some(-0.2))).fraction(),
            Some(0.0)
        );
    }

    fn reading(status: &str, page: Option<i64>, ko: Option<f64>) -> Reading {
        Reading {
            id: 1,
            book_id: 1,
            started_at: None,
            finished_at: None,
            status: status.to_string(),
            source: "manual".to_string(),
            current_page: page,
            ko_status: None,
            ko_percent: ko,
            ko_rating: None,
            created_at: 0,
            last_modified: 0,
        }
    }

    /// A reading is finished when its **status** says so. `abandon_reading`
    /// leaves the reading open on purpose, so `finished_at` cannot be the test.
    #[test]
    fn a_reading_is_finished_by_its_status() {
        assert_eq!(
            Progress::of_reading(&reading("finished", Some(10), None), Some(200)),
            Progress::Finished
        );
        let abandoned = Progress::of_reading(&reading("abandoned", Some(50), None), Some(200));
        assert_eq!(abandoned.percent(), Some(25));
        assert!(!abandoned.is_finished(), "put down is not finished");
    }

    /// An abandoned reading is `Started`, exactly like an active one. The
    /// difference is state, not progress, and this is the assertion that keeps
    /// "put down" from becoming a bar that stopped short.
    #[test]
    fn abandoned_and_reading_have_the_same_shape_of_progress() {
        let a = Progress::of_reading(&reading("abandoned", Some(50), None), Some(200));
        let b = Progress::of_reading(&reading("reading", Some(50), None), Some(200));
        assert_eq!(a, b);
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    fn book(page: Option<i64>, count: Option<i64>, finished: bool, ko: Option<f64>) -> Book {
        Book {
            current_page: page,
            page_count: count,
            finished,
            ko_percent: ko,
            ..Book::default()
        }
    }

    proptest! {
        /// The invariant a progress bar leans on, and the one no frontend could
        /// guarantee for itself: whatever arrives — a negative page, a stale
        /// count, a device past the end — the fraction is in range or absent.
        #[test]
        fn a_fraction_is_always_drawable(
            page in proptest::option::of(-1000i64..100_000),
            count in proptest::option::of(-10i64..5000),
            finished in any::<bool>(),
            ko in proptest::option::of(-2.0f64..3.0),
        ) {
            let p = Progress::of_book(&book(page, count, finished, ko));
            if let Some(f) = p.fraction() {
                prop_assert!((0.0..=1.0).contains(&f), "out of range: {f}");
            }
            if let Some(pct) = p.percent() {
                prop_assert!((0..=100).contains(&pct), "out of range: {pct}");
            }
        }

        /// A denominator that reaches a caller can always be divided by. This is
        /// the `[12/0]` bug stated as a rule rather than as one more example.
        #[test]
        fn a_reported_length_is_never_zero(
            page in proptest::option::of(0i64..1000),
            count in proptest::option::of(-5i64..5),
            ko in proptest::option::of(0.0f64..1.0),
        ) {
            let p = Progress::of_book(&book(page, count, false, ko));
            if let Some(t) = p.of() {
                prop_assert!(t > 0, "a false denominator escaped: {t}");
            }
        }

        /// `Untouched` means nothing was recorded — so it can only happen when
        /// nothing was. The converse of the constructor's own guard, which is
        /// what stops a future arm quietly swallowing a page.
        #[test]
        fn untouched_only_when_there_is_genuinely_nothing(
            page in proptest::option::of(0i64..1000),
            count in proptest::option::of(0i64..1000),
            ko in proptest::option::of(0.0f64..1.0),
        ) {
            let p = Progress::of_book(&book(page, count, false, ko));
            if p.is_untouched() {
                prop_assert!(page.is_none(), "a page was dropped");
                prop_assert!(ko.is_none(), "a device fraction was dropped");
            }
        }
    }
}
