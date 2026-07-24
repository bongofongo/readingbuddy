use sqlx::Row;
use time::OffsetDateTime;

use super::{Storage, now_unix};
use crate::error::Result;

/// Metadata for a new note row (body is passed separately — it lives on
/// disk and only enters the DB as the FTS cache).
#[derive(Debug, Clone, Copy)]
pub struct NewNoteMeta<'a> {
    pub book_id: Option<i64>,
    pub highlight_id: Option<i64>,
    pub page: Option<i64>,
    pub location: Option<&'a str>,
    pub file_path: &'a str,
    pub title: &'a str,
    pub kind: &'a str,
}

#[derive(Debug, Clone)]
pub struct NoteRecord {
    pub id: i64,
    pub book_id: Option<i64>,
    pub highlight_id: Option<i64>,
    pub page: Option<i64>,
    pub location: Option<String>,
    pub file_path: String,
    pub title: String,
    pub kind: String,
    pub created_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct NoteSearchHit {
    pub note: NoteRecord,
    pub snippet: String,
}

fn row_to_note(r: &sqlx::sqlite::SqliteRow) -> NoteRecord {
    NoteRecord {
        id: r.get("id"),
        book_id: r.get("book_id"),
        highlight_id: r.get("highlight_id"),
        page: r.get("page"),
        location: r.get("location"),
        file_path: r.get("file_path"),
        title: r.get("title"),
        kind: r.get("kind"),
        created_at: OffsetDateTime::from_unix_timestamp(r.get::<i64, _>("created_at")).ok(),
    }
}

impl Storage {
    /// Insert note metadata + FTS row + wikilink edges in one transaction.
    /// `links` are raw [[wikilink]] target titles; each is resolved against
    /// existing note titles, kept as text when dangling (zettelkasten
    /// forward references). Also back-resolves older dangling links that
    /// pointed at this note's title.
    pub async fn insert_note(
        &self,
        meta: NewNoteMeta<'_>,
        body: &str,
        links: &[String],
    ) -> Result<i64> {
        let NewNoteMeta {
            book_id,
            highlight_id,
            page,
            location,
            file_path,
            title,
            kind,
        } = meta;
        let mut tx = self.pool().begin().await?;
        let now = now_unix();
        let note_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO notes (book_id, highlight_id, page, location, file_path, title, kind, created_at, last_modified)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"#,
        )
        .bind(book_id)
        .bind(highlight_id)
        .bind(page)
        .bind(location)
        .bind(file_path)
        .bind(title)
        .bind(kind)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query("INSERT INTO notes_fts (rowid, title, body) VALUES (?, ?, ?)")
            .bind(note_id)
            .bind(title)
            .bind(body)
            .execute(&mut *tx)
            .await?;

        for target in links {
            let to_note: Option<i64> =
                sqlx::query_scalar("SELECT id FROM notes WHERE title = ? COLLATE NOCASE LIMIT 1")
                    .bind(target)
                    .fetch_optional(&mut *tx)
                    .await?;
            sqlx::query(
                r#"INSERT INTO note_links (from_note, to_note, target_title) VALUES (?, ?, ?)
                   ON CONFLICT(from_note, target_title) DO UPDATE SET to_note = excluded.to_note"#,
            )
            .bind(note_id)
            .bind(to_note)
            .bind(target)
            .execute(&mut *tx)
            .await?;
        }

        // Older notes may already link to this title.
        sqlx::query(
            "UPDATE note_links SET to_note = ? WHERE to_note IS NULL AND target_title = ? COLLATE NOCASE",
        )
        .bind(note_id)
        .bind(title)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(note_id)
    }

