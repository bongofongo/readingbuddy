//! Ratings: the user's own scale, its Goodreads lookup, and the value a review
//! carries.
//!
//! Three rules from `docs/decisions.md`, and all three are visible in the
//! schema rather than only in code:
//!
//! - **Numeric scales only for v1** — `min`, `max`, `step`. Ordered labels are
//!   explicitly out.
//! - **Store the raw value plus the scale id, never only the mapped value.**
//!   The map is user-editable, so the mapping has to stay re-derivable.
//! - **The Goodreads mapping is an explicit lookup table, never a formula.**
//!   Formulas are always wrong at the ends, and Goodreads' `My Rating` is an
//!   integer 0–5 with no halves — so a scale value with no mapping *says so*
//!   and is never silently rounded.

use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use super::{Storage, now_unix};
use crate::error::{EngineError, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct RatingScale {
    pub id: i64,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

/// A review's rating: the raw value **and** the scale it was given on, which is
/// the pair that keeps the Goodreads mapping re-derivable.
#[derive(Debug, Clone, PartialEq)]
pub struct Rating {
    pub scale: RatingScale,
    pub value: f64,
}

fn row_to_scale(r: &SqliteRow) -> RatingScale {
    RatingScale {
        id: r.get("id"),
        name: r.get("name"),
        min: r.get("min"),
        max: r.get("max"),
        step: r.get("step"),
    }
}

impl RatingScale {
    /// How many steps from `min` a value sits, when it sits on one at all.
    ///
    /// Everything that reads or writes a rating goes through this, and that is
    /// the point. `rating_map.value` is a REAL in a PRIMARY KEY, so a lookup is
    /// float equality — and `min + 3*0.1` is not the `0.3` the user typed. One
    /// quantizer on both sides means the value written and the value looked up
    /// are the same bits, whatever the step.
    pub fn step_index(&self, value: f64) -> Option<i64> {
        if !value.is_finite() || self.step <= 0.0 {
            return None;
        }
        // A tolerance rather than exact equality: the user types 4.5, we compute
        // it, and binary floats do not promise those agree to the last bit. It
        // stays comfortably above `value_at`'s rounding and far below any step a
        // person types, so a value that came *out* of the grid always goes back
        // in.
        let tolerance = (self.step * 1e-6).max(1e-8);
        // The bounds get the same tolerance, and that is not politeness: with a
        // fractional `min`, `value_at(0)` rounds to just *below* it, so a strict
        // `value < self.min` rejected the scale's own first point.
        if value < self.min - tolerance || value > self.max + tolerance {
            return None;
        }
        let n = ((value - self.min) / self.step).round();
        let snapped = self.min + n * self.step;
        ((value - snapped).abs() <= tolerance && n >= 0.0).then_some(n as i64)
    }

    /// The canonical value at step `n` — what actually goes into the database.
    pub fn value_at(&self, n: i64) -> f64 {
        // Rounded, so an accumulated 0.30000000000000004 and a typed 0.3 land on
        // one key. Nine places: enough to erase the accumulation, small enough
        // that the result is still inside `step_index`'s tolerance.
        let v = self.min + n as f64 * self.step;
        (v * 1e9).round() / 1e9
    }

    /// Every point on the scale, low to high.
    pub fn points(&self) -> Vec<f64> {
        let mut out = Vec::new();
        if self.step <= 0.0 || self.max < self.min {
            return out;
        }
        let last = ((self.max - self.min) / self.step).floor() as i64;
        for n in 0..=last {
            out.push(self.value_at(n));
        }
        out
    }

    /// Snap a user-supplied value onto the grid, or say why it is not on it.
    pub fn canonical(&self, value: f64) -> Result<f64> {
        match self.step_index(value) {
            Some(n) => Ok(self.value_at(n)),
            None => Err(EngineError::InvalidInput(format!(
                "{value} is not a point on the '{}' scale ({}..{} step {})",
                self.name, self.min, self.max, self.step
            ))),
        }
    }
}

impl Storage {
    /// Create a rating scale, or redefine one that already has this name.
    ///
    /// Redefining keeps the row's id — a review already rated on it keeps
    /// pointing at the scale it was given on, which is the whole reason the
    /// scale id is stored beside the value.
    pub async fn put_rating_scale(
        &self,
        name: &str,
        min: f64,
        max: f64,
        step: f64,
    ) -> Result<RatingScale> {
        if !(min.is_finite() && max.is_finite() && step.is_finite()) || step <= 0.0 || max <= min {
            return Err(EngineError::InvalidInput(format!(
                "a scale needs min < max and a positive step (got {min}..{max} step {step})"
            )));
        }
        let row = sqlx::query(
            "INSERT INTO rating_scales (name, min, max, step, created_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (name) DO UPDATE SET min = excluded.min, max = excluded.max,
                                              step = excluded.step
             RETURNING id, name, min, max, step",
        )
        .bind(name)
        .bind(min)
        .bind(max)
        .bind(step)
        .bind(now_unix())
        .fetch_one(self.pool())
        .await?;
        Ok(row_to_scale(&row))
    }

    /// The scale a new rating is given on: the most recently *created* one.
    ///
    /// Deliberately "created", not "modified": redefining `default`'s bounds
    /// must not yank ratings away from a scale the user made afterwards. A
    /// migration seeds `default`, so this is never `None` in practice — but a
    /// caller that deleted every scale gets an honest `None` rather than a
    /// panic.
    pub async fn active_rating_scale(&self) -> Result<Option<RatingScale>> {
        let row = sqlx::query(
            "SELECT id, name, min, max, step FROM rating_scales ORDER BY created_at DESC, id DESC LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.as_ref().map(row_to_scale))
    }

    pub async fn rating_scale_by_name(&self, name: &str) -> Result<Option<RatingScale>> {
        let row = sqlx::query("SELECT id, name, min, max, step FROM rating_scales WHERE name = ?")
            .bind(name)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.as_ref().map(row_to_scale))
    }

    pub async fn list_rating_scales(&self) -> Result<Vec<RatingScale>> {
        let rows =
            sqlx::query("SELECT id, name, min, max, step FROM rating_scales ORDER BY id ASC")
                .fetch_all(self.pool())
                .await?;
        Ok(rows.iter().map(row_to_scale).collect())
    }

    /// Rate a review. One rating per note, replaced on a second call.
    pub async fn set_review_rating(
        &self,
        note_id: i64,
        scale: &RatingScale,
        value: f64,
    ) -> Result<()> {
        let value = scale.canonical(value)?;
        sqlx::query(
            "INSERT INTO review_ratings (note_id, scale_id, value) VALUES (?, ?, ?)
             ON CONFLICT (note_id) DO UPDATE SET scale_id = excluded.scale_id,
                                                 value = excluded.value",
        )
        .bind(note_id)
        .bind(scale.id)
        .bind(value)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn clear_review_rating(&self, note_id: i64) -> Result<bool> {
        let done = sqlx::query("DELETE FROM review_ratings WHERE note_id = ?")
            .bind(note_id)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// A review's rating, with the scale it was given on.
    pub async fn review_rating(&self, note_id: i64) -> Result<Option<Rating>> {
        let row = sqlx::query(
            "SELECT s.id, s.name, s.min, s.max, s.step, r.value
               FROM review_ratings r JOIN rating_scales s ON s.id = r.scale_id
              WHERE r.note_id = ?",
        )
        .bind(note_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| Rating {
            value: r.get("value"),
            scale: row_to_scale(&r),
        }))
    }

    /// Record what one scale value means on Goodreads' integer 0–5.
    pub async fn map_rating(&self, scale: &RatingScale, value: f64, goodreads: u8) -> Result<()> {
        if goodreads > 5 {
            return Err(EngineError::InvalidInput(format!(
                "Goodreads ratings are integers 0–5 (0 = unrated); got {goodreads}"
            )));
        }
        let value = scale.canonical(value)?;
        sqlx::query(
            "INSERT INTO rating_map (scale_id, value, goodreads) VALUES (?, ?, ?)
             ON CONFLICT (scale_id, value) DO UPDATE SET goodreads = excluded.goodreads",
        )
        .bind(scale.id)
        .bind(value)
        .bind(goodreads)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The whole map of a scale, low value first.
    pub async fn rating_map(&self, scale_id: i64) -> Result<Vec<(f64, u8)>> {
        let rows = sqlx::query(
            "SELECT value, goodreads FROM rating_map WHERE scale_id = ? ORDER BY value ASC",
        )
        .bind(scale_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get::<f64, _>("value"), r.get::<i64, _>("goodreads") as u8))
            .collect())
    }

    /// What this scale value means on Goodreads, or `None` when the user has
    /// not said. **`None` is an answer, not a failure** — the caller reports it
    /// rather than rounding.
    pub async fn goodreads_for(&self, scale: &RatingScale, value: f64) -> Result<Option<u8>> {
        let Some(n) = scale.step_index(value) else {
            return Ok(None);
        };
        let mapped: Option<i64> =
            sqlx::query_scalar("SELECT goodreads FROM rating_map WHERE scale_id = ? AND value = ?")
                .bind(scale.id)
                .bind(scale.value_at(n))
                .fetch_optional(self.pool())
                .await?;
        Ok(mapped.map(|g| g as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn storage() -> Storage {
        Storage::connect("sqlite::memory:").await.unwrap()
    }

    /// The migration seeds a scale so that rating a review works out of the
    /// box, and maps the whole stars only — a half star has no honest Goodreads
    /// integer, and saying so is the behaviour this whole module exists for.
    #[tokio::test]
    async fn the_seeded_scale_maps_whole_stars_and_admits_the_halves() {
        let s = storage().await;
        let scale = s.active_rating_scale().await.unwrap().expect("seeded");
        assert_eq!(scale.name, "default");
        assert_eq!((scale.min, scale.max, scale.step), (0.0, 5.0, 0.5));

        assert_eq!(s.goodreads_for(&scale, 4.0).await.unwrap(), Some(4));
        assert_eq!(s.goodreads_for(&scale, 0.0).await.unwrap(), Some(0));
        assert_eq!(
            s.goodreads_for(&scale, 4.5).await.unwrap(),
            None,
            "a half star is unmapped until the user maps it, never rounded"
        );

        s.map_rating(&scale, 4.5, 5).await.unwrap();
        assert_eq!(s.goodreads_for(&scale, 4.5).await.unwrap(), Some(5));
    }

    #[tokio::test]
    async fn a_value_off_the_grid_is_refused_rather_than_snapped() {
        let s = storage().await;
        let scale = s.active_rating_scale().await.unwrap().unwrap();
        assert!(matches!(
            scale.canonical(4.3),
            Err(EngineError::InvalidInput(_))
        ));
        assert!(matches!(
            scale.canonical(9.0),
            Err(EngineError::InvalidInput(_))
        ));
        assert_eq!(scale.canonical(4.5).unwrap(), 4.5);
    }

    /// The pair that must round-trip is (value, scale) — never the mapped
    /// integer alone, or re-editing the map could not change what a stored
    /// review means.
    #[tokio::test]
    async fn the_raw_value_and_its_scale_round_trip() {
        let s = storage().await;
        let scale = s.put_rating_scale("tenths", 0.0, 1.0, 0.1).await.unwrap();
        let note = seed_note(&s).await;

        s.set_review_rating(note, &scale, 0.3).await.unwrap();
        let got = s.review_rating(note).await.unwrap().expect("rated");
        assert_eq!(got.value, 0.3);
        assert_eq!(got.scale, scale);

        // The map is re-editable, and the stored review follows it.
        assert_eq!(s.goodreads_for(&got.scale, got.value).await.unwrap(), None);
        s.map_rating(&scale, 0.3, 2).await.unwrap();
        assert_eq!(
            s.goodreads_for(&got.scale, got.value).await.unwrap(),
            Some(2)
        );
        s.map_rating(&scale, 0.3, 3).await.unwrap();
        assert_eq!(
            s.goodreads_for(&got.scale, got.value).await.unwrap(),
            Some(3)
        );
    }

    /// `0.1 * 3` is not `0.3`, and `rating_map.value` is half of a PRIMARY KEY.
    /// Without one quantizer on both sides, a map written by walking the scale
    /// would be invisible to a lookup of the value the user typed.
    #[tokio::test]
    async fn a_walked_scale_and_a_typed_value_agree() {
        let s = storage().await;
        let scale = s.put_rating_scale("tenths", 0.0, 1.0, 0.1).await.unwrap();
        for (i, p) in scale.points().iter().enumerate() {
            s.map_rating(&scale, *p, (i / 2) as u8).await.unwrap();
        }
        assert_eq!(s.goodreads_for(&scale, 0.3).await.unwrap(), Some(1));
        assert_eq!(s.goodreads_for(&scale, 0.7).await.unwrap(), Some(3));
    }

    /// Found by the property below, shrunk to this: `value_at` rounds, so on a
    /// scale whose `min` has more than nine decimals its own first point lands
    /// a fraction *below* `min` — and a strict bounds check then refused the
    /// scale's own value, leaving one point permanently unmappable.
    #[tokio::test]
    async fn the_first_point_of_a_fractional_scale_is_on_its_own_scale() {
        let s = storage().await;
        let min = 2.2720732703878195;
        let scale = s
            .put_rating_scale("odd", min, min + 5.0, 0.273)
            .await
            .unwrap();
        let first = scale.points()[0];
        assert!(first < min, "the rounding really does go under");
        assert_eq!(scale.canonical(first).unwrap(), first);

        s.map_rating(&scale, first, 1).await.unwrap();
        assert_eq!(s.goodreads_for(&scale, first).await.unwrap(), Some(1));
        assert_eq!(s.goodreads_for(&scale, min).await.unwrap(), Some(1));
    }

    #[tokio::test]
    async fn redefining_a_scale_keeps_its_id_and_its_ratings() {
        let s = storage().await;
        let scale = s.active_rating_scale().await.unwrap().unwrap();
        let note = seed_note(&s).await;
        s.set_review_rating(note, &scale, 4.0).await.unwrap();

        let wider = s.put_rating_scale("default", 0.0, 10.0, 1.0).await.unwrap();
        assert_eq!(wider.id, scale.id);
        let got = s.review_rating(note).await.unwrap().unwrap();
        assert_eq!(got.value, 4.0);
        assert_eq!(got.scale.max, 10.0);
    }

    #[tokio::test]
    async fn a_scale_needs_a_positive_step_and_room_to_move() {
        let s = storage().await;
        assert!(s.put_rating_scale("bad", 0.0, 5.0, 0.0).await.is_err());
        assert!(s.put_rating_scale("bad", 5.0, 5.0, 1.0).await.is_err());
        assert!(s.put_rating_scale("bad", 5.0, 0.0, 1.0).await.is_err());
    }

    #[tokio::test]
    async fn goodreads_refuses_a_value_it_cannot_hold() {
        let s = storage().await;
        let scale = s.active_rating_scale().await.unwrap().unwrap();
        assert!(matches!(
            s.map_rating(&scale, 4.0, 6).await,
            Err(EngineError::InvalidInput(_))
        ));
    }

    /// A bare note row to hang a rating on — this module does not care that a
    /// rating belongs to a review; `Engine::set_rating` is where that is
    /// enforced.
    async fn seed_note(s: &Storage) -> i64 {
        s.insert_note(
            super::super::NewNoteMeta {
                book_id: None,
                reading_id: None,
                highlight_id: None,
                page: None,
                location: None,
                file_path: "unsorted/review.md",
                title: "Review",
                kind: "review",
            },
            "body",
            &[],
        )
        .await
        .unwrap()
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Once every point on a scale is mapped, the map is a **total
        /// function** on that scale: every point the scale can produce has an
        /// answer, and no point produces one it was not given.
        ///
        /// A property rather than more examples because the rule is general
        /// over (min, max, step) and the interesting failures are float ones —
        /// a walked value and a typed value drifting apart at some step size
        /// nobody picked by hand.
        #[test]
        fn a_fully_mapped_scale_is_total(
            min in -10.0f64..10.0,
            span in 1.0f64..12.0,
            step in 0.25f64..3.0,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all().build().unwrap();
            rt.block_on(async {
                let s = Storage::connect("sqlite::memory:").await.unwrap();
                let scale = s.put_rating_scale("p", min, min + span, step).await.unwrap();
                let points = scale.points();
                prop_assert!(!points.is_empty());

                for (i, p) in points.iter().enumerate() {
                    s.map_rating(&scale, *p, (i % 6) as u8).await.unwrap();
                }
                for (i, p) in points.iter().enumerate() {
                    prop_assert_eq!(
                        s.goodreads_for(&scale, *p).await.unwrap(),
                        Some((i % 6) as u8),
                        "unmapped point {} on {}..{} step {}", p, min, min + span, step
                    );
                }

                // And it stays a function of the scale only: a value between two
                // points is not on the scale at all, so it has no answer rather
                // than the nearest one.
                if points.len() > 1 {
                    let between = (points[0] + points[1]) / 2.0;
                    if scale.step_index(between).is_none() {
                        prop_assert_eq!(s.goodreads_for(&scale, between).await.unwrap(), None);
                    }
                }
                Ok(())
            })?;
        }
    }
}
