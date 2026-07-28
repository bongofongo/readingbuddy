use sqlx::Row;
use time::OffsetDateTime;

use super::highlights::{HIGHLIGHT_COLUMNS, row_to_highlight};
use super::{Highlight, Storage, now_unix};
use crate::error::Result;

/// Metadata for a new note row (body is passed separately — it lives on
/// disk and only enters the DB as the FTS cache).
#[derive(Debug, Clone, Copy)]
pub struct NewNoteMeta<'a> {
    pub book_id: Option<i64>,
    /// The reading this note belongs to. A reflection or a review always has
    /// one; an ordinary note may float free.
    pub reading_id: Option<i64>,
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
    pub reading_id: Option<i64>,
    pub highlight_id: Option<i64>,
    pub page: Option<i64>,
    pub location: Option<String>,
    pub file_path: String,
    pub title: String,
    pub kind: String,
    pub created_at: Option<OffsetDateTime>,
}

const NOTE_COLUMNS: &str = "id, book_id, reading_id, highlight_id, page, location, \
     file_path, title, kind, created_at";

#[derive(Debug, Clone)]
pub struct NoteSearchHit {
    pub note: NoteRecord,
    pub snippet: String,
}

fn row_to_note(r: &sqlx::sqlite::SqliteRow) -> NoteRecord {
    NoteRecord {
        id: r.get("id"),
        book_id: r.get("book_id"),
        reading_id: r.get("reading_id"),
        highlight_id: r.get("highlight_id"),
        page: r.get("page"),
        location: r.get("location"),
        file_path: r.get("file_path"),
        title: r.get("title"),
        kind: r.get("kind"),
        created_at: OffsetDateTime::from_unix_timestamp(r.get::<i64, _>("created_at")).ok(),
    }
}

/// Write a note's outgoing edges: each target resolved against existing note
/// titles, kept as text when dangling (zettelkasten forward references), then
/// any older dangling link pointing at *this* note's title back-resolved.
///
/// Shared by the insert and the re-index so the two cannot disagree about what
/// a link means.
async fn write_links(
    tx: &mut sqlx::SqliteConnection,
    note_id: i64,
    title: &str,
    links: &[String],
) -> Result<()> {
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
    Ok(())
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
            reading_id,
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
            r#"INSERT INTO notes (book_id, reading_id, highlight_id, page, location, file_path, title, kind, created_at, last_modified)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"#,
        )
        .bind(book_id)
        .bind(reading_id)
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

        write_links(&mut tx, note_id, title, links).await?;

        tx.commit().await?;
        Ok(note_id)
    }

    /// Re-index a note's outgoing wikilinks after its body was rewritten.
    ///
    /// Without this, a note's edges are whatever its *first* body said for ever
    /// — and a reflection is opened empty and written afterwards, so the hub of
    /// the graph would be the one note with no edges at all.
    pub async fn set_note_links(&self, note_id: i64, title: &str, links: &[String]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        // Replaced, not merged: a link the user deleted from the body has to
        // leave the graph too.
        sqlx::query("DELETE FROM note_links WHERE from_note = ?")
            .bind(note_id)
            .execute(&mut *tx)
            .await?;
        write_links(&mut tx, note_id, title, links).await?;
        tx.commit().await?;
        Ok(())
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
                let sql = format!(
                    "SELECT {NOTE_COLUMNS} FROM notes WHERE book_id = ? ORDER BY created_at DESC"
                );
                sqlx::query(&sql).bind(id).fetch_all(self.pool()).await?
            }
            None => {
                let sql = format!("SELECT {NOTE_COLUMNS} FROM notes ORDER BY created_at DESC");
                sqlx::query(&sql).fetch_all(self.pool()).await?
            }
        };
        Ok(rows.iter().map(row_to_note).collect())
    }

    /// The reflection (or review) of one reading, if it has been opened.
    ///
    /// This is what makes `open_reflection` accrete rather than pile up: the
    /// second call finds the first call's note. The partial unique indexes
    /// `idx_one_reflection` / `idx_one_review` are what make "the" honest.
    pub async fn note_for_reading(
        &self,
        reading_id: i64,
        kind: &str,
    ) -> Result<Option<NoteRecord>> {
        let sql = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE reading_id = ? AND kind = ?");
        let row = sqlx::query(&sql)
            .bind(reading_id)
            .bind(kind)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.as_ref().map(row_to_note))
    }

    pub async fn get_note(&self, note_id: i64) -> Result<Option<NoteRecord>> {
        let sql = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(note_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.as_ref().map(row_to_note))
    }

    pub async fn search_notes(&self, query: &str, limit: i64) -> Result<Vec<NoteSearchHit>> {
        let rows = sqlx::query(
            r#"SELECT n.id, n.book_id, n.reading_id, n.highlight_id, n.page, n.location,
                      n.file_path, n.title, n.kind, n.created_at,
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

    /// Cite a highlight from a note.
    ///
    /// **By reference, never by copying the text in.** A citation that embedded
    /// the words would go stale the moment a device refresh rewrote them, and
    /// "which highlights did I actually use?" would stop being answerable.
    /// Idempotent: citing twice is the same citation.
    pub async fn add_citation(&self, note_id: i64, highlight_id: i64) -> Result<bool> {
        let done = sqlx::query(
            "INSERT INTO citations (note_id, highlight_id, created_at) VALUES (?, ?, ?)
             ON CONFLICT (note_id, highlight_id) DO NOTHING",
        )
        .bind(note_id)
        .bind(highlight_id)
        .bind(now_unix())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    pub async fn remove_citation(&self, note_id: i64, highlight_id: i64) -> Result<bool> {
        let done = sqlx::query("DELETE FROM citations WHERE note_id = ? AND highlight_id = ?")
            .bind(note_id)
            .bind(highlight_id)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The highlights a note cites, in the order a book reads.
    pub async fn citations_for(&self, note_id: i64) -> Result<Vec<Highlight>> {
        let columns = HIGHLIGHT_COLUMNS
            .split(", ")
            .map(|c| format!("h.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {columns} FROM citations c JOIN highlights h ON h.id = c.highlight_id
             WHERE c.note_id = ? ORDER BY h.page ASC, h.ko_datetime ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(note_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.iter().map(row_to_highlight).collect())
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
                    reading_id: None,
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
                    reading_id: None,
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
