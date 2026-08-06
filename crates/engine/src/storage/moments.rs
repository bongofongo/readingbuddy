//! Moments (item 23, migration `0017`) — the four things worth noticing, and
//! the memory that makes each of them happen once.
//!
//! A moment is **recognised, never announced**. Nothing here is a threshold a
//! frontend could put in front of somebody ("three more days and…"), nothing
//! counts down, and nothing counts up: `docs/decisions.md` bans task-completion
//! framing by name, and a target is what a threshold becomes the moment it is
//! visible before it is met. Every moment in this module is a fact in the past
//! tense about something the reader already did, assembled out of rows that
//! were written for other reasons.
//!
//! So **nothing about a moment is stored**. [`Storage::pending_moments`]
//! derives the whole candidate set on every ask. What *is* stored is what the
//! app has already shown — the `moments` table is a set of ids and nothing
//! else — because "it fired" is the one fact about a moment that is not a fact
//! about the reader.
//!
//! ## The four kinds
//!
//! | kind | the evidence | identity |
//! |---|---|---|
//! | [`MomentKind::ReadingClosed`] | `readings.finished_at` | the reading |
//! | [`MomentKind::FirstAnnotation`] | the earliest highlight stamp or note on a book | the book |
//! | [`MomentKind::ReflectionReached`] | a reflection's wikilink into another book's note | the pair |
//! | [`MomentKind::RunEnded`] | consecutive days in `reading_events` | the span |
//!
//! ## The two guards, and why both err toward silence
//!
//! **An import mints nothing.** `docs/decisions.md` decides the awkward case: a
//! fresh install importing a 400-book Goodreads CSV must not mint 400 moments,
//! because an import is history arriving rather than something you just did. So
//! a moment fires only for evidence dated **at or after the book entered the
//! library** — `books.created_at`, which has existed since `0001` and needed no
//! column of its own. A CSV's `Date Read` of 2019 against a book created this
//! afternoon is history; the same book finished tomorrow is not.
//!
//! **An upgrade is not a ceremony.** The per-book guard cannot answer for a
//! library that has been here for months: those books arrived honestly and
//! their reading happened afterwards, so on the first launch after `0017` every
//! one of them would fire at once — the same failure the CSV case names, from
//! the other direction. `moment_epoch.began_at` is the answer, set by the
//! migration to the instant the schema learned what a moment was.
//!
//! Both guards are approximations and both were chosen to fail the same way.
//! `books.created_at` is an **upper** bound on when a book really arrived
//! (`merge_books` folds a duplicate onto the older row), so it can suppress a
//! real moment and can never invent one. A reflection's reach is dated by the
//! **later of its two notes**, a *lower* bound — `note_links` carries no
//! timestamp of its own, which item 21 recorded and this module inherits — and
//! a lower bound that clears the guard proves the true time clears it too. Read
//! together: **a missed ceremony is a cost, and a replayed library is a bug.**
//!
//! ## The run, which is the one that had to be argued
//!
//! [`MomentKind::RunEnded`] is a hair's breadth from a streak, and a streak is
//! the shape `docs/decisions.md` calls out as looking most like a feature.
//! Three things keep it on the right side of that line and all three are
//! load-bearing:
//!
//! * **It is only ever recognised after it is over.** A run whose last day is
//!   yesterday is not a moment, because today might continue it; the cutoff is
//!   [`run_cutoff`]. Nothing can therefore be shown while a run is running,
//!   which is precisely what makes it impossible to put pressure on.
//! * **Two days is the definition of "consecutive", not a bar somebody chose.**
//!   [`RUN_MIN_DAYS`] is 2 because one day is not a run. A three or a seven
//!   would be a threshold, and this module would then be in the business of
//!   deciding what counts as enough.
//! * **`days` is past tense.** It says how many days you read, the way item 17
//!   permits a count of your own highlights. It is not a target, a remaining, a
//!   best, or a comparison with anything.
//!
//! ## What crosses, and what is opaque
//!
//! [`Moment`] carries `reading_id` beside `book_id` because a reread is a
//! second read of one book and a moment naming only the book cannot say which.
//! [`Moment::id`] is a string built here and **parsed nowhere else** — not by a
//! frontend, not by SQL in the migration. [`Storage::acknowledge_moment`] takes
//! one back and checks only that its kind is one this build knows, which is
//! enough to turn a client bug into an error instead of a row.

use std::collections::HashSet;

use sqlx::Row;

use super::{Storage, now_unix};
use crate::error::{EngineError, Result};

/// The shortest span that is a *run*, and it is a definition rather than a
/// threshold: one day is a day, two consecutive days are consecutive. Any
/// larger number would be this module deciding what counts as enough reading,
/// which is the thing it exists not to do.
pub const RUN_MIN_DAYS: usize = 2;

/// What happened. The payload is whatever the phrasing needs and the identity
/// does not: `book_id` and `reading_id` live on [`Moment`] itself, because
/// every kind that has them has them for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MomentKind {
    /// A read ended. `Moment::reading_id` is the read; `Moment::book_id` its
    /// book.
    ReadingClosed,
    /// The first mark of the reader's own on a book that carried none — a
    /// highlight, a note, a reflection. Whichever came first, by the evidence's
    /// own clock and never by ours: a highlight is dated by the device's
    /// `ko_datetime`, so an import cannot make one look like today.
    FirstAnnotation,
    /// A reflection reached a book it had not reached before, through the
    /// wikilink graph. `Moment::book_id` is the reflection's own book;
    /// `reached_book_id` is the far side.
    ReflectionReached { note_id: i64, reached_book_id: i64 },
    /// A run of consecutive days with activity, which has since ended. Never
    /// while it is running — see the module doc.
    RunEnded {
        /// `YYYY-MM-DD`, UTC, inclusive.
        from: String,
        /// `YYYY-MM-DD`, UTC, inclusive. Also [`Moment::day`].
        to: String,
        /// How many days it held. Past tense, and never compared with anything.
        days: i64,
    },
}

