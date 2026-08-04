//! Where each field of a book came from, and when (migration `0012`).
//!
//! **Why this is not a section of `provenance.rs`.** That module's defining
//! property, stated in its own first paragraph, is that it is *inert*: nothing
//! reads `book_tags` to decide anything, and `external_ids` is read only to find
//! a book again. This table is the opposite — the merge SQL in
//! [`books`](super::books) reads it on **every write to `books`**, and a `user`
//! row changes what that write does. Filing a table that gates writes under a
//! module documented as deciding nothing is how the next reader learns the wrong
//! rule. They are also different grains: those two record provenance about
//! *rows*, this records it about *fields*.
//!
//! ## The grain, and the thing this schema cannot say
//!
//! One row per `(book_id, field)`: **the winner, not the history**. A book can
//! carry a page count from OpenLibrary and a different one from calibre, and
//! only the last writer's claim survives here.
//!
//! That is the right trade for what this table is *for* — protecting a user's
//! correction from the next provider merge, and explaining the value the user is
//! actually looking at. Keeping every source's claim is a different feature
//! (*disagreement history*), it needs a different table
//! (`(book_id, field, source)` **plus the value each one claimed** — without a
//! value column, per-source rows record who was asked rather than what they
//! said, which is nothing), and it only starts paying once something re-asks a
//! provider it has already asked. That is item 30. The growth path is a second
//! table beside this one, not a wider primary key here.
//!
//! ## No `value_hash`
//!
//! `docs/spec-engine-29-32.md` names it as an open question for the next item.
//! Nothing in this item reads it: the user-wins rule keys on `source`, and
//! "the provider changed its mind" is a comparison item 30 makes between a
//! *fetched* record and a stored one, where both values are in hand and a hash
//! of one of them adds nothing. A column with no reader is a column that is
//! wrong by the time it gets one.

use sqlx::{Row, SqliteConnection};

use super::{Storage, now_unix};
use crate::error::Result;

/// Who supplied a field.
///
/// The vocabulary lives here and **not** in a `CHECK` constraint, following
/// `readings.source` in migration `0005`: a `CHECK` makes every new source a
/// schema migration, and this enum is the constraint that actually holds,
/// because it is the only thing that can write the column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    OpenLibrary,
    GoogleBooks,
    Calibre,
    Epub,
    KOReader,
    Goodreads,
    /// The user typed it. **Outranks everything**, which is the whole reason
    /// this table exists: `MERGE_RULES`'s `COALESCE` no-clobber is right for a
    /// partial provider record and cannot tell a hand correction from a
    /// provider's guess, so without this the first automatic enrichment
    /// silently overwrites the correction.
    User,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::OpenLibrary => "openlibrary",
            Source::GoogleBooks => "googlebooks",
            Source::Calibre => "calibre",
            Source::Epub => "epub",
            Source::KOReader => "koreader",
            Source::Goodreads => "goodreads",
            Source::User => USER,
        }
    }

    /// Every source, for tests that must cover the vocabulary rather than the
    /// three of it somebody remembered.
    pub const ALL: [Source; 7] = [
        Source::OpenLibrary,
        Source::GoogleBooks,
        Source::Calibre,
        Source::Epub,
        Source::KOReader,
        Source::Goodreads,
        Source::User,
    ];
}

/// The one spelling of the privileged source, so the value guard and the stamp
/// guard cannot drift into disagreeing about which rows are protected.
pub(super) const USER: &str = "user";

/// A SQL predicate: does the user own `field` on the book named by `book_expr`?
///
/// Rendered into the merge clause once per column by
/// [`merge_set`](super::books). `col` is a `MERGE_RULES` column name and
/// `book_expr` is a SQL expression naming the row — both are compile-time
/// constants from this crate, never anything a user typed, which is what makes
/// inlining them safe here.
pub(super) fn user_owns(book_expr: &str, col: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM field_provenance p \
         WHERE p.book_id = {book_expr} AND p.field = '{col}' AND p.source = '{USER}')"
    )
}

