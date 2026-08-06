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
const NOTE_LINK_INDEX: i64 = 8;
const GOODREADS: i64 = 9;
const SORT_KEY_INDEXES: i64 = 16;
const MOMENTS: i64 = 17;

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

/// Version numbers are a cross-branch resource, and this is the only thing that
/// notices when two of them collide.
///
/// Threads run in parallel, each owning a migration, and each picks its number
/// from what is on `main` when it starts. Two threads that both pick `0008` do
/// not conflict in git — the filenames differ past the number — so the collision
/// arrives as a second migration sharing a version, which sqlx applies in
/// whatever order the filenames sort in and records under one version. The
/// symptom lands much later and somewhere else.
///
/// Contiguity rather than mere uniqueness, because a *gap* is the same mistake
/// caught earlier: it means a number was claimed by a branch that has not merged
/// yet, and merging out of numeric order is what the build order forbids.
///
/// One caveat, and it is why this cannot be the *only* guard: `sqlx::migrate!`
/// is a compile-time macro, and dropping a new `.sql` into the directory does
/// not on its own invalidate this test binary — so locally the check can appear
/// to pass until something else forces a rebuild. CI always compiles from a
/// fresh checkout, and the `migrations` job in `ci.yml` catches the other half
/// (an *edited* migration) straight from the diff.
#[test]
fn migration_versions_are_contiguous_from_one() {
    let versions: Vec<i64> = MIGRATIONS.iter().map(|m| m.version).collect();

    assert!(
        !versions.is_empty(),
        "no migrations found — sqlx::migrate! resolved an empty directory"
    );

    let expected: Vec<i64> = (1..=versions.len() as i64).collect();
    assert_eq!(
        versions,
        expected,
        "migration versions must be contiguous 1..={}, got {versions:?}. \
         A duplicate means two branches claimed the same number; a gap means one \
         claimed a number and has not merged.",
        versions.len()
    );
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

/// The plan for one of the two queries `0008` exists for.
async fn plan(conn: &mut SqliteConnection, sql: &str) -> String {
    sqlx::query(&format!("EXPLAIN QUERY PLAN {sql}"))
        .fetch_all(conn)
        .await
        .expect("query plan")
        .iter()
        .map(|r| r.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// `0008` adds two indexes and changes nothing else, so the only claim it can
/// be judged on is that the planner reaches them.
///
/// Not ceremony. An index carries a collation and SQLite ignores one that does
/// not match the comparison's, so the obvious
/// `CREATE INDEX … ON note_links(target_title)` is BINARY and the
/// `COLLATE NOCASE` back-resolution in `write_links` scans straight past it —
/// an index that exists, reads correctly in the schema, and does nothing. The
/// `before` half is what makes the `after` half evidence rather than a
/// coincidence.
#[tokio::test]
async fn the_note_link_indexes_are_the_plan_the_planner_picks() {
    // `Storage::backlinks`, and `write_links`' back-resolution.
    const BACKLINKS: &str = "SELECT from_note FROM note_links WHERE to_note = 1";
    const RESOLVE: &str = "UPDATE note_links SET to_note = 1 \
         WHERE to_note IS NULL AND target_title = 'x' COLLATE NOCASE";

    let mut conn = migrated_below(NOTE_LINK_INDEX).await;
    for sql in [BACKLINKS, RESOLVE] {
        let detail = plan(&mut conn, sql).await;
        assert!(
            detail.contains("SCAN"),
            "before 0008 this must be a scan, or the test proves nothing: {detail}"
        );
    }

    apply(&mut conn, NOTE_LINK_INDEX).await;

    for (sql, index) in [
        (BACKLINKS, "idx_note_links_to"),
        (RESOLVE, "idx_note_links_target"),
    ] {
        let detail = plan(&mut conn, sql).await;
        assert!(
            detail.contains(index),
            "expected {index} in the plan, got {detail}"
        );
    }
}

/// `0016`'s before-and-after, in `0008`'s shape.
///
/// The engine's own
/// `storage::books::tests::the_sort_key_indexes_are_the_plan_the_planner_picks`
/// is the stronger half — it runs the exact statement `list_books` builds, so a
/// clause that stops matching its index fails there. What that one cannot show
/// is what the schema looked like *before*, and "the index is used" only means
/// something against a plan that did not use one. This is that half.
///
/// The `sort_author` clause is deliberately absent: the column does not exist
/// below this migration, so a "before" for it would be a syntax error rather
/// than a scan. Three sorts is what can honestly be compared across the line.
#[tokio::test]
async fn the_sort_key_indexes_are_a_change_of_plan() {
    const BY_RECENCY: &str =
        "SELECT id FROM books ORDER BY books.last_modified DESC, books.id DESC LIMIT 20";
    const BY_TITLE: &str = "SELECT id FROM books \
         ORDER BY COALESCE(books.sort_title, books.title) COLLATE NOCASE ASC, books.id ASC \
         LIMIT 20";
    const BY_YEAR: &str =
        "SELECT id FROM books ORDER BY books.publish_year DESC NULLS LAST, books.id DESC LIMIT 20";

    let mut conn = migrated_below(SORT_KEY_INDEXES).await;
    for sql in [BY_RECENCY, BY_TITLE, BY_YEAR] {
        let detail = plan(&mut conn, sql).await;
        assert!(
            detail.contains("USE TEMP B-TREE FOR ORDER BY"),
            "before 0016 every sort key sorted the whole table, or this test \
             proves nothing: {detail}"
        );
    }

    apply(&mut conn, SORT_KEY_INDEXES).await;

    for (sql, index) in [
        (BY_RECENCY, "idx_books_last_modified"),
        (BY_TITLE, "idx_books_sort_title"),
        (BY_YEAR, "idx_books_publish_year"),
    ] {
        let detail = plan(&mut conn, sql).await;
        assert!(
            detail.contains(index),
            "expected {index} in the plan, got {detail}"
        );
        assert!(
            !detail.contains("USE TEMP B-TREE FOR ORDER BY"),
            "{index} was scanned and the rows were sorted anyway: {detail}"
        );
    }
}

/// **The collation is part of the index**, and `0008` learned it the hard way.
///
/// `idx_books_sort_title` is declared `COLLATE NOCASE` because `BookSort::Title`
/// compares that way. Written without it — or over the bare column rather than
/// the `COALESCE` the clause names — the index exists, reads correctly in the
/// schema, and is silently never used. This asserts that the two *near misses*
/// really do miss, so the declaration above is doing work rather than
/// coinciding.
#[tokio::test]
async fn a_sort_title_index_that_nearly_matches_is_not_used() {
    let mut conn = migrated_below(SORT_KEY_INDEXES).await;
    apply(&mut conn, SORT_KEY_INDEXES).await;

    for near_miss in [
        // Right expression, wrong collation.
        "SELECT id FROM books ORDER BY COALESCE(books.sort_title, books.title) ASC, books.id ASC",
        // Right collation, wrong expression — the bare column the obvious
        // implementation would have indexed.
        "SELECT id FROM books ORDER BY books.sort_title COLLATE NOCASE ASC, books.id ASC",
    ] {
        let detail = plan(&mut conn, near_miss).await;
        assert!(
            !detail.contains("idx_books_sort_title"),
            "a clause that differs from the index must not reach it, or the \
             index's declaration is not what makes the real one work: {detail}"
        );
    }
}

/// The trap in item 10, caught at the level it is actually set: the migration.
///
/// `active_rating_scale()` used to be "the newest scale", so seeding a
/// `goodreads` one would have made it the scale every unqualified rating landed
/// on. `is_default` back-fills to **whichever scale the old ordering would have
/// picked** — not to `default` by name — so a library where the user made their
/// own scale keeps using theirs, and the seeded `goodreads` row takes nothing.
#[tokio::test]
async fn the_default_scale_survives_seeding_goodreads() {
    let mut conn = migrated_below(GOODREADS).await;
    // A scale the user made after the seeded `default`, which is exactly what
    // the old ordering rule meant by "active".
    sqlx::query(
        "INSERT INTO rating_scales (name, min, max, step, created_at)
         VALUES ('mine', 0.0, 10.0, 1.0, strftime('%s','now') + 10)",
    )
    .execute(&mut conn)
    .await
    .expect("insert scale");

    apply(&mut conn, GOODREADS).await;

    let default: String = sqlx::query_scalar("SELECT name FROM rating_scales WHERE is_default = 1")
        .fetch_one(&mut conn)
        .await
        .expect("exactly one default");
    assert_eq!(default, "mine");

    let goodreads: i64 =
        sqlx::query_scalar("SELECT is_default FROM rating_scales WHERE name = 'goodreads'")
            .fetch_one(&mut conn)
            .await
            .expect("0009 seeds it");
    assert_eq!(goodreads, 0, "the newest scale is not the default any more");

    // And the seeded map is the identity, so an imported `My Rating` needs no
    // translation to be exported again.
    let mapped: Vec<(f64, i64)> = sqlx::query_as(
        "SELECT value, goodreads FROM rating_map
          WHERE scale_id = (SELECT id FROM rating_scales WHERE name = 'goodreads')
          ORDER BY value",
    )
    .fetch_all(&mut conn)
    .await
    .expect("map");
    assert_eq!(mapped.len(), 6);
    assert!(mapped.iter().all(|(v, g)| *v as i64 == *g));
}

/// "Exactly one default" is an index, not a convention — the same shape as
/// `idx_readings_one_open` and for the same reason: two of them is a state no
/// code should have to defend against.
#[tokio::test]
async fn a_second_default_scale_is_refused() {
    let mut conn = migrated_below(GOODREADS).await;
    apply(&mut conn, GOODREADS).await;

    let err = sqlx::query("UPDATE rating_scales SET is_default = 1 WHERE name = 'goodreads'")
        .execute(&mut conn)
        .await
        .expect_err("two defaults must be refused");
    assert!(
        matches!(&err, sqlx::Error::Database(db) if db.is_unique_violation()),
        "expected a unique violation, got {err}"
    );
}

/// The moments epoch is a **singleton written by the migration itself**, and
/// both halves of that are load-bearing (item 23).
///
/// Its value is what stops the first launch after `0017` replaying an entire
/// reading history as a ceremony, so a database where the row is missing has no
/// answer to "is this news or is this history" — and a database with two has
/// whichever answer a query happened to read. `CHECK (id = 1)` is the same
/// device `idx_readings_one_open` and `idx_one_reflection` are: an invariant a
/// table holds rather than a convention code defends.
#[tokio::test]
async fn the_moments_epoch_is_one_row_and_the_migration_wrote_it() {
    let mut conn = migrated_below(MOMENTS).await;
    apply(&mut conn, MOMENTS).await;

    let began: i64 = sqlx::query_scalar("SELECT began_at FROM moment_epoch")
        .fetch_one(&mut conn)
        .await
        .expect("the migration writes its own epoch");
    assert!(
        began > 1_700_000_000,
        "the epoch is the instant the schema learned about moments, not zero: {began}"
    );

    let err = sqlx::query("INSERT INTO moment_epoch (id, began_at) VALUES (2, 0)")
        .execute(&mut conn)
        .await
        .expect_err("a second epoch must be refused");
    assert!(
        matches!(&err, sqlx::Error::Database(db) if db.message().contains("CHECK")),
        "expected the CHECK to refuse it, got {err}"
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
