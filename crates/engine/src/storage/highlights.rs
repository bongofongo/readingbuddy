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
    pub color: Option<String>,
    pub note: Option<String>,
    pub source: String,
}

impl NewHighlight {
    /// Stable identity for idempotent imports.
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
            color: None,
            note: None,
            source: "koreader".into(),
        }
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
