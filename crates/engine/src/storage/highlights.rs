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
        let mut hasher = Sha256::new();
        hasher.update(book_id.to_string());
        hasher.update("|");
        hasher.update(self.ko_datetime.as_deref().unwrap_or(""));
        hasher.update("|");
        hasher.update(self.pos0.as_deref().unwrap_or(""));
        hasher.update("|");
        hasher.update(&self.text);
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
    /// Where the row came from (`koreader`). Provenance, not payload — a device
    /// refresh must leave it alone.
    pub source: String,
    /// When *we* first stored it, unix seconds. Distinct from `ko_datetime`,
    /// which is when the device captured it, and equally not something a
    /// refresh may move.
    pub created_at: i64,
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

    pub async fn list_highlights(&self, book_id: i64) -> Result<Vec<Highlight>> {
        let rows = sqlx::query(
            r#"SELECT id, book_id, text, chapter, page, ko_note, annotation, ko_datetime,
                      source, created_at
               FROM highlights WHERE book_id = ?
               ORDER BY page ASC, ko_datetime ASC"#,
        )
        .bind(book_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Highlight {
                id: r.get("id"),
                book_id: r.get("book_id"),
                text: r.get("text"),
                chapter: r.get("chapter"),
                page: r.get("page"),
                ko_note: r.get("ko_note"),
                annotation: r.get("annotation"),
                ko_datetime: r.get("ko_datetime"),
                source: r.get("source"),
                created_at: r.get("created_at"),
            })
            .collect())
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
            .upsert_book(&Book {
                title: Some("T".into()),
                ..Default::default()
            })
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
            .upsert_book(&Book {
                title: Some("T".into()),
                ..Default::default()
            })
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
            .upsert_book(&Book {
                title: Some("Another".into()),
                ..Default::default()
            })
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
}