/// [`user_owns`] asked of a live connection rather than rendered into a clause.
///
/// The stamp half of the **pair** guard (`Rule::pair`): a column held back
/// because the user owns its *other* half has no `user` row of its own, so
/// [`stamp`]'s `WHERE` — which only ever protects the same field — would claim
/// it for a provider whose value the merge refused to write. Same predicate,
/// same `USER` spelling, asked the other way round.
pub(super) async fn user_owns_now(
    conn: &mut SqliteConnection,
    book_id: i64,
    field: &str,
) -> Result<bool> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM field_provenance WHERE book_id = ? AND field = ? AND source = ?",
    )
    .bind(book_id)
    .bind(field)
    .bind(USER)
    .fetch_one(&mut *conn)
    .await?;
    Ok(n > 0)
}

/// One field's attribution, as stored.
///
/// `source` is the raw column and not a [`Source`], deliberately: there is no
/// `CHECK`, so a row written by another build is *data*, and the same choice
/// `readings.status` made (`STATUS_*` are `&str` consts, not an enum at the
/// boundary) applies for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSource {
    pub field: String,
    pub source: String,
    pub fetched_at: i64,
}

/// Record that `source` supplied `fields` on this book, now.
///
/// **Takes a connection, not `&self`**, because it must run inside the same
/// transaction as the write it describes. A book whose `page_count` landed and
/// whose provenance did not is a field that silently claims whatever it claimed
/// last — which is exactly the state this item exists to remove.
///
/// A field the user owns is skipped unless `source` is [`Source::User`]: the
/// value was held back by the merge clause, so claiming it for the provider
/// would attribute a value that provider never supplied.
pub(super) async fn stamp(
    conn: &mut SqliteConnection,
    book_id: i64,
    fields: &[&'static str],
    source: Source,
) -> Result<()> {
    let now = now_unix();
    for field in fields {
        sqlx::query(
            "INSERT INTO field_provenance (book_id, field, source, fetched_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (book_id, field) DO UPDATE SET
                 source = excluded.source, fetched_at = excluded.fetched_at
             WHERE field_provenance.source <> ?5 OR excluded.source = ?5",
        )
        .bind(book_id)
        .bind(field)
        .bind(source.as_str())
        .bind(now)
        .bind(USER)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Move `src`'s claims onto `dst` for the listed fields only.
///
/// `merge_books`'s fill is **`dst`-wins**, so only the fields `dst` had nothing
/// to say about actually take `src`'s value — and only those may take `src`'s
/// attribution. Moving the rest would stamp `dst`'s own value with the source of
/// a value that was discarded.
///
/// `UPDATE OR IGNORE`, like `book_tags`: a field both books have a claim for
/// cannot move, and the loser goes with the cascade a moment later.
pub(super) async fn move_claims(
    conn: &mut SqliteConnection,
    src: i64,
    dst: i64,
    fields: &[&'static str],
) -> Result<()> {
    for field in fields {
        sqlx::query(
            "UPDATE OR IGNORE field_provenance SET book_id = ? WHERE book_id = ? AND field = ?",
        )
        .bind(dst)
        .bind(src)
        .bind(field)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

impl Storage {
    /// Every attributed field on a book, in a stable order.
    ///
    /// **Absence is ordinary and means "nobody has claimed this".** Migration
    /// `0012` back-fills nothing, so every book that predates it reports an
    /// empty list however well-populated its row is; so does every field written
    /// by a path that cannot honestly say which source supplied it. A caller
    /// that renders "unknown" for a missing field is right; one that renders
    /// "readingbuddy" is inventing the thing this table exists to stop.
    pub async fn field_provenance(&self, book_id: i64) -> Result<Vec<FieldSource>> {
        let rows = sqlx::query(
            "SELECT field, source, fetched_at FROM field_provenance
             WHERE book_id = ? ORDER BY field ASC",
        )
        .bind(book_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| FieldSource {
                field: r.get("field"),
                source: r.get("source"),
                fetched_at: r.get("fetched_at"),
            })
            .collect())
    }

    /// The source that supplied one field, if anyone has claimed it.
    pub async fn field_source(&self, book_id: i64, field: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT source FROM field_provenance WHERE book_id = ? AND field = ?",
        )
        .bind(book_id)
        .bind(field)
        .fetch_optional(self.pool())
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;

    async fn seeded() -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    page_count: Some(490),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        (s, id)
    }

    /// Migration `0012` back-fills nothing, and `None` is a real answer rather
    /// than a forgotten argument. Both show up the same way, which is the point:
    /// an absent row means "nobody has claimed this", and no caller can mistake
    /// it for a claim.
    #[tokio::test]
    async fn an_unattributed_write_claims_nothing() {
        let (s, id) = seeded().await;
        assert_eq!(s.field_provenance(id).await.unwrap(), vec![]);
        assert_eq!(s.field_source(id, "page_count").await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_claim_records_its_source_and_when() {
        let (s, id) = seeded().await;
        s.enrich_book(
            id,
            &Book {
                page_count: Some(512),
                ..Default::default()
            },
            Some(Source::OpenLibrary),
        )
        .await
        .unwrap();

        let got = s.field_provenance(id).await.unwrap();
        assert_eq!(got.len(), 1, "only the field it spoke to: {got:?}");
        assert_eq!(got[0].field, "page_count");
        assert_eq!(got[0].source, "openlibrary");
        assert!(got[0].fetched_at > 0, "staleness needs a real timestamp");

        // A later source replaces the claim rather than adding one: this table
        // keeps the winner, and a second row for the same field is not
        // representable.
        s.enrich_book(
            id,
            &Book {
                page_count: Some(500),
                ..Default::default()
            },
            Some(Source::GoogleBooks),
        )
        .await
        .unwrap();
        let got = s.field_provenance(id).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "googlebooks");
    }

    /// The stamp guard, stated on its own: once the user owns a field no other
    /// source may claim it, and the user may always reclaim it. Its twin is the
    /// *value* guard in `merge_set` — the two have to agree, and
    /// `a_user_owned_field_survives_every_provider_merge` is where they are
    /// checked together.
    #[tokio::test]
    async fn a_user_claim_cannot_be_taken_by_a_provider() {
        let (s, id) = seeded().await;
        let page = |n| Book {
            page_count: Some(n),
            ..Default::default()
        };

        s.enrich_book(id, &page(496), Some(Source::User))
            .await
            .unwrap();
        for source in Source::ALL.into_iter().filter(|s| *s != Source::User) {
            s.enrich_book(id, &page(1), Some(source)).await.unwrap();
            assert_eq!(
                s.field_source(id, "page_count").await.unwrap().as_deref(),
                Some("user"),
                "{} took a field the user owns",
                source.as_str()
            );
        }
        assert_eq!(s.get_book(id).await.unwrap().unwrap().page_count, Some(496));

        // The user is not locked out of their own field.
        s.enrich_book(id, &page(498), Some(Source::User))
            .await
            .unwrap();
        assert_eq!(s.get_book(id).await.unwrap().unwrap().page_count, Some(498));
    }

    /// Every source in the vocabulary round-trips through the column, which is
    /// what a `TEXT` column with no `CHECK` is trading on.
    #[tokio::test]
    async fn every_source_writes_the_string_it_says_it_does() {
        for source in Source::ALL {
            let (s, id) = seeded().await;
            s.enrich_book(
                id,
                &Book {
                    description: Some("x".into()),
                    ..Default::default()
                },
                Some(source),
            )
            .await
            .unwrap();
            assert_eq!(
                s.field_source(id, "description").await.unwrap().as_deref(),
                Some(source.as_str())
            );
        }
    }
}