    /// Refresh a note body in the FTS index (delete + insert).
    pub async fn refresh_note_body(&self, note_id: i64, title: &str, body: &str) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM notes_fts WHERE rowid = ?")
            .bind(note_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO notes_fts (rowid, title, body) VALUES (?, ?, ?)")
            .bind(note_id)
            .bind(title)
            .bind(body)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE notes SET last_modified = ? WHERE id = ?")
            .bind(now_unix())
            .bind(note_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Remove a note: its row (cascading `note_links` from it, nulling links
    /// pointing at it) and its FTS entry. The FTS table is virtual, so a
    /// foreign key can't cascade into it — it must be cleared explicitly.
    pub async fn delete_note(&self, note_id: i64) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM notes_fts WHERE rowid = ?")
            .bind(note_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM notes WHERE id = ?")
            .bind(note_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_notes(&self, book_id: Option<i64>) -> Result<Vec<NoteRecord>> {
        let rows = match book_id {
            Some(id) => {
                sqlx::query(
                    r#"SELECT id, book_id, highlight_id, page, location, file_path, title, kind, created_at
                       FROM notes WHERE book_id = ? ORDER BY created_at DESC"#,
                )
                .bind(id)
                .fetch_all(self.pool())
                .await?
            }
            None => {
                sqlx::query(
                    r#"SELECT id, book_id, highlight_id, page, location, file_path, title, kind, created_at
                       FROM notes ORDER BY created_at DESC"#,
                )
                .fetch_all(self.pool())
                .await?
            }
        };
        Ok(rows.iter().map(row_to_note).collect())
    }

    pub async fn search_notes(&self, query: &str, limit: i64) -> Result<Vec<NoteSearchHit>> {
        let rows = sqlx::query(
            r#"SELECT n.id, n.book_id, n.highlight_id, n.page, n.location, n.file_path, n.title, n.kind, n.created_at,
                      snippet(notes_fts, 1, '>>', '<<', '…', 12) AS snip
               FROM notes_fts
               JOIN notes n ON n.id = notes_fts.rowid
               WHERE notes_fts MATCH ?
               ORDER BY rank LIMIT ?"#,
        )
        .bind(query)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| NoteSearchHit {
                note: row_to_note(r),
                snippet: r.get("snip"),
            })
            .collect())
    }

    /// Outgoing links of a note: (target_title, resolved note id if any).
    pub async fn note_links(&self, note_id: i64) -> Result<Vec<(String, Option<i64>)>> {
        let rows = sqlx::query("SELECT target_title, to_note FROM note_links WHERE from_note = ?")
            .bind(note_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("target_title"), r.get("to_note")))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fts_roundtrip_and_link_resolution() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();

        // First note links to a not-yet-existing title (forward reference).
        let n1 = s
            .insert_note(
                NewNoteMeta {
                    book_id: None,
                    highlight_id: None,
                    page: None,
                    location: None,
                    file_path: "unsorted/a.md",
                    title: "First thought",
                    kind: "note",
                },
                "Reminds me of [[Han]] as a concept.",
                &["Han".to_string()],
            )
            .await
            .unwrap();
        let links = s.note_links(n1).await.unwrap();
        assert_eq!(links, vec![("Han".to_string(), None)]);

        // Creating "Han" back-resolves the dangling link.
        let n2 = s
            .insert_note(
                NewNoteMeta {
                    book_id: None,
                    highlight_id: None,
                    page: None,
                    location: None,
                    file_path: "unsorted/han.md",
                    title: "Han",
                    kind: "note",
                },
                "Korean concept of grief.",
                &[],
            )
            .await
            .unwrap();
        let links = s.note_links(n1).await.unwrap();
        assert_eq!(links, vec![("Han".to_string(), Some(n2))]);

        // FTS hits body content.
        let hits = s.search_notes("grief", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.id, n2);
        assert!(hits[0].snippet.contains(">>grief<<"));

        // Refresh replaces body in the index.
        s.refresh_note_body(n2, "Han", "Now about resilience.")
            .await
            .unwrap();
        assert!(s.search_notes("grief", 10).await.unwrap().is_empty());
        assert_eq!(s.search_notes("resilience", 10).await.unwrap().len(), 1);
    }
}
