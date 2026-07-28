//! What another system said about a book: its own id for it, and the shelves
//! it filed it under.
//!
//! Both tables (migration `0009`) are **inert provenance**. Nothing reads
//! `book_tags` to decide anything, there is no UI for it, and there are no merge
//! semantics — `docs/decisions.md` defers collections outright, because three
//! systems minting them is a merge problem with no good default. Recording the
//! raw value now is what lets that design be made later against real shelves.
//!
//! `external_ids` is the working half: it is what makes re-importing the same
//! Goodreads CSV find the same books instead of creating a second copy of every
//! row whose ISBN we do not have. It is keyed on `(source, external_id)` rather
//! than living as a `goodreads_id` column on `books` because Calibre needs
//! exactly the same mapping — and the moment there are two such columns, there
//! is a query that checks one of them.

use sqlx::Row;

use super::{Storage, now_unix};
use crate::error::Result;

/// One shelf, as the origin wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookTag {
    pub tag: String,
    pub source: String,
    /// The origin's own string, before normalization. Kept because the
    /// normalization is ours and the shelf name is theirs.
    pub raw: Option<String>,
}

impl Storage {
    /// Record that `source` calls this book `external_id`.
    ///
    /// **Repoints on conflict.** An id belongs to one book on the far side, so
    /// the same `(source, external_id)` arriving against a different book means
    /// the mapping was wrong (or the books were merged) — keeping the stale one
    /// would make every later import write to the wrong book, silently.
    pub async fn link_external_id(
        &self,
        source: &str,
        external_id: &str,
        book_id: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO external_ids (source, external_id, book_id, created_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (source, external_id) DO UPDATE SET book_id = excluded.book_id",
        )
        .bind(source)
        .bind(external_id)
        .bind(book_id)
        .bind(now_unix())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The book `source` calls `external_id`, if we have been told.
    pub async fn book_for_external_id(
        &self,
        source: &str,
        external_id: &str,
    ) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT book_id FROM external_ids WHERE source = ? AND external_id = ?",
        )
        .bind(source)
        .bind(external_id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Add tags from one source. Idempotent — re-importing a shelf list writes
    /// no duplicate rows — and **additive**: a shelf the user removed on the far
    /// side is not deleted here, because nothing reads these rows and losing
    /// provenance is the one failure this table cannot recover from.
    ///
    /// Returns how many rows were genuinely new.
    pub async fn add_book_tags(
        &self,
        book_id: i64,
        source: &str,
        tags: &[(String, String)],
    ) -> Result<usize> {
        let mut added = 0;
        for (tag, raw) in tags {
            let done = sqlx::query(
                "INSERT INTO book_tags (book_id, tag, source, raw) VALUES (?, ?, ?, ?)
                 ON CONFLICT (book_id, tag, source) DO NOTHING",
            )
            .bind(book_id)
            .bind(tag)
            .bind(source)
            .bind(raw)
            .execute(self.pool())
            .await?;
            added += done.rows_affected() as usize;
        }
        Ok(added)
    }

    /// Every tag on a book, whoever said it, in a stable order.
    pub async fn book_tags(&self, book_id: i64) -> Result<Vec<BookTag>> {
        let rows = sqlx::query(
            "SELECT tag, source, raw FROM book_tags WHERE book_id = ?
             ORDER BY source ASC, tag ASC",
        )
        .bind(book_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| BookTag {
                tag: r.get("tag"),
                source: r.get("source"),
                raw: r.get("raw"),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;

    async fn seeded() -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(&Book {
                title: Some("Pachinko".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        (s, id)
    }

    #[tokio::test]
    async fn an_external_id_round_trips_and_repoints() {
        let (s, book) = seeded().await;
        assert_eq!(
            s.book_for_external_id("goodreads", "34051011")
                .await
                .unwrap(),
            None
        );

        s.link_external_id("goodreads", "34051011", book)
            .await
            .unwrap();
        assert_eq!(
            s.book_for_external_id("goodreads", "34051011")
                .await
                .unwrap(),
            Some(book)
        );

        let other = s
            .upsert_book(&Book {
                title: Some("Pachinko (paperback)".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        s.link_external_id("goodreads", "34051011", other)
            .await
            .unwrap();
        assert_eq!(
            s.book_for_external_id("goodreads", "34051011")
                .await
                .unwrap(),
            Some(other),
            "an id names one book on the far side; a stale mapping writes to the wrong one"
        );
        // And the sources are separate namespaces, which is the whole reason
        // this is not a `goodreads_id` column.
        assert_eq!(
            s.book_for_external_id("calibre", "34051011").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn tags_are_idempotent_and_keep_the_raw_shelf_name() {
        let (s, book) = seeded().await;
        let shelves = [
            ("to-read".to_string(), "to-read".to_string()),
            ("korean-lit".to_string(), "Korean Lit".to_string()),
        ];
        assert_eq!(
            s.add_book_tags(book, "goodreads", &shelves).await.unwrap(),
            2
        );
        assert_eq!(
            s.add_book_tags(book, "goodreads", &shelves).await.unwrap(),
            0,
            "re-importing the same export adds nothing"
        );

        let tags = s.book_tags(book).await.unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].tag, "korean-lit");
        assert_eq!(tags[0].raw.as_deref(), Some("Korean Lit"));
        assert_eq!(tags[0].source, "goodreads");
    }
}
