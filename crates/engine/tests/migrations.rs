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

/// A connection migrated up to (but not including) `0005`.
async fn before_readings() -> SqliteConnection {
    let mut conn = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open in-memory db");
    conn.ensure_migrations_table()
        .await
        .expect("migrations table");
    for m in MIGRATIONS.iter().filter(|m| m.version < READINGS) {
        conn.apply(m).await.expect("apply migration");
    }
    conn
}

async fn apply_readings(conn: &mut SqliteConnection) {
    for m in MIGRATIONS.iter().filter(|m| m.version == READINGS) {
        conn.apply(m).await.expect("apply 0005");
    }
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
