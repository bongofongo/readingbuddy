use sqlx::Row;

use super::{Storage, now_unix};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct FlashcardRow {
    pub id: i64,
    pub word: String,
    pub context: Option<String>,
    pub book_title: String,
    pub exported: bool,
}

impl Storage {
    /// Returns true if newly inserted (UNIQUE(book_id, word) dedupes).
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
        let filter = if include_exported { "" } else { "WHERE f.exported = 0" };
        let sql = format!(
            r#"SELECT f.id, f.word, f.context, f.exported, b.title AS book_title
               FROM flashcards f JOIN books b ON b.id = f.book_id
               {filter} ORDER BY f.created_at ASC"#
        );
        let rows = sqlx::query(&sql).fetch_all(self.pool()).await?;
        Ok(rows
            .into_iter()
            .map(|r| FlashcardRow {
                id: r.get("id"),
                word: r.get("word"),
                context: r.get("context"),
                book_title: r.get("book_title"),
                exported: r.get::<i64, _>("exported") != 0,
            })
            .collect())
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
