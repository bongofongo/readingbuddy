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

pub(super) const NOTE_COLUMNS: &str = "id, book_id, reading_id, highlight_id, page, location, \
     file_path, title, kind, created_at";

/// One outgoing edge, read from the linking note's side.
///
/// `to` is `None` for a **dangling** target: a `[[wikilink]]` naming a note that
/// has not been written yet. That is an ordinary zettelkasten forward reference,
/// not an error, which is why the raw text is carried beside the resolution
/// rather than the row being dropped.
#[derive(Debug, Clone)]
pub struct OutgoingLink {
    /// The wikilink target exactly as the body wrote it.
    pub target_title: String,
    /// The note it resolves to, when one exists.
    pub to: Option<NoteRecord>,
}

/// Prefix a canonical column list with a table alias, so a joined query can
/// reuse `NOTE_COLUMNS` / `HIGHLIGHT_COLUMNS` instead of spelling out a second
/// copy that can drift from it.
pub(super) fn qualified(columns: &str, alias: &str) -> String {
    columns
        .split(", ")
        .map(|c| format!("{alias}.{c}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn row_to_note(r: &sqlx::sqlite::SqliteRow) -> NoteRecord {
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

/// Rewrite a note's FTS row and stamp `last_modified`.
///
/// Shared by [`Storage::refresh_note_body`] and [`Storage::reindex_note`] so
/// the two cannot disagree about what re-indexing a body means.
async fn reindex(
    tx: &mut sqlx::SqliteConnection,
    note_id: i64,
    title: &str,
    body: &str,
) -> Result<()> {
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
    Ok(())
}

/// A note reduced to what the vault reconcile needs of it.
#[derive(Debug, Clone)]
pub struct NoteFile {
    pub id: i64,
    /// Vault-relative, exactly as `notes.file_path` stores it.
    pub file_path: String,
    pub title: String,
    /// Unix seconds: when this note's *index* was last written.
    pub last_modified: i64,
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
        reindex(&mut tx, note_id, title, body).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Bring a note's **whole** index in line with a body — the FTS row, the
    /// wikilink edges and `last_modified` — in one transaction.
    ///
    /// One transaction rather than `refresh_note_body` followed by
    /// `set_note_links`, and that is what makes the vault watcher cancel-safe:
    /// [`crate::VaultWatcher::next`] is dropped mid-await by every `select!`
    /// that races it, and a drop between two transactions would leave a note
    /// whose searchable body is the new file and whose graph edges are the old
    /// one — with nothing on either side looking wrong. Dropped inside *this*
    /// one, the transaction rolls back and the watcher retries.
    pub async fn reindex_note(
        &self,
        note_id: i64,
        title: &str,
        body: &str,
        links: &[String],
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        reindex(&mut tx, note_id, title, body).await?;
        // Replaced, not merged — see `set_note_links`.
        sqlx::query("DELETE FROM note_links WHERE from_note = ?")
            .bind(note_id)
            .execute(&mut *tx)
            .await?;
        write_links(&mut tx, note_id, title, links).await?;
        tx.commit().await?;
        Ok(())
    }

    /// The body this note is currently searchable *by*.
    ///
    /// `notes_fts` is an ordinary fts5 table used as a body cache, so its
    /// content is readable by rowid. That is what lets a refresh ask "has the
    /// file actually changed?" before writing anything — which is the whole of
    /// the watcher's answer to seeing its own writes.
    pub async fn indexed_body(&self, note_id: i64) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT body FROM notes_fts WHERE rowid = ?")
                .bind(note_id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// The note stored at this **vault-relative** path, if any.
    ///
    /// A markdown file under the vault that no row claims is not an error and
    /// not a note: readingbuddy does not adopt files it did not write.
    pub async fn note_by_path(&self, file_path: &str) -> Result<Option<NoteRecord>> {
        let sql = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE file_path = ?");
        Ok(sqlx::query(&sql)
            .bind(file_path)
            .fetch_optional(self.pool())
            .await?
            .as_ref()
            .map(row_to_note))
    }

    /// Every note as the reconcile sweep needs it: where its file is, what to
    /// index it under, and when we last wrote its index.
    ///
    /// Deliberately **not** a field on [`NoteRecord`]. `last_modified` is
    /// bookkeeping about the index rather than a fact about the note, and
    /// putting it on the record would put it on `NoteDto` and therefore on
    /// every frontend's screen vocabulary.
    pub async fn note_files(&self) -> Result<Vec<NoteFile>> {
        let rows = sqlx::query("SELECT id, file_path, title, last_modified FROM notes")
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .iter()
            .map(|r| NoteFile {
                id: r.get("id"),
                file_path: r.get("file_path"),
                title: r.get("title"),
                last_modified: r.get("last_modified"),
            })
            .collect())
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

    /// Notes, newest first — for one book or for the whole vault.
    ///
    /// **`limit` selects along `created_at`**, the same contract
    /// [`BookSort`](super::BookSort) states for books: `list_notes(None,
    /// Some(12))` is the twelve most recent notes, not twelve arbitrary ones.
    ///
    /// `None` is every note, and it stays reachable **deliberately**. Item 18
    /// found this method with no limit at all — a full table scan into a `Vec`
    /// for a screen showing twelve rows — and the obvious fix, a default cap, is
    /// wrong for the callers that are not screens: `resolve_note` scans every
    /// title in the vault to answer `rb links "Reflection: Pachinko"`, and a cap
    /// there would silently stop resolving the notes past it. A truncated
    /// correctness pass is worse than a slow one. So the cap belongs to the
    /// caller that has a viewport, and `None` belongs to the caller that has a
    /// whole graph to walk.
    pub async fn list_notes(
        &self,
        book_id: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<NoteRecord>> {
        // `created_at` is a second-resolution timestamp and two notes written in
        // one second are ordinary, so `id` closes the order the way every book
        // sort now does — without it a limit could cut a tie arbitrarily.
        let order = "ORDER BY created_at DESC, id DESC";
        // SQLite reads a negative limit as no limit, which is what makes the two
        // arms one statement rather than four.
        let limit = limit.unwrap_or(-1);
        let rows = match book_id {
            Some(id) => {
                let sql =
                    format!("SELECT {NOTE_COLUMNS} FROM notes WHERE book_id = ? {order} LIMIT ?");
                sqlx::query(&sql)
                    .bind(id)
                    .bind(limit)
                    .fetch_all(self.pool())
                    .await?
            }
            None => {
                let sql = format!("SELECT {NOTE_COLUMNS} FROM notes {order} LIMIT ?");
                sqlx::query(&sql).bind(limit).fetch_all(self.pool()).await?
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
        let columns = qualified(HIGHLIGHT_COLUMNS, "h");
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
    ///
    /// The cheap half of [`Storage::outgoing_links`], kept because
    /// `open_anchored` only ever wants the titles a body wrote and has no use
    /// for the target rows.
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

    /// Outgoing links of a note, with the target note itself where the edge
    /// resolves — the half of the graph a body can be read off.
    ///
    /// Ordered by `rowid`, which is insertion order, which is the order the
    /// links appeared in the body: `set_note_links` deletes and rewrites the
    /// whole set, so it never drifts from what the user wrote.
    pub async fn outgoing_links(&self, note_id: i64) -> Result<Vec<OutgoingLink>> {
        let columns = qualified(NOTE_COLUMNS, "n");
        let sql = format!(
            "SELECT l.target_title AS target_title, {columns}
             FROM note_links l LEFT JOIN notes n ON n.id = l.to_note
             WHERE l.from_note = ? ORDER BY l.rowid"
        );
        let rows = sqlx::query(&sql)
            .bind(note_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .iter()
            .map(|r| OutgoingLink {
                target_title: r.get("target_title"),
                // The join missed, so every other `n.` column is NULL and
                // `row_to_note` would fail decoding them. Reading `id` as an
                // `Option` first is what keeps a dangling target a value rather
                // than an error.
                to: r.get::<Option<i64>, _>("id").map(|_| row_to_note(r)),
            })
            .collect())
    }

    /// The notes that link **to** this one — "what links here".
    ///
    /// A plain `WHERE to_note = ?`, with no dangling-by-title union, and that is
    /// a decision rather than an omission. Two reasons:
    ///
    /// * `write_links` back-resolves every dangling edge naming a title the
    ///   moment a note with that title is written, so an edge left `NULL` is one
    ///   whose target genuinely does not exist. Pinned by
    ///   `a_forward_reference_resolves_when_its_target_is_written`.
    /// * The two directions have to be the same edge set read from opposite
    ///   ends. [`Storage::outgoing_links`] reports an unresolved edge as
    ///   dangling text; a union here would make the same edge read as a real
    ///   backlink from the other side, and the pane would contradict itself.
    ///
    /// The second reason is the one that survives the exception. `notes.title`
    /// is not unique, so an edge that resolved to one of two same-titled notes
    /// dangles again if *that* one is deleted, and nothing re-resolves it toward
    /// the survivor. Both sides then agree it dangles, which is worth more than
    /// one side guessing —
    /// `a_title_shared_by_two_notes_is_where_back_resolution_stops`.
    ///
    /// Newest first, as `list_notes` orders — with `id` breaking the tie, since
    /// `created_at` is only second-resolution.
    pub async fn backlinks(&self, note_id: i64) -> Result<Vec<NoteRecord>> {
        let columns = qualified(NOTE_COLUMNS, "n");
        let sql = format!(
            "SELECT {columns} FROM note_links l JOIN notes n ON n.id = l.from_note
             WHERE l.to_note = ? ORDER BY n.created_at DESC, n.id DESC"
        );
        let rows = sqlx::query(&sql)
            .bind(note_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.iter().map(row_to_note).collect())
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
        let search = |s: Storage, q: &'static str| async move {
            s.search_marks(q, Some(super::super::SearchSource::Note), 10)
                .await
                .unwrap()
        };
        let hits = search(s.clone(), "grief").await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].as_note().unwrap().id, n2);
        assert!(hits[0].snippet().contains(">>grief<<"));

        // Refresh replaces body in the index.
        s.refresh_note_body(n2, "Han", "Now about resilience.")
            .await
            .unwrap();
        assert!(search(s.clone(), "grief").await.is_empty());
        assert_eq!(search(s.clone(), "resilience").await.len(), 1);
    }

    /// `limit` selects along `created_at`, and its absence is every note.
    ///
    /// Item 18's whole change to this method, and both halves matter: a screen
    /// showing twelve rows no longer reads the table, and `resolve_note` — which
    /// walks every title in the vault — still can.
    #[tokio::test]
    async fn a_note_limit_selects_the_newest_and_its_absence_is_everything() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        for (i, title) in ["one", "two", "three", "four"].into_iter().enumerate() {
            s.insert_note(
                NewNoteMeta {
                    book_id: None,
                    reading_id: None,
                    highlight_id: None,
                    page: None,
                    location: None,
                    file_path: &format!("unsorted/{i}.md"),
                    title,
                    kind: "note",
                },
                "body",
                &[],
            )
            .await
            .unwrap();
        }

        let all = s.list_notes(None, None).await.unwrap();
        assert_eq!(all.len(), 4, "no limit is every note");
        let page = s.list_notes(None, Some(2)).await.unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(
            page.iter().map(|n| n.id).collect::<Vec<_>>(),
            all.iter().take(2).map(|n| n.id).collect::<Vec<_>>(),
            "a limited list is the head of the unlimited one — the newest, not \
             two arbitrary notes"
        );
        assert!(s.list_notes(None, Some(0)).await.unwrap().is_empty());
    }
}
