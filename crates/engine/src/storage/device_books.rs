//! The device-file → book mapping (`device_books`, migration `0003`).
//!
//! Keyed on the sidecar's root `partial_md5_checksum`. See
//! `docs/koreader-format.md` §2.1 for why it is that value and not `stats.md5`.

use super::books::{BOOK_COLUMNS, BOOK_FROM, row_to_book};
use super::{Storage, now_unix};
use crate::book::Book;
use crate::error::Result;

/// How a device file came to be linked to a book.
///
/// A `Manual` link is the user's decision and must never be silently
/// overwritten by a later automatic match; `Auto` is ours and may be revisited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkedBy {
    Auto,
    Manual,
}

impl LinkedBy {
    fn as_str(self) -> &'static str {
        match self {
            LinkedBy::Auto => "auto",
            LinkedBy::Manual => "manual",
        }
    }
}

impl Storage {
    /// The book a device file is already linked to, if any.
    ///
    /// A subquery rather than a second join so `BOOK_COLUMNS`/`BOOK_FROM` are
    /// reused verbatim — spelling the select out here is a second copy of the
    /// list waiting to drift from the first.
    pub async fn find_book_by_partial_md5(&self, partial_md5: &str) -> Result<Option<Book>> {
        let sql = format!(
            "SELECT {BOOK_COLUMNS} {BOOK_FROM}
             WHERE books.id = (SELECT book_id FROM device_books WHERE partial_md5 = ?)"
        );
        let row = sqlx::query(&sql)
            .bind(partial_md5)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_book).transpose()
    }

    /// Record (or refresh) the mapping.
    ///
    /// Re-linking only bumps `last_seen`: `book_id` and `linked_by` are left
    /// alone so re-importing a library cannot quietly relabel a link the user
    /// made by hand as one we guessed. Repointing a mapping at a different book
    /// is a deliberate act and belongs to item 3's merge, not to a scan.
    pub async fn link_device_book(
        &self,
        partial_md5: &str,
        book_id: i64,
        linked_by: LinkedBy,
    ) -> Result<()> {
        let now = now_unix();
        sqlx::query(
            r#"INSERT INTO device_books (partial_md5, book_id, linked_by, first_seen, last_seen)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(partial_md5) DO UPDATE SET last_seen = excluded.last_seen"#,
        )
        .bind(partial_md5)
        .bind(book_id)
        .bind(linked_by.as_str())
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Point a device file at a book, overwriting whatever it pointed at
    /// before.
    ///
    /// The sharp edge that [`Storage::link_device_book`] deliberately does not
    /// have. A scan must never repoint a mapping — it would relabel the user's
    /// own decision as a guess — but an explicit link *is* the decision, and it
    /// has to be able to correct a bad automatic one. Keep the two methods
    /// distinct: collapsing them would make every library scan a repointer.
    pub async fn set_device_link(
        &self,
        partial_md5: &str,
        book_id: i64,
        linked_by: LinkedBy,
    ) -> Result<()> {
        let now = now_unix();
        sqlx::query(
            r#"INSERT INTO device_books (partial_md5, book_id, linked_by, first_seen, last_seen)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(partial_md5) DO UPDATE SET
                   book_id   = excluded.book_id,
                   linked_by = excluded.linked_by,
                   last_seen = excluded.last_seen"#,
        )
        .bind(partial_md5)
        .bind(book_id)
        .bind(linked_by.as_str())
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Every device file linked to a book. Used by the merge to carry mappings
    /// across, and by callers that want to show what a book is linked to.
    pub async fn device_links_for_book(&self, book_id: i64) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT partial_md5 FROM device_books WHERE book_id = ? ORDER BY partial_md5",
        )
        .bind(book_id)
        .fetch_all(self.pool())
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;

    async fn seed(s: &Storage, title: &str) -> i64 {
        s.upsert_book(&Book {
            title: Some(title.into()),
            ..Default::default()
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_link_is_found_again_and_relinking_is_idempotent() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = seed(&s, "Pachinko").await;

        assert!(s.find_book_by_partial_md5("abc").await.unwrap().is_none());
        s.link_device_book("abc", id, LinkedBy::Auto).await.unwrap();

        let found = s.find_book_by_partial_md5("abc").await.unwrap().unwrap();
        assert_eq!(found.id, Some(id));
        assert_eq!(found.title.as_deref(), Some("Pachinko"));

        // Re-linking must not duplicate the row.
        s.link_device_book("abc", id, LinkedBy::Auto).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM device_books")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    /// A manual link is the user's decision. A later library scan re-links the
    /// same file automatically, and that must not relabel it.
    #[tokio::test]
    async fn an_automatic_relink_does_not_downgrade_a_manual_one() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = seed(&s, "Pachinko").await;
        s.link_device_book("abc", id, LinkedBy::Manual)
            .await
            .unwrap();
        s.link_device_book("abc", id, LinkedBy::Auto).await.unwrap();

        let by: String =
            sqlx::query_scalar("SELECT linked_by FROM device_books WHERE partial_md5 = ?")
                .bind("abc")
                .fetch_one(s.pool())
                .await
                .unwrap();
        assert_eq!(by, "manual");
    }

    /// The distinction between the two link methods, stated as a test: a scan
    /// may not repoint, an explicit link must be able to.
    #[tokio::test]
    async fn only_an_explicit_link_repoints_a_mapping() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let first = seed(&s, "Pachinko").await;
        let second = seed(&s, "Pachinko (the other copy)").await;

        s.link_device_book("abc", first, LinkedBy::Auto)
            .await
            .unwrap();
        s.link_device_book("abc", second, LinkedBy::Auto)
            .await
            .unwrap();
        assert_eq!(
            s.find_book_by_partial_md5("abc").await.unwrap().unwrap().id,
            Some(first),
            "a scan must not repoint"
        );

        s.set_device_link("abc", second, LinkedBy::Manual)
            .await
            .unwrap();
        assert_eq!(
            s.find_book_by_partial_md5("abc").await.unwrap().unwrap().id,
            Some(second),
            "an explicit link must be able to correct a bad guess"
        );
        assert_eq!(s.device_links_for_book(second).await.unwrap(), ["abc"]);
        assert!(s.device_links_for_book(first).await.unwrap().is_empty());
    }

    /// The mapping is meaningless without its book, and a dangling row would
    /// make `find_book_by_partial_md5` return None while the link still looked
    /// present to anything querying the table directly.
    #[tokio::test]
    async fn deleting_the_book_cascades_the_mapping_away() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = seed(&s, "Pachinko").await;
        s.link_device_book("abc", id, LinkedBy::Auto).await.unwrap();

        s.delete_book(id).await.unwrap();

        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM device_books")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(n, 0, "foreign keys must be ON for this to hold");
    }
}
