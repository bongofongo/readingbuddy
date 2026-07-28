//! The one migration in this repo that destroys data.
//!
//! `0005_readings.sql` back-fills `readings` from `books` and then drops the
//! four progress columns. Everything else in the suite connects a *fully*
//! migrated database, which can never exercise the back-fill: by the time the
//! first row is written the columns it reads are already gone. So these tests
//! apply the migrations in two halves, write a pre-`0005` library in between,
//! and assert what survived.

use sqlx::migrate::{Migrate, Migrator};
use sqlx::sqlite::SqliteConnection;
use sqlx::{Connection, Row};

static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

const READINGS: i64 = 5;
const REFLECTION: i64 = 7;

/// A connection migrated up to (but not including) `version`.
async fn migrated_below(version: i64) -> SqliteConnection {
    let mut conn = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open in-memory db");
    conn.ensure_migrations_table()
        .await
        .expect("migrations table");
    for m in MIGRATIONS.iter().filter(|m| m.version < version) {
        conn.apply(m).await.expect("apply migration");
    }
    conn
}

async fn apply(conn: &mut SqliteConnection, version: i64) {
    for m in MIGRATIONS.iter().filter(|m| m.version == version) {
        conn.apply(m).await.expect("apply migration");
    }
}

async fn before_readings() -> SqliteConnection {
    migrated_below(READINGS).await
}

async fn apply_readings(conn: &mut SqliteConnection) {
    apply(conn, READINGS).await;
}

/// Insert a pre-`0005` book straight through the old columns.
async fn old_book(
    conn: &mut SqliteConnection,
    title: &str,
    current_page: Option<i64>,
    finished: i64,
    date_started: Option<i64>,
    date_finished: Option<i64>,
) -> i64 {
    sqlx::query(
        "INSERT INTO books (title, current_page, finished, date_started, date_finished,
                            created_at, last_modified)
         VALUES (?, ?, ?, ?, ?, 0, 0) RETURNING id",
    )
    .bind(title)
    .bind(current_page)
    .bind(finished)
    .bind(date_started)
    .bind(date_finished)
    .fetch_one(&mut *conn)
    .await
    .expect("insert old book")
    .get("id")
}

#[tokio::test]
async fn the_backfill_round_trips_every_shape_of_progress() {
    let mut conn = before_readings().await;

    let part = old_book(&mut conn, "part read", Some(120), 0, Some(1_000), None).await;
    let done = old_book(&mut conn, "finished", Some(490), 1, Some(1), Some(2)).await;
    // Finished with no recorded end date: the back-fill leaves `finished_at`
    // NULL, so this reading counts as *open* while its status says finished.
    // That is deliberate — inventing a date would be worse than an open reading
    // the user can close.
    let dateless = old_book(&mut conn, "finished, undated", None, 1, None, None).await;
    let untouched = old_book(&mut conn, "never opened", None, 0, None, None).await;

    apply_readings(&mut conn).await;

    let rows = sqlx::query(
        "SELECT book_id, started_at, finished_at, status, source, current_page
         FROM readings ORDER BY book_id",
    )
    .fetch_all(&mut conn)
    .await
    .expect("select readings");

    assert_eq!(rows.len(), 3, "a book with no progress gets no reading");
    let by_book: Vec<i64> = rows.iter().map(|r| r.get("book_id")).collect();
    assert_eq!(by_book, vec![part, done, dateless]);
    assert!(!by_book.contains(&untouched));

    assert_eq!(rows[0].get::<Option<i64>, _>("current_page"), Some(120));
    assert_eq!(rows[0].get::<Option<i64>, _>("started_at"), Some(1_000));
    assert_eq!(rows[0].get::<Option<i64>, _>("finished_at"), None);
    assert_eq!(rows[0].get::<String, _>("status"), "reading");
    assert_eq!(rows[0].get::<String, _>("source"), "migrated");

    assert_eq!(rows[1].get::<Option<i64>, _>("current_page"), Some(490));
    assert_eq!(rows[1].get::<Option<i64>, _>("started_at"), Some(1));
    assert_eq!(rows[1].get::<Option<i64>, _>("finished_at"), Some(2));
    assert_eq!(rows[1].get::<String, _>("status"), "finished");

    assert_eq!(rows[2].get::<String, _>("status"), "finished");
    assert_eq!(rows[2].get::<Option<i64>, _>("finished_at"), None);
}

