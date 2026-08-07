use sqlx::Row;

use super::{Storage, now_unix};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct FlashcardRow {
    pub id: i64,
    /// The book this card was captured from.
    ///
    /// On the table since `0001_init.sql` and selected by nothing until item
    /// 45. Without it a card knows only its book's *title*, which is not a
    /// handle — two editions of one book share it, and a frontend holding a
    /// list of cards had no way back to the shelf.
    pub book_id: i64,
    /// The passage the word was taken from, where there is one.
    ///
    /// `None` for a card minted without a highlight; `Some` for one the
    /// KOReader import auto-captured off a single-word highlight, which is
    /// most of them. This is what lets a card be shown *beside* its passage
    /// rather than beside its book's name.
    pub highlight_id: Option<i64>,
    pub word: String,
    pub context: Option<String>,
    pub book_title: String,
    pub exported: bool,
}

/// The columns both list queries read, written once.
///
/// Two hand-written column lists that agree today disagree the first time a
/// field is added to one of them — which is exactly what happened here: item 45
/// had to add `book_id` and `highlight_id` to two `SELECT`s and two row
/// mappers, in a module whose whole SQL surface is four statements.
const FLASHCARD_COLUMNS: &str =
    "f.id, f.book_id, f.highlight_id, f.word, f.context, f.exported, b.title AS book_title";

fn row_to_flashcard(r: &sqlx::sqlite::SqliteRow) -> FlashcardRow {
    FlashcardRow {
        id: r.get("id"),
        book_id: r.get("book_id"),
        highlight_id: r.get("highlight_id"),
        word: r.get("word"),
        context: r.get("context"),
        book_title: r.get("book_title"),
        exported: r.get::<i64, _>("exported") != 0,
    }
}

impl Storage {
    /// Returns true if newly inserted (UNIQUE(book_id, word) dedupes).
    ///
    /// **`DO NOTHING`, so a second attempt at the same word is not a rewrite.**
    /// The first card's `context` and `highlight_id` are what the reader
    /// actually captured; a `DO UPDATE` would let a later capture of the same
    /// word — a different chapter, a different passage — silently repoint the
    /// card at it, and the bool this returns would stop distinguishing
    /// *created* from *already there*.
    pub async fn insert_flashcard(
        &self,
        book_id: i64,
        highlight_id: Option<i64>,
        word: &str,
        context: Option<&str>,
    ) -> Result<bool> {
        let res = sqlx::query(
            r#"INSERT INTO flashcards (book_id, highlight_id, word, context, created_at)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(book_id, word) DO NOTHING"#,
        )
        .bind(book_id)
        .bind(highlight_id)
        .bind(word)
        .bind(context)
        .bind(now_unix())
        .execute(self.pool())
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn list_flashcards(&self, include_exported: bool) -> Result<Vec<FlashcardRow>> {
        let filter = if include_exported {
            ""
        } else {
            "WHERE f.exported = 0"
        };
        let sql = format!(
            r#"SELECT {FLASHCARD_COLUMNS}
               FROM flashcards f JOIN books b ON b.id = f.book_id
               {filter} ORDER BY f.created_at ASC"#
        );
        let rows = sqlx::query(&sql).fetch_all(self.pool()).await?;
        Ok(rows.iter().map(row_to_flashcard).collect())
    }

    /// All flashcards for one book (both pending and exported), for the
    /// per-book Cards tab.
    pub async fn list_flashcards_for_book(&self, book_id: i64) -> Result<Vec<FlashcardRow>> {
        let sql = format!(
            r#"SELECT {FLASHCARD_COLUMNS}
               FROM flashcards f JOIN books b ON b.id = f.book_id
               WHERE f.book_id = ? ORDER BY f.created_at ASC"#
        );
        let rows = sqlx::query(&sql)
            .bind(book_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.iter().map(row_to_flashcard).collect())
    }

    pub async fn mark_flashcards_exported(&self, ids: &[i64]) -> Result<()> {
        for id in ids {
            sqlx::query("UPDATE flashcards SET exported = 1 WHERE id = ?")
                .bind(id)
                .execute(self.pool())
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::book::Book;
    use crate::storage::{NewHighlight, Storage};

    async fn mem() -> Storage {
        Storage::connect("sqlite::memory:").await.expect("storage")
    }

    async fn a_book(s: &Storage, title: &str) -> i64 {
        s.upsert_book(
            &Book {
                title: Some(title.into()),
                ..Default::default()
            },
            None,
        )
        .await
        .expect("book")
    }

    fn hl(text: &str) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: None,
            page: Some(1),
            pos0: Some(format!("/body/p[1]/text().{}", text.len())),
            pos1: None,
            ko_datetime: Some("2026-01-05 21:14:08".into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    /// The whole point of item 45: a card can be shown beside the passage it
    /// came from, which needs both handles to survive the round trip.
    #[tokio::test]
    async fn a_card_carries_its_book_and_its_passage() {
        let s = mem().await;
        let book = a_book(&s, "Station Eleven").await;
        let h = s
            .insert_highlight(book, &hl("pachinko"))
            .await
            .expect("highlight")
            .expect("new");
        assert!(
            s.insert_flashcard(book, Some(h), "pachinko", Some("ch1"))
                .await
                .expect("insert")
        );

        for card in [
            s.list_flashcards(true).await.expect("list").remove(0),
            s.list_flashcards_for_book(book)
                .await
                .expect("for book")
                .remove(0),
        ] {
            assert_eq!(card.book_id, book);
            assert_eq!(card.highlight_id, Some(h));
            assert_eq!(card.word, "pachinko");
        }
    }

    /// `ON CONFLICT DO NOTHING` is load-bearing, not incidental: the second
    /// attempt must report *not new* **and** leave the first card exactly as it
    /// was. A `DO UPDATE` would satisfy neither.
    #[tokio::test]
    async fn a_second_card_for_the_same_word_neither_counts_nor_rewrites() {
        let s = mem().await;
        let book = a_book(&s, "Station Eleven").await;
        let h = s
            .insert_highlight(book, &hl("pachinko"))
            .await
            .expect("highlight")
            .expect("new");

        assert!(
            s.insert_flashcard(book, Some(h), "pachinko", Some("first"))
                .await
                .expect("insert")
        );
        assert!(
            !s.insert_flashcard(book, None, "pachinko", Some("second"))
                .await
                .expect("insert")
        );

        let cards = s.list_flashcards_for_book(book).await.expect("list");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].context.as_deref(), Some("first"));
        assert_eq!(cards[0].highlight_id, Some(h));
    }
    /// What `Engine::create_flashcard`'s re-read is actually buying, pinned at
    /// the layer below it: `flashcards.highlight_id` is a foreign key, so an
    /// unchecked write against a stale id does **not** quietly succeed — it
    /// comes back as a raw `FOREIGN KEY constraint failed`, which crosses the
    /// API seam as `internal`. `link_foreign_record` records the same finding
    /// about `external_ids`, and takes the same medicine.
    ///
    /// So the validation converts a wrong error into a typed one as well as
    /// closing the mismatched-book hole. If this ever stops being an error,
    /// the facade check has become the only thing standing between a card and
    /// an anchor pointing at nothing.
    #[tokio::test]
    async fn an_unchecked_anchor_is_a_raw_constraint_error() {
        let s = mem().await;
        let book = a_book(&s, "Station Eleven").await;
        assert!(
            s.insert_flashcard(book, Some(9_999), "ghost", None)
                .await
                .is_err(),
            "the FK is what makes the facade's re-read a correction, not a nicety"
        );
    }
}
