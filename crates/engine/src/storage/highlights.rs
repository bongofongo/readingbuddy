use sha2::{Digest, Sha256};
use sqlx::Row;

use super::{Storage, now_unix};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct NewHighlight {
    pub text: String,
    pub chapter: Option<String>,
    pub page: Option<i64>,
    pub pos0: Option<String>,
    pub pos1: Option<String>,
    pub ko_datetime: Option<String>,
    /// KOReader's `datetime_updated`: when the annotation was last *edited* on
    /// the device, as distinct from when it was created.
    ///
    /// **Parsed but not yet persisted** — the column arrives with item 2's
    /// ownership migration, which is what will use it to tell "the device
    /// changed this" from "nothing happened". It is carried here rather than in
    /// a side channel so that item 2 does not have to reopen the parser, the
    /// fixtures and every golden just to add one field.
    ///
    /// It must never enter `identity_hash`: `datetime` is KOReader's immutable
    /// creation stamp and this one moves on every edit, so hashing it would
    /// make an edited highlight re-import as a duplicate row. See
    /// `docs/koreader-format.md` §1.
    pub ko_datetime_updated: Option<String>,
    pub color: Option<String>,
    pub note: Option<String>,
    pub source: String,
}

impl NewHighlight {
    /// Stable identity for idempotent imports.
    ///
    /// Deliberately excludes everything the device may rewrite in place:
    /// `chapter`, `page`, `color`, `note` and `ko_datetime_updated`. Only
    /// `ko_datetime` (creation time, never changed by KOReader), `pos0` and the
    /// highlighted text take part.
    pub fn identity_hash(&self, book_id: i64) -> String {
        identity_hash_of(
            book_id,
            self.ko_datetime.as_deref(),
            self.pos0.as_deref(),
            &self.text,
        )
    }
}

/// The identity hash from loose parts.
///
/// Exists so [`Storage::merge_books`] can recompute a stored highlight's hash
/// against its new `book_id` — `book_id` is one of the inputs, so a moved
/// highlight's hash is stale the moment it moves. Recomputing it there with a
/// second copy of this formula would be a silent duplicate-detection bug the
/// day either copy changed, so there is one copy and both callers use it.
pub(crate) fn identity_hash_of(
    book_id: i64,
    ko_datetime: Option<&str>,
    pos0: Option<&str>,
    text: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(book_id.to_string());
    hasher.update("|");
    hasher.update(ko_datetime.unwrap_or(""));
    hasher.update("|");
    hasher.update(pos0.unwrap_or(""));
    hasher.update("|");
    hasher.update(text);
    format!("{:x}", hasher.finalize())
}

/// One digest over the device-owned state of a set of highlights.
///
/// Exists so the two sides of the device scan's comparison — what the sidecar
/// file says, and what the library already holds — are hashed by **one** piece
/// of code. Two copies of this formula would drift the day either changed, and
/// the symptom would be a scan that silently stopped noticing device edits.
/// Same rule as [`identity_hash_of`] and `DEVICE_FIELDS_DIFFER`.
///
/// The fields are the identity (`ko_datetime`, `pos0`, `text`) plus exactly the
/// four the device owns. `annotation` is ours and is deliberately absent: a
/// note the reader wrote here must not make the device look changed.
///
/// Sorted in [`DeviceDigest::finish`] rather than by the caller, so neither
/// side has to promise an ordering — a `SELECT` without an `ORDER BY` and a
/// sidecar's own sequence are then equally fine.
#[derive(Default)]
pub(crate) struct DeviceDigest {
    parts: Vec<String>,
}

/// One annotation's identity plus the four fields the device owns — the exact
/// field set [`DeviceDigest`] hashes, named once so a sidecar row and a stored
/// row cannot be assembled differently.
pub(crate) struct DeviceEntry<'a> {
    pub ko_datetime: Option<&'a str>,
    pub pos0: Option<&'a str>,
    pub text: &'a str,
    pub ko_note: Option<&'a str>,
    pub color: Option<&'a str>,
    pub chapter: Option<&'a str>,
    pub page: Option<i64>,
}

impl NewHighlight {
    pub(crate) fn device_entry(&self) -> DeviceEntry<'_> {
        DeviceEntry {
            ko_datetime: self.ko_datetime.as_deref(),
            pos0: self.pos0.as_deref(),
            text: &self.text,
            ko_note: self.note.as_deref(),
            color: self.color.as_deref(),
            chapter: self.chapter.as_deref(),
            page: self.page,
        }
    }
}