/// The columns are really gone. A migration that back-filled but left them
/// behind would keep every stale writer silently working.
#[tokio::test]
async fn the_progress_columns_leave_books() {
    let mut conn = before_readings().await;
    apply_readings(&mut conn).await;

    let names: Vec<String> = sqlx::query("SELECT name FROM pragma_table_info('books')")
        .fetch_all(&mut conn)
        .await
        .expect("pragma")
        .into_iter()
        .map(|r| r.get("name"))
        .collect();
    for gone in ["current_page", "finished", "date_started", "date_finished"] {
        assert!(!names.contains(&gone.to_string()), "books still has {gone}");
    }
}

/// The back-fill writes exactly one reading per book, so it cannot itself
/// violate the "at most one open reading" index — but the index has to be
/// *there* for anything written afterwards.
#[tokio::test]
async fn the_one_open_reading_index_exists_after_the_backfill() {
    let mut conn = before_readings().await;
    let id = old_book(&mut conn, "part read", Some(10), 0, None, None).await;
    apply_readings(&mut conn).await;

    let err = sqlx::query(
        "INSERT INTO readings (book_id, status, source, created_at, last_modified)
         VALUES (?, 'reading', 'manual', 0, 0)",
    )
    .bind(id)
    .execute(&mut conn)
    .await
    .expect_err("a second open reading must be refused");
    assert!(
        matches!(&err, sqlx::Error::Database(db) if db.is_unique_violation()),
        "expected a unique violation, got {err}"
    );
}

/// `0007` rewrites the superseded `final` kind. A vault file is the note's
/// body, so the rewrite must move the *row* and leave the path alone — a note
/// whose `file_path` shifted would be a note whose text vanished.
#[tokio::test]
async fn old_final_notes_become_reflections_and_keep_their_files() {
    let mut conn = migrated_below(REFLECTION).await;
    for (path, title, kind) in [
        ("unsorted/a.md", "Last thoughts", "final"),
        ("unsorted/b.md", "A stray idea", "note"),
        ("unsorted/c.md", "Mid-book", "session"),
    ] {
        sqlx::query(
            "INSERT INTO notes (file_path, title, kind, created_at, last_modified)
             VALUES (?, ?, ?, 0, 0)",
        )
        .bind(path)
        .bind(title)
        .bind(kind)
        .execute(&mut conn)
        .await
        .expect("insert pre-0007 note");
    }

    apply(&mut conn, REFLECTION).await;

    let rows = sqlx::query("SELECT file_path, kind, reading_id FROM notes ORDER BY file_path")
        .fetch_all(&mut conn)
        .await
        .expect("select notes");
    assert_eq!(rows[0].get::<String, _>("kind"), "reflection");
    assert_eq!(rows[0].get::<String, _>("file_path"), "unsorted/a.md");
    assert_eq!(rows[0].get::<Option<i64>, _>("reading_id"), None);
    assert_eq!(rows[1].get::<String, _>("kind"), "note");
    assert_eq!(rows[2].get::<String, _>("kind"), "session");
}

/// Several old `final` notes migrate at once, and they all land with
/// `reading_id IS NULL` — under a plain unique index that would be a violation
/// and the whole migration would fail on a real library. SQLite treats NULLs as
/// distinct, which is exactly what makes the partial index usable here.
#[tokio::test]
async fn the_rewrite_survives_a_library_full_of_final_notes() {
    let mut conn = migrated_below(REFLECTION).await;
    for i in 0..5 {
        sqlx::query(
            "INSERT INTO notes (file_path, title, kind, created_at, last_modified)
             VALUES (?, ?, 'final', 0, 0)",
        )
        .bind(format!("unsorted/{i}.md"))
        .bind(format!("Note {i}"))
        .execute(&mut conn)
        .await
        .expect("insert");
    }
    apply(&mut conn, REFLECTION).await;

    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM notes WHERE kind = 'reflection'")
        .fetch_one(&mut conn)
        .await
        .expect("count");
    assert_eq!(n, 5);
}

