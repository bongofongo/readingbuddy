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
    pub note: Option<String>,
    pub ko_datetime: Option<String>,
}

impl Storage {
    /// Insert one highlight; returns Some(id) if newly inserted, None if it
    /// already existed (identity_hash conflict).
    pub async fn insert_highlight(&self, book_id: i64, h: &NewHighlight) -> Result<Option<i64>> {
        let row = sqlx::query(
            r#"INSERT INTO highlights
                (book_id, text, chapter, page, pos0, pos1, ko_datetime, color, note, source,
                 identity_hash, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(&h.source)
        .bind(h.identity_hash(book_id))
        .bind(now_unix())
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| r.get("id")))
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
            r#"SELECT id, book_id, text, chapter, page, note, ko_datetime
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
                note: r.get("note"),
                ko_datetime: r.get("ko_datetime"),
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
}