impl DeviceDigest {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&mut self, e: DeviceEntry<'_>) {
        // `\u{1f}` (unit separator) cannot occur in any of these, so no value
        // can impersonate a field boundary.
        self.parts.push(format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            e.ko_datetime.unwrap_or(""),
            e.pos0.unwrap_or(""),
            e.text,
            e.ko_note.unwrap_or(""),
            e.color.unwrap_or(""),
            e.chapter.unwrap_or(""),
            e.page.map(|p| p.to_string()).unwrap_or_default(),
        ));
    }

    pub(crate) fn finish(mut self) -> String {
        self.parts.sort();
        let mut hasher = Sha256::new();
        for p in &self.parts {
            hasher.update(p);
            hasher.update("\u{1e}");
        }
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug, Clone)]
pub struct Highlight {
    pub id: i64,
    pub book_id: i64,
    pub text: String,
    pub chapter: Option<String>,
    pub page: Option<i64>,
    /// KOReader's note, theirs. Refreshed from the device on every import.
    pub ko_note: Option<String>,
    /// The reader's own annotation, ours. Import never touches it.
    pub annotation: Option<String>,
    pub ko_datetime: Option<String>,
    /// Which reading of the book this was captured during, when that can be
    /// worked out — [`Storage::attribute_highlights`] matches `ko_datetime` into
    /// a reading's window and this is its answer.
    ///
    /// **`None` is an ordinary outcome, not a failure.** KOReader's sidecar is
    /// per-file and a reread appends to the same file, so the device cannot
    /// supply this and a highlight captured between two readings genuinely
    /// belongs to neither. A frontend showing highlights per reading has to have
    /// somewhere for these to go; it must not drop them, since `book_id` is
    /// still authoritative and the highlight is real.
    ///
    /// Derived, never entered: it is recomputed from scratch on every import, so
    /// writing to it directly would be overwritten on the next sync.
    pub reading_id: Option<i64>,
    /// Where the row came from (`koreader`). Provenance, not payload — a device
    /// refresh must leave it alone.
    pub source: String,
    /// When *we* first stored it, unix seconds. Distinct from `ko_datetime`,
    /// which is when the device captured it, and equally not something a
    /// refresh may move.
    pub created_at: i64,
}

/// Every column a [`Highlight`] is built from, and the mapper that builds it.
///
/// Shared rather than inlined per query: `citations_for` in [`super::notes`]
/// returns highlights too, and a second hand-written projection is how the two
/// drift into disagreeing about what a highlight is.
pub(super) const HIGHLIGHT_COLUMNS: &str = "id, book_id, text, chapter, page, ko_note, annotation, ko_datetime, reading_id, \
     source, created_at";

pub(super) fn row_to_highlight(r: &sqlx::sqlite::SqliteRow) -> Highlight {
    Highlight {
        id: r.get("id"),
        book_id: r.get("book_id"),
        text: r.get("text"),
        chapter: r.get("chapter"),
        page: r.get("page"),
        ko_note: r.get("ko_note"),
        annotation: r.get("annotation"),
        ko_datetime: r.get("ko_datetime"),
        reading_id: r.get("reading_id"),
        source: r.get("source"),
        created_at: r.get("created_at"),
    }
}

/// The device-owned payload columns, and the null-safe test for "the sidecar
/// disagrees with what we stored".
///
/// One copy, shared by the conditional `UPDATE` in [`Storage::refresh_device_fields`]
/// and the read-only `SELECT` in [`Storage::device_fields_differ`], because a
/// dry run that previewed a different set of changes than the real import would
/// be worse than no preview at all.
///
/// `IS NOT` rather than `!=`: SQLite's `!=` yields NULL when either side is
/// NULL, so a note the user *deleted* on the device (`'x'` → NULL) would
/// compare as "no change" and never be removed.
const DEVICE_FIELDS_DIFFER: &str =
    "(ko_note IS NOT ?3 OR color IS NOT ?4 OR chapter IS NOT ?5 OR page IS NOT ?6)";