impl MomentKind {
    /// The stable token that opens an id. Adding a kind adds a token; changing
    /// one would orphan every acknowledgement already recorded against it, so
    /// these are as permanent as a migration.
    pub fn tag(&self) -> &'static str {
        match self {
            MomentKind::ReadingClosed => "reading_closed",
            MomentKind::FirstAnnotation => "first_annotation",
            MomentKind::ReflectionReached { .. } => "reflection_reached",
            MomentKind::RunEnded { .. } => "run_ended",
        }
    }

    /// Every tag this build knows, for [`Storage::acknowledge_moment`]'s check.
    const TAGS: [&'static str; 4] = [
        "reading_closed",
        "first_annotation",
        "reflection_reached",
        "run_ended",
    ];
}

/// One thing worth noticing, derived and not stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moment {
    /// Stable, opaque, and the only thing
    /// [`Storage::acknowledge_moment`] needs. Built from the ids of the rows
    /// the moment is made of, so it survives a restart, a re-derivation and a
    /// rebuilt activity log — and so that two frontends looking at one library
    /// acknowledge the same string.
    pub id: String,
    pub kind: MomentKind,
    /// `None` only for [`MomentKind::RunEnded`], which is about the library
    /// rather than a book.
    pub book_id: Option<i64>,
    /// Which read this belongs to, where the evidence settles on one. A reread
    /// is a second read of one book, so a card minted per reading cannot be
    /// selected by `book_id` alone. `None` where two reads share the evidence
    /// or none claims it — the call [`Storage::reading_for_day`] and
    /// `attribute_highlights` both already make.
    pub reading_id: Option<i64>,
    /// `YYYY-MM-DD`, UTC. The same day convention as `reading_events`, for the
    /// same reason: a second one here would put a moment and the event it was
    /// derived from on different days for everyone not on UTC.
    pub day: String,
    /// Unix seconds. What the list is ordered by, newest first.
    pub occurred_at: i64,
}

impl Moment {
    fn new(
        kind: MomentKind,
        book_id: Option<i64>,
        reading_id: Option<i64>,
        at: i64,
    ) -> Result<Moment> {
        let subject = match &kind {
            MomentKind::ReadingClosed => reading_id
                .ok_or_else(|| {
                    EngineError::InvalidInput("a closed reading has no reading id".into())
                })?
                .to_string(),
            MomentKind::FirstAnnotation => book_id
                .ok_or_else(|| EngineError::InvalidInput("a first annotation has no book".into()))?
                .to_string(),
            MomentKind::ReflectionReached {
                note_id,
                reached_book_id,
            } => format!("{note_id}:{reached_book_id}"),
            MomentKind::RunEnded { from, to, .. } => format!("{from}..{to}"),
        };
        Ok(Moment {
            id: format!("{}:{subject}", kind.tag()),
            day: day_of_unix(at)?,
            kind,
            book_id,
            reading_id,
            occurred_at: at,
        })
    }
}

/// `YYYY-MM-DD`, UTC, from unix seconds.
///
/// The same conversion `reading_events` does, reached through that module so
/// there is one of it. A moment derived from an event must not be able to land
/// on a different day from the event.
fn day_of_unix(ts: i64) -> Result<String> {
    super::reading_events::day_of_unix(ts)
}

fn parse_day(s: &str) -> Option<time::Date> {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    time::Date::parse(s, fmt).ok()
}

fn format_day(d: time::Date) -> String {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    d.format(fmt).unwrap_or_default()
}

/// Midnight UTC of a day, as unix seconds. A run has no instant of its own —
/// it is a span of days — so it is dated by the start of the day it ended on,
/// which is the earliest instant that day could be said to have happened.
fn midnight_of(day: time::Date) -> i64 {
    day.midnight().assume_utc().unix_timestamp()
}

/// The last day a run may end on and still be over.
///
/// A run whose last day is **yesterday** is not finished: today has not
/// happened yet, and a run recognised while it might still be running is a
/// streak. So the cutoff is the day before yesterday, and the test is
/// `last_day <= cutoff`.
///
/// UTC, from `now`, because `reading_events.day` is UTC. Item 17 left *relative
/// time* in the frontends — "three days ago" needs an answer to what today is
/// locally — and this is not that: it is the same UTC day arithmetic the
/// activity log already does, with the clock passed in rather than read here so
/// that a test can say when it is.
fn run_cutoff(now: i64) -> Result<time::Date> {
    let today = time::OffsetDateTime::from_unix_timestamp(now)
        .map_err(|e| EngineError::InvalidInput(format!("timestamp {now} is out of range: {e}")))?
        .date();
    Ok(today - time::Duration::days(2))
}