/// The two partial indexes are the invariant "one reflection and one review per
/// reading". Asserted here against raw SQL rather than through the engine,
/// because it is the *index* that has to hold — the engine's accretion path
/// looks the existing note up and never gets this far.
#[tokio::test]
async fn a_reading_holds_one_reflection_and_one_review() {
    let mut conn = migrated_below(REFLECTION).await;
    apply(&mut conn, REFLECTION).await;

    let book: i64 = sqlx::query_scalar(
        "INSERT INTO books (title, created_at, last_modified) VALUES ('t', 0, 0) RETURNING id",
    )
    .fetch_one(&mut conn)
    .await
    .expect("book");
    let reading: i64 = sqlx::query_scalar(
        "INSERT INTO readings (book_id, status, source, created_at, last_modified)
         VALUES (?, 'reading', 'manual', 0, 0) RETURNING id",
    )
    .bind(book)
    .fetch_one(&mut conn)
    .await
    .expect("reading");

    let add = |path: &'static str, kind: &'static str| {
        sqlx::query(
            "INSERT INTO notes (reading_id, file_path, title, kind, created_at, last_modified)
             VALUES (?, ?, ?, ?, 0, 0)",
        )
        .bind(reading)
        .bind(path)
        .bind(path)
        .bind(kind)
    };

    add("a.md", "reflection")
        .execute(&mut conn)
        .await
        .expect("first reflection");
    add("b.md", "review")
        .execute(&mut conn)
        .await
        .expect("a review is a different kind, so it fits beside it");

    for (path, kind) in [("c.md", "reflection"), ("d.md", "review")] {
        let err = add(path, kind)
            .execute(&mut conn)
            .await
            .expect_err("a second one of the same kind must be refused");
        assert!(
            matches!(&err, sqlx::Error::Database(db) if db.is_unique_violation()),
            "expected a unique violation for a second {kind}, got {err}"
        );
    }

    // Ordinary notes are not constrained: a reading collects as many as it likes.
    for path in ["e.md", "f.md"] {
        add(path, "note")
            .execute(&mut conn)
            .await
            .expect("notes are unconstrained");
    }
}

mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Whatever a book's progress was, the back-fill reproduces it exactly:
        /// one reading, same page, same dates, and a status that agrees with the
        /// old `finished` flag.
        ///
        /// A property rather than more examples because the rule is general over
        /// a four-field product — page present or not, finished or not, either
        /// date present or not — and the sixteen corners include the two that
        /// decide whether a *row is written at all*. Hand-picking three of them
        /// picks the three that work.
        #[test]
        fn every_pre_migration_book_round_trips(
            page in proptest::option::of(0i64..2000),
            finished in 0i64..2,
            started in proptest::option::of(0i64..2_000_000_000),
            ended in proptest::option::of(0i64..2_000_000_000),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all().build().unwrap();
            rt.block_on(async {
                let mut conn = before_readings().await;
                let id = old_book(&mut conn, "t", page, finished, started, ended).await;
                apply_readings(&mut conn).await;

                let rows = sqlx::query(
                    "SELECT started_at, finished_at, status, current_page
                     FROM readings WHERE book_id = ?")
                    .bind(id)
                    .fetch_all(&mut conn)
                    .await
                    .expect("select");

                let has_progress = page.is_some() || finished == 1
                    || started.is_some() || ended.is_some();
                if !has_progress {
                    prop_assert!(rows.is_empty(), "nothing to record, nothing recorded");
                    return Ok(());
                }
                prop_assert_eq!(rows.len(), 1, "exactly one reading per book");
                prop_assert_eq!(rows[0].get::<Option<i64>, _>("current_page"), page);
                prop_assert_eq!(rows[0].get::<Option<i64>, _>("started_at"), started);
                prop_assert_eq!(rows[0].get::<Option<i64>, _>("finished_at"), ended);
                prop_assert_eq!(
                    rows[0].get::<String, _>("status"),
                    if finished == 1 { "finished" } else { "reading" }
                );
                Ok(())
            })?;
        }
    }
}