impl Storage {
    /// Insert one highlight; returns Some(id) if newly inserted, None if it
    /// already existed (identity_hash conflict).
    ///
    /// The conflict clause stays `DO NOTHING` on purpose. `DO UPDATE` would
    /// make `RETURNING id` yield a row on the conflict path too, so `Some(id)`
    /// would stop meaning "newly inserted" and the inserted/skipped counts —
    /// which the goldens assert — would collapse into each other. Refreshing a
    /// row the device changed is [`Storage::refresh_device_fields`]'s job.
    pub async fn insert_highlight(&self, book_id: i64, h: &NewHighlight) -> Result<Option<i64>> {
        let row = sqlx::query(
            r#"INSERT INTO highlights
                (book_id, text, chapter, page, pos0, pos1, ko_datetime, color, ko_note,
                 last_seen_ko_note, source, identity_hash, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(book_id, identity_hash) DO NOTHING
               RETURNING id"#,
        )
        .bind(book_id)
        .bind(&h.text)
        .bind(h.chapter.as_ref())
        .bind(h.page)
        .bind(h.pos0.as_ref())
        .bind(h.pos1.as_ref())
        .bind(h.ko_datetime.as_ref())
        .bind(h.color.as_ref())
        .bind(h.note.as_ref())
        // Seeded here as well as in the refresh: we have just seen this value
        // on the device, and a row whose `ko_note` is set while
        // `last_seen_ko_note` is NULL would read, to the two-way sync this
        // column exists for, as "the device never said anything" — which is the
        // one thing it must never say.
        .bind(h.note.as_ref())
        .bind(&h.source)
        .bind(h.identity_hash(book_id))
        .bind(now_unix())
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| r.get("id")))
    }

    /// Refresh the device-owned payload of an existing highlight. Returns true
    /// if a column actually changed.
    ///
    /// **Straight assignment, not `COALESCE`.** The books upsert's
    /// `COALESCE(excluded.x, books.x)` no-clobber pattern is right for
    /// *providers*, which return partial records — a missing field there means
    /// "I don't know". A sidecar is the *complete* state of that annotation, so
    /// a missing note means the user **deleted** it, and copying the books
    /// pattern here would make note deletion impossible to sync, permanently.
    ///
    /// Never touches `annotation`, `text`, `source`, `created_at` or `id`. In
    /// particular the row is updated in place rather than replaced:
    /// `notes.highlight_id` and `flashcards.highlight_id` are foreign keys, so a
    /// delete-and-reinsert would null note anchors and cascade flashcards away.
    ///
    /// One conditional statement rather than read-compare-write: the comparison
    /// has to be exact for the caller's counter to mean anything, and doing it
    /// in SQL keeps it atomic as well as single-copy.
    pub async fn refresh_device_fields(&self, book_id: i64, h: &NewHighlight) -> Result<bool> {
        let sql = format!(
            r#"UPDATE highlights SET
                   ko_note           = ?3,
                   color             = ?4,
                   chapter           = ?5,
                   page              = ?6,
                   last_seen_ko_note = ?3
               WHERE book_id = ?1 AND identity_hash = ?2 AND {DEVICE_FIELDS_DIFFER}"#
        );
        let done = sqlx::query(&sql)
            .bind(book_id)
            .bind(h.identity_hash(book_id))
            .bind(h.note.as_ref())
            .bind(h.color.as_ref())
            .bind(h.chapter.as_ref())
            .bind(h.page)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Would [`Storage::refresh_device_fields`] change anything? Read-only, for
    /// the dry-run preview.
    pub async fn device_fields_differ(&self, book_id: i64, h: &NewHighlight) -> Result<bool> {
        let sql = format!(
            "SELECT count(*) FROM highlights
             WHERE book_id = ?1 AND identity_hash = ?2 AND {DEVICE_FIELDS_DIFFER}"
        );
        let n: i64 = sqlx::query_scalar(&sql)
            .bind(book_id)
            .bind(h.identity_hash(book_id))
            .bind(h.note.as_ref())
            .bind(h.color.as_ref())
            .bind(h.chapter.as_ref())
            .bind(h.page)
            .fetch_one(self.pool())
            .await?;
        Ok(n > 0)
    }

    /// Set the reader's own annotation on a highlight. Ours; the device never
    /// sees it and import never overwrites it.
    pub async fn set_annotation(&self, highlight_id: i64, annotation: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE highlights SET annotation = ? WHERE id = ?")
            .bind(annotation)
            .bind(highlight_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// The device-owned state of everything this book holds from a device, as
    /// one digest.
    ///
    /// The device scan's cheap half. Compared against the digest cached for a
    /// sidecar, it decides whether an unmodified file can be called `Unchanged`
    /// without re-parsing it — and, unlike a row count, it notices a note the
    /// device rewrote in place, which is the case that would otherwise be
    /// reported once and then silently forgotten forever.
    ///
    /// Restricted to `source = 'koreader'` so a highlight added by any other
    /// route cannot make a sidecar look imported.
    pub async fn device_highlight_digest(&self, book_id: i64) -> Result<String> {
        let rows = sqlx::query(
            "SELECT ko_datetime, pos0, text, ko_note, color, chapter, page
             FROM highlights WHERE book_id = ? AND source = 'koreader'",
        )
        .bind(book_id)
        .fetch_all(self.pool())
        .await?;

        let mut digest = DeviceDigest::new();
        for r in &rows {
            let (ko_datetime, pos0, text, ko_note, color, chapter) = (
                r.get::<Option<String>, _>("ko_datetime"),
                r.get::<Option<String>, _>("pos0"),
                r.get::<String, _>("text"),
                r.get::<Option<String>, _>("ko_note"),
                r.get::<Option<String>, _>("color"),
                r.get::<Option<String>, _>("chapter"),
            );
            digest.add(DeviceEntry {
                ko_datetime: ko_datetime.as_deref(),
                pos0: pos0.as_deref(),
                text: &text,
                ko_note: ko_note.as_deref(),
                color: color.as_deref(),
                chapter: chapter.as_deref(),
                page: r.get::<Option<i64>, _>("page"),
            });
        }
        Ok(digest.finish())
    }

    pub async fn highlight_exists(&self, book_id: i64, h: &NewHighlight) -> Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM highlights WHERE book_id = ? AND identity_hash = ?",
        )
        .bind(book_id)
        .bind(h.identity_hash(book_id))
        .fetch_one(self.pool())
        .await?;
        Ok(n > 0)
    }

    /// Which book a highlight belongs to — `None` when there is no such
    /// highlight.
    ///
    /// One statement answering both halves of *is this passage a passage of
    /// that book*, which is the question `Engine::create_flashcard` has to ask
    /// before it writes a card pointing at one. Deliberately not a
    /// `get_highlight`: nothing needs the row, and returning the whole thing
    /// would put the reader's private text on a path whose only output is a
    /// yes or a no.
    pub async fn highlight_book(&self, highlight_id: i64) -> Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT book_id FROM highlights WHERE id = ?")
                .bind(highlight_id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn list_highlights(&self, book_id: i64) -> Result<Vec<Highlight>> {
        let sql = format!(
            "SELECT {HIGHLIGHT_COLUMNS} FROM highlights WHERE book_id = ?
             ORDER BY page ASC, ko_datetime ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(book_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.iter().map(row_to_highlight).collect())
    }

    /// What was highlighted during one reading, in the same order
    /// [`Storage::list_highlights`] uses.
    ///
    /// A `Reading` is a first-class row that a reflection and a review already
    /// anchor to, so asking one what was highlighted during it is the natural
    /// question — and answering it by pulling the whole book's highlights and
    /// filtering in the frontend is the sort of thing two frontends do
    /// differently.
    ///
    /// It deliberately cannot ask for the **unattributed** ones: `reading_id IS
    /// NULL` is a property of the *book's* list, not of any reading, and a
    /// method that answered it here would need a book id anyway. Filter
    /// `list_highlights` for those — the field is on the row precisely so that
    /// grouping is a frontend's to do.
    pub async fn highlights_for_reading(&self, reading_id: i64) -> Result<Vec<Highlight>> {
        let sql = format!(
            "SELECT {HIGHLIGHT_COLUMNS} FROM highlights WHERE reading_id = ?
             ORDER BY page ASC, ko_datetime ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(reading_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.iter().map(row_to_highlight).collect())
    }

    /// The one passage a card shows for a reading (item 44), or `None`.
    ///
    /// **The rule is the longest passage of the reading, ties broken by the
    /// lowest `id`.** That is a *selection predicate*, which item 17 puts in
    /// the engine and not in a frontend: `highlights[0]` in TypeScript is a
    /// frontend inventing a rule, and the day the TUI grows a card the two apps
    /// show a different passage for the same reading with neither looking
    /// wrong.
    ///
    /// Why length, when "the first one" and "the one you annotated" were both
    /// on the table:
    ///
    /// - **This database already treats a short highlight as not-a-passage.**
    ///   `koreader.rs` turns single-word highlights into flashcard candidates,
    ///   because a one-word mark on a reader is a dictionary lookup rather than
    ///   something the reader wanted to keep. Ordering by length is the cheapest
    ///   rule that lands on the other kind, and any rule keyed on *position*
    ///   (first marked, lowest page) picks those vocabulary marks constantly —
    ///   they are scattered through a book and one of them is usually near the
    ///   front.
    /// - **It needs nothing that is usually missing.** `annotation`, `ko_note`
    ///   and a citation are all better signals of "the passage that mattered"
    ///   and all three are absent for most readings, so a rule resting on them
    ///   makes the ordinary card the empty one.
    /// - **It is stable across a device re-import**, which the alternatives are
    ///   not obviously. `refresh_device_fields` rewrites `ko_note`, `color`,
    ///   `chapter` and `page` in place and never `text`, and highlight ids are
    ///   asserted stable across that refresh
    ///   (`highlight_ids_are_stable_across_refresh`), so both the key and the
    ///   tie-break survive a sync. A rule ordering by `page` would be reordered
    ///   by one, since a re-render moves page numbers.
    ///
    /// **What it costs, plainly: it selects for the longest drag, not the best
    /// passage.** A mis-drag that grabbed half a screen outranks the sentence
    /// the reader actually loved, and a reading whose marks are all one
    /// sentence long is decided by a few characters. A length *cap* would only
    /// trade that for a magic number making a claim about how long a passage is
    /// allowed to be, so the cost is stated rather than papered over.
    ///
    /// `length()` counts **characters**, not bytes, which is the honest measure
    /// of how much was dragged: on bytes a CJK passage would score three times a
    /// Latin one of the same length and win every card in a mixed library.
    ///
    /// **Scoped to the reading, exactly like [`Storage::highlights_for_reading`]
    /// and for the card's own reason** — the card is per reading, so two reads
    /// of one book must be able to show different passages, which is the
    /// comparison the card exists to make. It follows that a reading whose
    /// highlights are all unattributed has no card passage: `reading_id` is
    /// `None` for an ordinary and well-understood set of highlights, so this
    /// returns `None` the same way `highlights_for_reading` returns an empty
    /// list, and a card drawing that absence *as* an absence is right.
    ///
    /// The order is **total** — `id` is the primary key, so no two rows tie all
    /// the way down — which is what makes "stable across calls" a property of
    /// the statement rather than of SQLite's query plan. `storage/CLAUDE.md`
    /// records the same requirement for the paged list arms, and for the same
    /// reason: a partial order is deterministic in testing right up until the
    /// plan changes.
    pub async fn card_passage(&self, reading_id: i64) -> Result<Option<Highlight>> {
        let sql = format!(
            "SELECT {HIGHLIGHT_COLUMNS} FROM highlights WHERE reading_id = ?
             ORDER BY length(text) DESC, id ASC
             LIMIT 1"
        );
        let row = sqlx::query(&sql)
            .bind(reading_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.as_ref().map(row_to_highlight))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;

    fn hl(text: &str) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: Some("Ch 1".into()),
            page: Some(42),
            pos0: Some("/body/DocFragment[8]/p[3]/text().0".into()),
            pos1: None,
            ko_datetime: Some("2026-01-01 10:00:00".into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    /// The invariant `docs/koreader-format.md` §1 establishes, asserted rather
    /// than trusted: KOReader leaves `datetime` alone when a note is edited and
    /// stamps `datetime_updated` instead. If that field ever reached the hash,
    /// every edited highlight would come back as a second row on the next
    /// import — silently, with nothing on screen looking wrong.
    #[test]
    fn identity_survives_an_edit_on_the_device() {
        let base = hl("a phrase worth keeping");

        let edited = NewHighlight {
            ko_datetime_updated: Some("2026-06-01 12:00:00".into()),
            note: Some("a note the user typed later".into()),
            // A re-render moves page numbers with no user action at all.
            page: Some(43),
            chapter: Some("Ch 1 (renamed)".into()),
            color: Some("gray".into()),
            ..base.clone()
        };

        assert_eq!(
            base.identity_hash(1),
            edited.identity_hash(1),
            "device-owned fields must not take part in identity"
        );
    }

    #[test]
    fn identity_still_tracks_the_fields_it_should() {
        let base = hl("a phrase worth keeping");
        for changed in [
            NewHighlight {
                text: "a different phrase".into(),
                ..base.clone()
            },
            NewHighlight {
                pos0: Some("/body/DocFragment[9]/p[1]/text().0".into()),
                ..base.clone()
            },
            NewHighlight {
                ko_datetime: Some("2026-01-02 10:00:00".into()),
                ..base.clone()
            },
        ] {
            assert_ne!(base.identity_hash(1), changed.identity_hash(1));
        }
        // book_id is an input, so the same annotation under a different book is
        // a different row — this is what item 3's merge has to recompute.
        assert_ne!(base.identity_hash(1), base.identity_hash(2));
    }

    #[tokio::test]
    async fn double_insert_is_idempotent() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book_id = s
            .upsert_book(
                &Book {
                    title: Some("T".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let h = hl("a phrase worth keeping");
        assert!(s.insert_highlight(book_id, &h).await.unwrap().is_some());
        assert!(s.insert_highlight(book_id, &h).await.unwrap().is_none());
        assert_eq!(s.list_highlights(book_id).await.unwrap().len(), 1);
        assert!(s.highlight_exists(book_id, &h).await.unwrap());
    }

    async fn seeded() -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book_id = s
            .upsert_book(
                &Book {
                    title: Some("T".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        (s, book_id)
    }

    /// The counter the CLI prints comes straight off this boolean, so it has to
    /// be exact in both directions: a no-op refresh must report false.
    #[tokio::test]
    async fn a_refresh_reports_only_a_real_change() {
        let (s, book_id) = seeded().await;
        let h = hl("a phrase worth keeping");
        s.insert_highlight(book_id, &h).await.unwrap();

        assert!(
            !s.refresh_device_fields(book_id, &h).await.unwrap(),
            "identical payload is not an update"
        );
        assert!(!s.device_fields_differ(book_id, &h).await.unwrap());

        let edited = NewHighlight {
            note: Some("typed later on the device".into()),
            ..h.clone()
        };
        assert!(s.device_fields_differ(book_id, &edited).await.unwrap());
        assert!(s.refresh_device_fields(book_id, &edited).await.unwrap());
        assert!(
            !s.refresh_device_fields(book_id, &edited).await.unwrap(),
            "the second pass has nothing left to do"
        );

        let stored = &s.list_highlights(book_id).await.unwrap()[0];
        assert_eq!(stored.ko_note.as_deref(), Some("typed later on the device"));
    }

    /// Every device-owned column, one at a time. A refresh that quietly ignored
    /// `color` would look exactly like a refresh that worked.
    #[tokio::test]
    async fn every_device_owned_field_refreshes() {
        let base = hl("a phrase worth keeping");
        for edited in [
            NewHighlight {
                note: Some("a note".into()),
                ..base.clone()
            },
            NewHighlight {
                color: Some("cyan".into()),
                ..base.clone()
            },
            NewHighlight {
                chapter: Some("Ch 2".into()),
                ..base.clone()
            },
            NewHighlight {
                page: Some(43),
                ..base.clone()
            },
        ] {
            let (s, book_id) = seeded().await;
            s.insert_highlight(book_id, &base).await.unwrap();
            assert!(
                s.refresh_device_fields(book_id, &edited).await.unwrap(),
                "a changed device field must count as an update"
            );
        }
    }

    /// The `COALESCE` trap, asserted rather than trusted. A note the user
    /// deleted on the device arrives as absent, and absent means gone — copy
    /// the books upsert's no-clobber merge here and deletion becomes
    /// impossible to sync, permanently.
    #[tokio::test]
    async fn deleting_a_note_on_the_device_deletes_it_here() {
        let (s, book_id) = seeded().await;
        let with_note = NewHighlight {
            note: Some("regretted immediately".into()),
            ..hl("a phrase worth keeping")
        };
        s.insert_highlight(book_id, &with_note).await.unwrap();

        let cleared = NewHighlight {
            note: None,
            ..with_note.clone()
        };
        assert!(s.refresh_device_fields(book_id, &cleared).await.unwrap());
        assert_eq!(s.list_highlights(book_id).await.unwrap()[0].ko_note, None);
    }

    /// Ours survives theirs. `annotation` is the one column import may never
    /// write, and a refresh is the moment it would be lost.
    #[tokio::test]
    async fn a_refresh_never_touches_our_annotation() {
        let (s, book_id) = seeded().await;
        let h = hl("a phrase worth keeping");
        let id = s.insert_highlight(book_id, &h).await.unwrap().unwrap();
        s.set_annotation(id, Some("what I actually think"))
            .await
            .unwrap();

        let edited = NewHighlight {
            note: Some("typed later on the device".into()),
            ..h.clone()
        };
        assert!(s.refresh_device_fields(book_id, &edited).await.unwrap());

        let stored = &s.list_highlights(book_id).await.unwrap()[0];
        assert_eq!(stored.id, id, "the row is updated in place, not replaced");
        assert_eq!(stored.annotation.as_deref(), Some("what I actually think"));
    }

    /// `last_seen_ko_note` is the whole reason the column exists: without it a
    /// future two-way sync cannot tell "the user changed it here" from "the
    /// device changed it there". It must track the device from the first insert,
    /// not only from the first refresh.
    #[tokio::test]
    async fn last_seen_tracks_the_device_from_the_very_first_import() {
        let (s, book_id) = seeded().await;
        let h = NewHighlight {
            note: Some("as captured".into()),
            ..hl("a phrase worth keeping")
        };
        s.insert_highlight(book_id, &h).await.unwrap();

        let last_seen = |s: Storage| async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT last_seen_ko_note FROM highlights LIMIT 1",
            )
            .fetch_one(s.pool())
            .await
            .unwrap()
        };
        assert_eq!(last_seen(s.clone()).await.as_deref(), Some("as captured"));

        let edited = NewHighlight {
            note: Some("as edited".into()),
            ..h.clone()
        };
        s.refresh_device_fields(book_id, &edited).await.unwrap();
        assert_eq!(last_seen(s.clone()).await.as_deref(), Some("as edited"));
    }

    /// A refresh is scoped to one row of one book. Two books can hold the same
    /// annotation (the identity hash takes `book_id`, so the hashes differ), and
    /// a `WHERE` clause missing `book_id` would still be green against a
    /// single-book fixture.
    #[tokio::test]
    async fn a_refresh_does_not_reach_into_another_book() {
        let (s, first) = seeded().await;
        let second = s
            .upsert_book(
                &Book {
                    title: Some("Another".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let h = hl("a phrase worth keeping");
        s.insert_highlight(first, &h).await.unwrap();
        s.insert_highlight(second, &h).await.unwrap();

        let edited = NewHighlight {
            note: Some("only on the first".into()),
            ..h.clone()
        };
        assert!(s.refresh_device_fields(first, &edited).await.unwrap());
        assert_eq!(s.list_highlights(second).await.unwrap()[0].ko_note, None);
    }

    // ---- the device digest -------------------------------------------------

    fn digest_of(hs: &[NewHighlight]) -> String {
        let mut d = DeviceDigest::new();
        for h in hs {
            d.add(h.device_entry());
        }
        d.finish()
    }

    /// The two sides of the scan's comparison arrive in different orders — a
    /// sidecar's own sequence against a `SELECT` with no `ORDER BY` — so the
    /// digest cannot depend on one.
    #[test]
    fn the_digest_does_not_depend_on_the_order_it_was_fed() {
        let a = hl("first");
        let mut b = hl("second");
        b.ko_datetime = Some("2026-02-02 10:00:00".into());
        let mut c = hl("third");
        c.page = Some(9);

        let forward = digest_of(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(digest_of(&[c.clone(), a.clone(), b.clone()]), forward);
        assert_eq!(digest_of(&[b, c, a]), forward);
    }

    /// Every field the device owns has to move the digest, or the scan's cheap
    /// path would call that change "unchanged" and never look again.
    #[test]
    fn every_device_owned_field_moves_the_digest() {
        let base = digest_of(&[hl("passage")]);
        let moves = |name: &str, mutate: &dyn Fn(&mut NewHighlight)| {
            let mut h = hl("passage");
            mutate(&mut h);
            assert_ne!(digest_of(&[h]), base, "{name} did not move the digest");
        };
        moves("text", &|h| h.text = "different".into());
        moves("ko_datetime", &|h| {
            h.ko_datetime = Some("2020-01-01 00:00:00".into())
        });
        moves("pos0", &|h| h.pos0 = Some("/body/p[9]/text().0".into()));
        moves("note", &|h| h.note = Some("edited on the device".into()));
        moves("color", &|h| h.color = Some("cyan".into()));
        moves("chapter", &|h| h.chapter = Some("Ch 2".into()));
        moves("page", &|h| h.page = Some(43));
    }

    /// A field separator that a value could contain would let two different
    /// sets of annotations hash the same.
    #[test]
    fn a_value_cannot_impersonate_a_field_boundary() {
        let mut split = hl("a");
        split.chapter = Some("b".into());
        let mut joined = hl("a\u{1f}b");
        joined.chapter = None;
        assert_ne!(digest_of(&[split]), digest_of(&[joined]));
    }

    /// The ownership seam, on the scan's side: `annotation` is ours, and the
    /// reader writing one must not make the device look like it changed.
    #[tokio::test]
    async fn our_own_annotation_is_not_part_of_the_device_digest() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let h = hl("a passage");
        let id = s.insert_highlight(book, &h).await.unwrap().unwrap();

        let before = s.device_highlight_digest(book).await.unwrap();
        assert_eq!(
            before,
            digest_of(std::slice::from_ref(&h)),
            "both sides, one formula"
        );

        s.set_annotation(id, Some("what I thought about it"))
            .await
            .unwrap();
        assert_eq!(
            s.device_highlight_digest(book).await.unwrap(),
            before,
            "ours must not read as a device change"
        );

        // A highlight from anywhere else is not the device's either.
        let mut mine = hl("something I typed");
        mine.source = "manual".into();
        s.insert_highlight(book, &mine).await.unwrap();
        assert_eq!(s.device_highlight_digest(book).await.unwrap(), before);
    }
}

/// Properties of the card's passage rule (item 44).
///
/// The rule is *an ordering*, and an ordering is the kind of claim more examples
/// cover badly: whether the right row wins depends on how the lengths happen to
/// be arranged, and a hand-written case is arranged by whoever wrote the
/// implementation. Ties are generated deliberately dense here, since a tie is
/// where "deterministic" stops being free.
#[cfg(test)]
mod props {
    use super::*;
    use crate::book::Book;
    use proptest::prelude::*;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// A mark of `n + 1` characters, in one of two scripts, with an anchor that
    /// makes it its own row. The scripts differ in bytes per character, so a
    /// rule that measured bytes disagrees with the expected answer here rather
    /// than only in a fixture somebody remembered to write.
    fn mark(n: usize, cjk: bool, seq: usize) -> NewHighlight {
        let text = if cjk { "極" } else { "x" }.repeat(n + 1);
        NewHighlight {
            text,
            chapter: None,
            page: Some(1),
            pos0: Some(format!("/body/p[{seq}]/text().0")),
            pos1: None,
            ko_datetime: Some("2026-01-05 12:00:00".into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    proptest! {
        /// The card's passage is always a mark of that reading, and always the
        /// longest of them in characters — ties going to the lowest id, which
        /// is what makes the answer the same on every call.
        ///
        /// The expected value is computed in Rust from what was inserted, never
        /// by a second query, so it cannot agree with the implementation by
        /// sharing its mistake.
        #[test]
        fn the_card_passage_is_the_longest_mark_of_its_reading(
            marks in proptest::collection::vec((0usize..6, any::<bool>()), 0..9),
        ) {
            rt().block_on(async {
                let s = Storage::connect("sqlite::memory:").await.unwrap();
                let book = s.upsert_book(
                    &Book { title: Some("C".into()), ..Default::default() },
                    None,
                ).await.unwrap();
                let reading = s
                    .record_reading(book, Some(1_767_225_600), None, "reading", "manual")
                    .await
                    .unwrap();

                for (seq, &(n, cjk)) in marks.iter().enumerate() {
                    let id = s.insert_highlight(book, &mark(n, cjk, seq)).await.unwrap()
                        .expect("every anchor here is distinct, so every insert is a row");
                    sqlx::query("UPDATE highlights SET reading_id = ? WHERE id = ?")
                        .bind(reading)
                        .bind(id)
                        .execute(s.pool())
                        .await
                        .unwrap();
                }

                let listed = s.highlights_for_reading(reading).await.unwrap();
                prop_assert_eq!(listed.len(), marks.len());

                let got = s.card_passage(reading).await.unwrap();

                // Absence is exactly absence of marks, and nothing else.
                prop_assert_eq!(got.is_none(), marks.is_empty());
                let Some(got) = got else { return Ok(()); };

                // It is one of the reading's own marks — a card and the full
                // list can never disagree about what was marked.
                prop_assert!(
                    listed.iter().any(|h| h.id == got.id),
                    "the card showed a passage the reading does not hold"
                );

                // And it is *the* maximum, by characters then by lowest id.
                let want = listed
                    .iter()
                    .min_by_key(|h| (std::cmp::Reverse(h.text.chars().count()), h.id))
                    .unwrap();
                prop_assert_eq!(got.id, want.id);
                prop_assert_eq!(&got.text, &want.text);

                // Asked again, the same answer. The order is total — `id` is a
                // primary key — so this holds however SQLite plans the query.
                let again = s.card_passage(reading).await.unwrap().unwrap();
                prop_assert_eq!(again.id, got.id);
                Ok(())
            })?;
        }
    }
}