/// Split a sorted, deduplicated list of days into maximal consecutive runs.
///
/// Pure, and the properties are in `mod props`: the runs **partition** the
/// input in order, every run is consecutive, and no run can be extended —
/// which together are the whole meaning of "maximal run", stated as three
/// things a generator can try to break rather than as one word.
///
/// A day the parser cannot read is dropped rather than guessed at. Nothing in
/// this codebase invents a day, and a run built around an unreadable one would
/// silently join two spans that are not adjacent.
pub fn runs_of_days(days: &[String]) -> Vec<(String, String, usize)> {
    let mut parsed: Vec<time::Date> = days.iter().filter_map(|d| parse_day(d)).collect();
    parsed.sort_unstable();
    parsed.dedup();

    let mut runs = Vec::new();
    let mut i = 0;
    while i < parsed.len() {
        let start = parsed[i];
        let mut end = start;
        while i + 1 < parsed.len() && parsed[i + 1] == end.next_day().unwrap_or(end) {
            i += 1;
            end = parsed[i];
        }
        // `next_day` is `None` only at `Date::MAX`, where `unwrap_or(end)`
        // makes the comparison false and the run ends — the correct answer
        // rather than a panic on a date no library will hold.
        let len = (end - start).whole_days() as usize + 1;
        runs.push((format_day(start), format_day(end), len));
        i += 1;
    }
    runs
}

impl Storage {
    /// When moments began — `moment_epoch.began_at`, written by migration
    /// `0017`.
    ///
    /// Everything dated before it is history rather than news. See the module
    /// doc: without it, the first launch after that migration replays a whole
    /// reading life.
    pub async fn moment_epoch(&self) -> Result<i64> {
        Ok(
            sqlx::query_scalar::<_, i64>("SELECT began_at FROM moment_epoch WHERE id = 1")
                .fetch_one(self.pool())
                .await?,
        )
    }

    /// Move the epoch. The one caller that is not a test is a future frontend
    /// offering "start noticing from here"; the tests are what make the guard
    /// assertable, since an in-memory database's epoch is always *now* and
    /// every fixture's history is in the past.
    ///
    /// It is on `Storage` and deliberately not on `Engine`: nothing in the
    /// product asks for it yet, and item 14's seam is that the facade carries
    /// what a frontend needs rather than everything that exists.
    pub async fn set_moment_epoch(&self, began_at: i64) -> Result<()> {
        sqlx::query("UPDATE moment_epoch SET began_at = ? WHERE id = 1")
            .bind(began_at)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Everything worth noticing that has not been shown yet, newest first.
    ///
    /// Derived on every call — there is no moments table to go stale, and
    /// nothing accumulates. `now` is the clock the run cutoff is measured
    /// against and is passed in rather than read here, so a test can say when
    /// it is; `Engine::pending_moments` supplies `now_unix()`.
    ///
    /// `limit` truncates **after** ordering, so a caller asking for three gets
    /// the three most recent rather than three arbitrary ones. How many to
    /// surface at once is a frontend's decision and this is the only lever it
    /// gets: there is deliberately no count endpoint and no length field
    /// anywhere on the wire, because a number of things waiting is a badge and
    /// `docs/decisions.md` forbids exactly that.
    pub async fn pending_moments(&self, now: i64, limit: Option<i64>) -> Result<Vec<Moment>> {
        let epoch = self.moment_epoch().await?;
        let mut out = Vec::new();
        out.extend(self.readings_closed(epoch).await?);
        out.extend(self.first_annotations(epoch).await?);
        out.extend(self.reflections_reached(epoch).await?);
        out.extend(self.runs_ended(epoch, now).await?);

        let surfaced: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT id FROM moments")
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .collect();
        out.retain(|m| !surfaced.contains(&m.id));

        // Newest first, and by id under a tie so that two calls with the same
        // data return the same order — a ceremony that reshuffled itself
        // between two polls would show one moment twice and another never.
        out.sort_by(|a, b| {
            b.occurred_at
                .cmp(&a.occurred_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        if let Some(n) = limit {
            out.truncate(n.max(0) as usize);
        }
        Ok(out)
    }

    /// Record that a moment was shown, so the ceremony does not replay.
    ///
    /// **Idempotent**: `ON CONFLICT DO NOTHING`, so acknowledging twice — or
    /// from two frontends — cannot write a second row or move the first
    /// `surfaced_at`. The time kept is the *first* surfacing, which is the one
    /// that is true.
    ///
    /// The id is checked for a kind this build knows and nothing more. It is
    /// deliberately **not** re-derived: `RunEnded` depends on the clock, so a
    /// moment that was pending when it was shown can legitimately fail to be
    /// pending a moment later, and refusing the acknowledgement then would
    /// replay it for ever. A well-formed id for a moment that never existed
    /// costs one inert row.
    pub async fn acknowledge_moment(&self, id: &str) -> Result<()> {
        let tag = id.split(':').next().unwrap_or_default();
        if !MomentKind::TAGS.contains(&tag) || id.len() <= tag.len() + 1 {
            return Err(EngineError::InvalidInput(format!(
                "{id:?} is not a moment id"
            )));
        }
        sqlx::query(
            "INSERT INTO moments (id, surfaced_at) VALUES (?, ?)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(now_unix())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// A read that ended.
    ///
    /// `finished_at IS NOT NULL` is what closed means here — `storage/CLAUDE.md`
    /// is explicit that an *abandoned* book is one you might pick up, so the
    /// status is not the test and a moment does not judge how the read ended.
    async fn readings_closed(&self, epoch: i64) -> Result<Vec<Moment>> {
        let rows = sqlx::query(
            "SELECT r.id AS reading_id, r.book_id, r.finished_at AS at
               FROM readings r
               JOIN books b ON b.id = r.book_id
              WHERE r.finished_at IS NOT NULL
                AND r.finished_at >= b.created_at
                AND r.finished_at >= ?",
        )
        .bind(epoch)
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|r| {
                Moment::new(
                    MomentKind::ReadingClosed,
                    Some(r.get("book_id")),
                    Some(r.get("reading_id")),
                    r.get("at"),
                )
            })
            .collect()
    }

    /// The first mark of the reader's own on a book that had none.
    ///
    /// Two evidence streams and they are dated differently on purpose. A note
    /// is ours, so `notes.created_at` is when it happened. A highlight is the
    /// **device's**, so it is dated by `ko_datetime` and never by our
    /// `highlights.created_at` — the latter is when the sidecar was read, and
    /// using it would make every KOReader import mint a first-annotation moment
    /// for every book in it, which is the CSV failure with a different importer.
    /// A highlight whose stamp SQLite cannot read as a time contributes
    /// nothing, the same rule `reading_events`' fillers follow.
    ///
    /// `min` over the union is what makes it *first*: adding a highlight today
    /// to a book you annotated last year is not a first annotation, and the
    /// guard sees last year.
    async fn first_annotations(&self, epoch: i64) -> Result<Vec<Moment>> {
        let rows = sqlx::query(
            "WITH marks AS (
                 SELECT h.book_id AS book_id,
                        CAST(strftime('%s', h.ko_datetime) AS INTEGER) AS at,
                        h.reading_id AS reading_id
                   FROM highlights h
                  WHERE h.ko_datetime IS NOT NULL
                    AND strftime('%s', h.ko_datetime) IS NOT NULL
                  UNION ALL
                 SELECT n.book_id, n.created_at, n.reading_id
                   FROM notes n
                  WHERE n.book_id IS NOT NULL
             ),
             earliest AS (SELECT book_id, min(at) AS at FROM marks GROUP BY book_id)
             SELECT f.book_id, f.at AS at,
                    (SELECT CASE WHEN count(DISTINCT m.reading_id) = 1
                                 THEN min(m.reading_id) END
                       FROM marks m
                      WHERE m.book_id = f.book_id AND m.at = f.at) AS reading_id
               FROM earliest f
               JOIN books b ON b.id = f.book_id
              WHERE f.at IS NOT NULL
                AND f.at >= b.created_at
                AND f.at >= ?",
        )
        .bind(epoch)
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|r| {
                Moment::new(
                    MomentKind::FirstAnnotation,
                    Some(r.get("book_id")),
                    r.get("reading_id"),
                    r.get("at"),
                )
            })
            .collect()
    }

    /// A reflection reaching a book it had not reached before.
    ///
    /// The edge is `note_links`, so only a **resolved** link counts: a wikilink
    /// that is still text names a note that does not exist, and a note that
    /// does not exist belongs to no book. Back-resolution fills `to_note` in as
    /// soon as the target is written, which is what makes "resolved" a state
    /// this can wait for rather than a race it can lose.
    ///
    /// **`note_links` carries no timestamp** — item 21 recorded that and it is
    /// unchanged here — so the reach is dated `max(reflection.created_at,
    /// target.created_at)`: an edge cannot predate either end of itself. That
    /// is a lower bound, and a lower bound is the safe direction against a
    /// guard that suppresses (module doc). The cost, stated: a reflection
    /// written before a book arrived and linked to it afterwards is dated by
    /// the *note* the link found, so the moment fires exactly when that note is
    /// what makes it possible — which is the honest reading of the evidence
    /// available.
    ///
    /// A reflection linking to another note of its **own** book is not reaching
    /// anywhere, so it is excluded. A reflection with no book of its own — one
    /// whose book was deleted, since `notes.book_id` is `ON DELETE SET NULL` —
    /// still reaches.
    async fn reflections_reached(&self, epoch: i64) -> Result<Vec<Moment>> {
        let rows = sqlx::query(
            "SELECT * FROM (
                 SELECT r.id AS note_id, r.book_id AS book_id, r.reading_id AS reading_id,
                        t.book_id AS reached_book_id,
                        rb.created_at AS reached_created_at,
                        min(max(r.created_at, t.created_at)) AS at
                   FROM notes r
                   JOIN note_links l ON l.from_note = r.id
                   JOIN notes t ON t.id = l.to_note
                   JOIN books rb ON rb.id = t.book_id
                  WHERE r.kind = 'reflection'
                    AND (r.book_id IS NULL OR t.book_id <> r.book_id)
                  GROUP BY r.id, t.book_id
             )
             WHERE at >= reached_created_at AND at >= ?",
        )
        .bind(epoch)
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|r| {
                Moment::new(
                    MomentKind::ReflectionReached {
                        note_id: r.get("note_id"),
                        reached_book_id: r.get("reached_book_id"),
                    },
                    r.get("book_id"),
                    r.get("reading_id"),
                    r.get("at"),
                )
            })
            .collect()
    }

    /// Runs of consecutive activity days that are over.
    ///
    /// The days come from `reading_events`, which is the one place every source
    /// agrees on a grain — that is item 21's whole argument, and it is why this
    /// moment works for a library that arrived as a CSV and has no minutes in
    /// it at all. The per-book guard is applied to the **evidence** rather than
    /// to the run: a day only counts if it is at or after the day its book
    /// entered the library, so a 400-book import contributes no days and
    /// therefore no runs, without this function knowing anything about imports.
    ///
    /// The gap-and-islands is done in Rust ([`runs_of_days`]) rather than in
    /// SQL, because it is date arithmetic and SQLite's is string arithmetic
    /// with a `date()` around it; a pure function is also what lets the
    /// partition be asserted as a property rather than by example.
    async fn runs_ended(&self, epoch: i64, now: i64) -> Result<Vec<Moment>> {
        let days: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT e.day
               FROM reading_events e
               JOIN books b ON b.id = e.book_id
              WHERE e.day >= date(b.created_at, 'unixepoch')
                AND e.day >= date(?, 'unixepoch')
              ORDER BY e.day ASC",
        )
        .bind(epoch)
        .fetch_all(self.pool())
        .await?;

        let cutoff = run_cutoff(now)?;
        runs_of_days(&days)
            .into_iter()
            .filter(|(_, to, len)| {
                *len >= RUN_MIN_DAYS && parse_day(to).is_some_and(|d| d <= cutoff)
            })
            .map(|(from, to, len)| {
                let at = parse_day(&to).map(midnight_of).unwrap_or_default();
                Moment::new(
                    MomentKind::RunEnded {
                        from,
                        to,
                        days: len as i64,
                    },
                    None,
                    None,
                    at,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::storage::{NewHighlight, NewNoteMeta};

    const DAY: i64 = 86_400;
    /// 2026-01-05 00:00:00 UTC.
    const JAN5: i64 = 1_767_571_200;
    /// Long enough after the fixtures that every run in them has ended.
    const NOW: i64 = JAN5 + 100 * DAY;
    /// Before every fixture, so the epoch is not what a test is asserting
    /// unless it says so.
    const LONG_AGO: i64 = JAN5 - 400 * DAY;

    async fn seeded() -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        s.set_moment_epoch(LONG_AGO).await.unwrap();
        let id = book_at(&s, "Pachinko", LONG_AGO).await;
        (s, id)
    }

    /// A book that entered the library at a given instant. `books.created_at`
    /// is written by the insert, so the fixtures move it afterwards — which is
    /// exactly the shape the per-book guard is about.
    async fn book_at(s: &Storage, title: &str, created_at: i64) -> i64 {
        let id = s
            .upsert_book(
                &Book {
                    title: Some(title.into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        sqlx::query("UPDATE books SET created_at = ? WHERE id = ?")
            .bind(created_at)
            .bind(id)
            .execute(s.pool())
            .await
            .unwrap();
        id
    }

    fn hl(text: &str, when: &str) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: None,
            page: Some(1),
            pos0: Some(format!("/body/p[1]/text().{text}")),
            pos1: None,
            ko_datetime: Some(when.into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    async fn note_on(s: &Storage, book: i64, title: &str, kind: &str, at: i64) -> i64 {
        let path = format!("vault/{title}.md");
        let id = s
            .insert_note(
                NewNoteMeta {
                    book_id: Some(book),
                    highlight_id: None,
                    reading_id: None,
                    file_path: &path,
                    title,
                    kind,
                    page: None,
                    location: None,
                },
                "",
                &[],
            )
            .await
            .unwrap();
        sqlx::query("UPDATE notes SET created_at = ? WHERE id = ?")
            .bind(at)
            .bind(id)
            .execute(s.pool())
            .await
            .unwrap();
        id
    }

    async fn pending(s: &Storage) -> Vec<Moment> {
        s.pending_moments(NOW, None).await.unwrap()
    }

    fn ids(ms: &[Moment]) -> Vec<String> {
        ms.iter().map(|m| m.id.clone()).collect()
    }

    /// The moments of one kind, by its own tag — so a test names the kind once
    /// rather than spelling a `matches!` over a payload it does not care about.
    fn of_kind(ms: &[Moment], tag: &str) -> Vec<Moment> {
        ms.iter().filter(|m| m.kind.tag() == tag).cloned().collect()
    }

    // ---- the runs, as a pure function -------------------------------------

    #[test]
    fn consecutive_days_collapse_into_one_run_and_a_gap_splits_them() {
        let days: Vec<String> = ["2026-01-01", "2026-01-02", "2026-01-03", "2026-01-05"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            runs_of_days(&days),
            vec![
                ("2026-01-01".into(), "2026-01-03".into(), 3),
                ("2026-01-05".into(), "2026-01-05".into(), 1),
            ]
        );
    }

    #[test]
    fn a_month_boundary_is_not_a_gap() {
        let days: Vec<String> = ["2026-01-31", "2026-02-01", "2026-02-28", "2026-03-01"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            runs_of_days(&days),
            vec![
                ("2026-01-31".into(), "2026-02-01".into(), 2),
                ("2026-02-28".into(), "2026-03-01".into(), 2),
            ],
            "2026 is not a leap year, so the 28th and the 1st are adjacent"
        );
    }

    #[test]
    fn an_unreadable_day_is_dropped_rather_than_joining_two_spans() {
        let days: Vec<String> = ["2026-01-01", "2026-13-45", "2026-01-02"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            runs_of_days(&days),
            vec![("2026-01-01".into(), "2026-01-02".into(), 2)]
        );
    }

    // ---- the four kinds ---------------------------------------------------

    #[tokio::test]
    async fn a_reading_that_closed_after_its_book_arrived_is_a_moment() {
        let (s, b) = seeded().await;
        let r = s
            .record_reading(b, Some(JAN5), Some(JAN5 + 5 * DAY), "finished", "manual")
            .await
            .unwrap();

        let ms = pending(&s).await;
        let closed: Vec<&Moment> = ms
            .iter()
            .filter(|m| m.kind == MomentKind::ReadingClosed)
            .collect();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].id, format!("reading_closed:{r}"));
        assert_eq!(closed[0].book_id, Some(b));
        assert_eq!(
            closed[0].reading_id,
            Some(r),
            "a card is minted per reading, so the moment has to name one"
        );
        assert_eq!(closed[0].day, "2026-01-10");
    }

    #[tokio::test]
    async fn an_open_reading_is_not_a_moment() {
        let (s, b) = seeded().await;
        s.open_reading(b, Some(JAN5), "manual").await.unwrap();
        assert!(
            pending(&s)
                .await
                .iter()
                .all(|m| m.kind != MomentKind::ReadingClosed)
        );
    }

    #[tokio::test]
    async fn the_first_mark_on_a_book_is_a_moment_and_the_second_is_not() {
        let (s, b) = seeded().await;
        s.insert_highlight(b, &hl("a", "2026-01-05 08:00:00"))
            .await
            .unwrap();
        s.insert_highlight(b, &hl("b", "2026-01-09 08:00:00"))
            .await
            .unwrap();

        let first = of_kind(&pending(&s).await, "first_annotation");
        assert_eq!(first.len(), 1, "one book, one first annotation");
        assert_eq!(first[0].day, "2026-01-05", "the earliest, not the latest");
    }

    #[tokio::test]
    async fn a_note_counts_as_a_first_mark_and_beats_a_later_highlight() {
        let (s, b) = seeded().await;
        note_on(&s, b, "thoughts", "note", JAN5).await;
        s.insert_highlight(b, &hl("later", "2026-01-09 08:00:00"))
            .await
            .unwrap();

        let m = pending(&s)
            .await
            .into_iter()
            .find(|m| m.kind == MomentKind::FirstAnnotation)
            .unwrap();
        assert_eq!(m.day, "2026-01-05");
    }

    #[tokio::test]
    async fn a_highlight_with_no_readable_stamp_is_no_evidence_at_all() {
        let (s, b) = seeded().await;
        s.insert_highlight(b, &hl("junk", "sometime last winter"))
            .await
            .unwrap();
        assert!(
            pending(&s)
                .await
                .iter()
                .all(|m| m.kind != MomentKind::FirstAnnotation),
            "nothing here invents a day, so nothing here invents a moment"
        );
    }

    #[tokio::test]
    async fn a_reflection_reaching_another_book_is_a_moment_once_per_book_reached() {
        let (s, a) = seeded().await;
        let b = book_at(&s, "Middlemarch", LONG_AGO).await;
        let c = book_at(&s, "The Overstory", LONG_AGO).await;

        let target_b = note_on(&s, b, "middlemarch-note", "note", JAN5).await;
        let target_c = note_on(&s, c, "overstory-note", "note", JAN5).await;
        let own = note_on(&s, a, "pachinko-note", "note", JAN5).await;
        let refl = note_on(&s, a, "Reflection: Pachinko", "reflection", JAN5).await;

        s.set_note_links(
            refl,
            "Reflection: Pachinko",
            &[
                "middlemarch-note".into(),
                "overstory-note".into(),
                "pachinko-note".into(),
            ],
        )
        .await
        .unwrap();
        assert!(target_b > 0 && target_c > 0 && own > 0);

        let reached = of_kind(&pending(&s).await, "reflection_reached");
        assert_eq!(
            reached.len(),
            2,
            "two books reached; its own is not a reach"
        );
        assert!(reached.iter().all(|m| m.book_id == Some(a)
            && matches!(
                m.kind,
                MomentKind::ReflectionReached { reached_book_id, .. } if reached_book_id != a
            )));
    }

    #[tokio::test]
    async fn a_dangling_wikilink_reaches_nothing() {
        let (s, a) = seeded().await;
        let refl = note_on(&s, a, "Reflection: Pachinko", "reflection", JAN5).await;
        s.set_note_links(
            refl,
            "Reflection: Pachinko",
            &["a note nobody wrote".into()],
        )
        .await
        .unwrap();
        assert!(
            pending(&s)
                .await
                .iter()
                .all(|m| !matches!(m.kind, MomentKind::ReflectionReached { .. })),
            "a target that is still text belongs to no book"
        );
    }

    #[tokio::test]
    async fn a_plain_note_linking_across_is_not_a_reflection_reaching() {
        let (s, a) = seeded().await;
        let b = book_at(&s, "Middlemarch", LONG_AGO).await;
        note_on(&s, b, "middlemarch-note", "note", JAN5).await;
        let plain = note_on(&s, a, "just a note", "note", JAN5).await;
        s.set_note_links(plain, "just a note", &["middlemarch-note".into()])
            .await
            .unwrap();
        assert!(
            pending(&s)
                .await
                .iter()
                .all(|m| !matches!(m.kind, MomentKind::ReflectionReached { .. }))
        );
    }

    #[tokio::test]
    async fn a_run_of_days_that_ended_is_a_moment_and_a_single_day_is_not() {
        let (s, b) = seeded().await;
        for d in [0, 1, 2, 5] {
            note_on(&s, b, &format!("n{d}"), "note", JAN5 + d * DAY).await;
        }
        s.refill_reading_events().await.unwrap();

        let runs = of_kind(&pending(&s).await, "run_ended");
        assert_eq!(runs.len(), 1, "the lone day five days later is not a run");
        assert_eq!(
            runs[0].kind,
            MomentKind::RunEnded {
                from: "2026-01-05".into(),
                to: "2026-01-07".into(),
                days: 3,
            }
        );
        assert_eq!(runs[0].book_id, None, "a run is about the library");
    }

    #[tokio::test]
    async fn a_run_that_might_still_be_running_is_not_a_moment() {
        let (s, b) = seeded().await;
        for d in [0, 1] {
            note_on(&s, b, &format!("n{d}"), "note", JAN5 + d * DAY).await;
        }
        s.refill_reading_events().await.unwrap();

        // "Now" is the day after the run's last day, so today could still
        // extend it. Recognising it here is what a streak does.
        let ongoing = s.pending_moments(JAN5 + 2 * DAY, None).await.unwrap();
        assert!(
            ongoing
                .iter()
                .all(|m| !matches!(m.kind, MomentKind::RunEnded { .. }))
        );

        // A clear day later it is over, and only then.
        let over = s.pending_moments(JAN5 + 3 * DAY, None).await.unwrap();
        assert_eq!(
            over.iter()
                .filter(|m| matches!(m.kind, MomentKind::RunEnded { .. }))
                .count(),
            1
        );
    }

    // ---- the guards -------------------------------------------------------

    #[tokio::test]
    async fn a_four_hundred_book_import_mints_nothing() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        s.set_moment_epoch(LONG_AGO).await.unwrap();

        // The shape of a Goodreads import into a fresh library: the books
        // arrive now, and every date they carry is years old.
        let arrived = JAN5 + 50 * DAY;
        for n in 0..20 {
            let b = book_at(&s, &format!("Imported {n}"), arrived).await;
            s.record_reading(
                b,
                Some(JAN5 - 300 * DAY),
                Some(JAN5 - 290 * DAY + n * DAY),
                "finished",
                "goodreads",
            )
            .await
            .unwrap();
            s.insert_highlight(b, &hl(&format!("h{n}"), "2025-02-02 10:00:00"))
                .await
                .unwrap();
        }
        s.refill_reading_events().await.unwrap();

        assert_eq!(
            pending(&s).await,
            vec![],
            "an import is history arriving, not a thing you just did"
        );
    }

    #[tokio::test]
    async fn a_book_finished_after_it_arrived_is_still_a_moment() {
        let (s, _) = seeded().await;
        let b = book_at(&s, "Bought Yesterday", JAN5).await;
        s.record_reading(b, Some(JAN5), Some(JAN5 + 2 * DAY), "finished", "manual")
            .await
            .unwrap();
        assert!(
            pending(&s)
                .await
                .iter()
                .any(|m| m.kind == MomentKind::ReadingClosed && m.book_id == Some(b)),
            "the guard is about the order of the two facts, not about importers"
        );
    }

    #[tokio::test]
    async fn everything_older_than_the_epoch_is_history() {
        let (s, b) = seeded().await;
        s.record_reading(b, Some(JAN5), Some(JAN5 + 5 * DAY), "finished", "manual")
            .await
            .unwrap();
        note_on(&s, b, "n", "note", JAN5).await;
        assert!(!pending(&s).await.is_empty());

        // What migration 0017 does on a library that has been here for months.
        s.set_moment_epoch(JAN5 + 50 * DAY).await.unwrap();
        assert_eq!(
            pending(&s).await,
            vec![],
            "the first launch after moments existed is not a ceremony"
        );
    }

    #[tokio::test]
    async fn the_default_epoch_is_now_so_a_fresh_migration_shows_nothing_old() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let b = book_at(&s, "Old News", JAN5).await;
        s.record_reading(b, Some(JAN5), Some(JAN5 + DAY), "finished", "manual")
            .await
            .unwrap();
        assert_eq!(
            s.pending_moments(now_unix(), None).await.unwrap(),
            vec![],
            "the migration's own epoch is what makes the upgrade quiet"
        );
    }

    // ---- fires once -------------------------------------------------------

    #[tokio::test]
    async fn an_acknowledged_moment_never_comes_back() {
        let (s, b) = seeded().await;
        s.record_reading(b, Some(JAN5), Some(JAN5 + 5 * DAY), "finished", "manual")
            .await
            .unwrap();
        note_on(&s, b, "n", "note", JAN5).await;

        let first = pending(&s).await;
        assert!(first.len() >= 2);
        for m in &first {
            s.acknowledge_moment(&m.id).await.unwrap();
        }
        assert_eq!(pending(&s).await, vec![]);
    }

    #[tokio::test]
    async fn acknowledging_twice_writes_one_row_and_keeps_the_first_time() {
        let (s, b) = seeded().await;
        s.record_reading(b, Some(JAN5), Some(JAN5 + 5 * DAY), "finished", "manual")
            .await
            .unwrap();
        let m = pending(&s).await.into_iter().next().unwrap();

        s.acknowledge_moment(&m.id).await.unwrap();
        let at: i64 = sqlx::query_scalar("SELECT surfaced_at FROM moments WHERE id = ?")
            .bind(&m.id)
            .fetch_one(s.pool())
            .await
            .unwrap();

        s.acknowledge_moment(&m.id).await.unwrap();
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM moments")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(rows, 1);
        let again: i64 = sqlx::query_scalar("SELECT surfaced_at FROM moments WHERE id = ?")
            .bind(&m.id)
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(again, at, "the first surfacing is the one that is true");
    }

    #[tokio::test]
    async fn an_id_of_no_known_kind_is_refused() {
        let (s, _) = seeded().await;
        for bad in ["", "nonsense:1", "reading_closed", "reading_closed:"] {
            assert!(
                matches!(
                    s.acknowledge_moment(bad).await,
                    Err(EngineError::InvalidInput(_))
                ),
                "{bad:?} should not be acknowledgeable"
            );
        }
    }

    #[tokio::test]
    async fn the_list_is_newest_first_and_limit_takes_from_the_top() {
        let (s, b) = seeded().await;
        let c = book_at(&s, "Second", LONG_AGO).await;
        s.record_reading(b, Some(JAN5), Some(JAN5 + DAY), "finished", "manual")
            .await
            .unwrap();
        s.record_reading(c, Some(JAN5), Some(JAN5 + 9 * DAY), "finished", "manual")
            .await
            .unwrap();

        let all = pending(&s).await;
        assert!(all.windows(2).all(|w| w[0].occurred_at >= w[1].occurred_at));
        let one = s.pending_moments(NOW, Some(1)).await.unwrap();
        assert_eq!(ids(&one), ids(&all[..1]));
    }

    #[tokio::test]
    async fn deleting_a_book_takes_its_moments_with_it() {
        let (s, b) = seeded().await;
        s.record_reading(b, Some(JAN5), Some(JAN5 + DAY), "finished", "manual")
            .await
            .unwrap();
        assert!(!pending(&s).await.is_empty());
        s.delete_book(b).await.unwrap();
        assert_eq!(
            pending(&s).await,
            vec![],
            "a moment is derived, so deleting its evidence deletes it"
        );
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        /// Days inside one month, so a generator produces adjacency often
        /// enough for the runs to be interesting.
        fn day_set() -> impl Strategy<Value = Vec<String>> {
            prop::collection::vec(1u32..=28, 0..20)
                .prop_map(|ds| ds.into_iter().map(|d| format!("2026-01-{d:02}")).collect())
        }

        proptest! {
            /// The three halves of "maximal consecutive run", each stated as
            /// something a generator can break. Asserting the word instead
            /// would assert nothing.
            #[test]
            fn runs_partition_the_days_in_order(days in day_set()) {
                let runs = runs_of_days(&days);

                let mut want: Vec<time::Date> =
                    days.iter().filter_map(|d| parse_day(d)).collect();
                want.sort_unstable();
                want.dedup();

                // 1. Every run is consecutive, and its length is its span.
                let mut got = Vec::new();
                for (from, to, len) in &runs {
                    let (a, b) = (parse_day(from).unwrap(), parse_day(to).unwrap());
                    prop_assert!(a <= b);
                    prop_assert_eq!((b - a).whole_days() as usize + 1, *len);
                    let mut d = a;
                    loop {
                        got.push(d);
                        if d == b { break; }
                        d = d.next_day().unwrap();
                    }
                }

                // 2. Together they are exactly the input, in order.
                prop_assert_eq!(&got, &want);

                // 3. And no run can be extended: the day after one run's end
                //    is never the day the next run starts.
                for w in runs.windows(2) {
                    let end = parse_day(&w[0].1).unwrap();
                    let next = parse_day(&w[1].0).unwrap();
                    prop_assert!(next > end.next_day().unwrap());
                }
            }

            /// Order and duplicates in the input change nothing. The days
            /// arrive from a `SELECT DISTINCT … ORDER BY` today, and a moment
            /// that depended on that staying true would break silently.
            #[test]
            fn runs_do_not_depend_on_the_order_they_arrive_in(days in day_set()) {
                let sorted = runs_of_days(&days);
                let mut shuffled = days.clone();
                shuffled.reverse();
                shuffled.extend(days.iter().cloned());
                prop_assert_eq!(runs_of_days(&shuffled), sorted);
            }
        }
    }

    /// The invariant the item is named for, as a property rather than as one
    /// example: whatever order moments are surfaced and acknowledged in, and
    /// however many times, **no moment is ever offered twice after it has been
    /// acknowledged**, and acknowledging everything empties the list.
    #[tokio::test]
    async fn a_moment_fires_at_most_once_however_the_calls_interleave() {
        let (s, b) = seeded().await;
        let c = book_at(&s, "Middlemarch", LONG_AGO).await;
        s.record_reading(b, Some(JAN5), Some(JAN5 + 2 * DAY), "finished", "manual")
            .await
            .unwrap();
        s.record_reading(c, Some(JAN5), Some(JAN5 + 4 * DAY), "finished", "manual")
            .await
            .unwrap();
        note_on(&s, b, "n1", "note", JAN5).await;
        note_on(&s, b, "n2", "note", JAN5 + DAY).await;
        note_on(&s, c, "n3", "note", JAN5 + 2 * DAY).await;
        s.refill_reading_events().await.unwrap();

        let mut seen: Vec<String> = Vec::new();
        // One at a time, re-deriving between each — the way a frontend that
        // polls, shows one, and acknowledges it actually behaves.
        loop {
            let batch = s.pending_moments(NOW, Some(1)).await.unwrap();
            let Some(m) = batch.into_iter().next() else {
                break;
            };
            assert!(!seen.contains(&m.id), "{} was offered twice", m.id);
            seen.push(m.id.clone());
            s.acknowledge_moment(&m.id).await.unwrap();
            // A second acknowledgement mid-stream must change nothing.
            s.acknowledge_moment(&m.id).await.unwrap();
        }
        assert!(seen.len() >= 4, "the fixture is meant to be interesting");
        assert_eq!(pending(&s).await, vec![]);

        // And nothing new appears once the data stops moving.
        assert_eq!(
            s.pending_moments(NOW + 10 * DAY, None).await.unwrap(),
            vec![]
        );
    }
}
