//! Goodreads CSV, in and out, through the facade.
//!
//! Two fixture sets, and the split is the point (see
//! `tests/fixtures/goodreads/README.md`): `recorded/` is hand-authored because
//! a Goodreads CSV is an artifact of *another* system and generating one from
//! our own understanding of the format would prove only that we agree with
//! ourselves; `generated/` is `cargo run -p corpus -- gen-goodreads` and covers
//! volume rather than shape.
//!
//! Offline throughout — nothing here reaches a provider, and the engine runs on
//! `sqlite::memory:` with a temp vault.

mod common;

use std::path::{Path, PathBuf};

use readingbuddy::goodreads::{ImportOptions, Shelf, parse_csv};
use readingbuddy::{Book, DiagnosticKind, Engine, GoodreadsMatch, TextOutcome};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/goodreads");

fn recorded(name: &str) -> PathBuf {
    Path::new(FIXTURES).join("recorded").join(name)
}

fn generated() -> PathBuf {
    Path::new(FIXTURES).join("generated/library.csv")
}

fn plain() -> ImportOptions {
    ImportOptions::default()
}

/// Ambiguity is the matcher's subject, not the format's. Where a test is about
/// the *format* — the round trip, the corpus — a row landing in the candidate
/// band would silently shrink the library and make the assertion mean something
/// else, so those tests take the same escape hatch a user would (`--new`).
fn creating() -> ImportOptions {
    ImportOptions {
        create_ambiguous: true,
        ..Default::default()
    }
}

/// Write `csv` somewhere the engine can read it.
fn tmp_csv(dir: &Path, name: &str, csv: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, csv).expect("write csv");
    path
}

// ---- parsing ---------------------------------------------------------------

/// Every shape in this file was a mistake waiting to happen, and this is the one
/// test that looks at them before a database is involved.
#[test]
fn the_recorded_export_parses_the_shapes_only_a_real_file_has() {
    let rows = parse_csv(&recorded("library-export.csv")).expect("parses");
    assert_eq!(rows.len(), 5);

    // Excel armour off, then `normalize_isbn` like everything else.
    assert_eq!(rows[0].isbn_13.as_deref(), Some("9781455563937"));
    assert_eq!(rows[0].isbn_10.as_deref(), Some("1455563935"));
    assert_eq!(rows[0].external_id.as_deref(), Some("34051011"));
    assert_eq!(rows[0].shelves, vec!["korean-lit", "favourites"]);
    assert_eq!(rows[0].exclusive_shelf, Some(Shelf::Read));

    // `Read Count 3` with one `Date Read`.
    assert_eq!(rows[1].read_count, 3);
    assert!(rows[1].date_read.is_some());
    // The translator comes through `Additional Authors`.
    assert_eq!(rows[1].authors.len(), 2);

    // `=""` is armour around nothing, which is not an ISBN and is not `""`.
    assert_eq!(rows[2].isbn_10, None);
    assert_eq!(rows[2].isbn_13, None);
    assert_eq!(rows[2].exclusive_shelf, Some(Shelf::CurrentlyReading));

    // An empty `My Rating` and an explicit `0` are different cells — and
    // neither is a rating.
    assert_eq!(rows[2].my_rating, None, "an empty cell said nothing");
    assert_eq!(rows[3].my_rating, Some(0), "a literal 0 said 'unrated'");

    // A comma and a double quote inside a title, and an embedded newline
    // inside a review — both inside CRLF line endings.
    assert_eq!(
        rows[4].title.as_deref(),
        Some(r#"The Catcher in the Rye, or "Holden""#)
    );
    assert!(rows[4].review.as_deref().unwrap().contains('\n'));
    assert_eq!(
        rows[4].private_notes.as_deref(),
        Some("Borrowed from Dad; give it back.")
    );
    // If CRLF survived into the last column, this would be `1\r`.
    assert_eq!(rows[4].read_count, 1);
}

/// Goodreads' exporter writes `My Review`; its importer documents `Review`, and
/// that is the name our own export writes. A file we wrote has to be a file we
/// can read.
#[test]
fn the_importers_smaller_column_set_reads_too() {
    let rows = parse_csv(&recorded("importer-columns.csv")).expect("parses");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].my_rating, Some(5));
    assert_eq!(rows[0].review.as_deref(), Some("A river of a book."));
    assert_eq!(rows[0].isbn_13.as_deref(), Some("9781455563937"));
    // No `Exclusive Shelf` column at all, so `Date Read` is what says it was
    // read — otherwise our own export could not be re-imported.
    assert_eq!(rows[0].exclusive_shelf, None);
    assert_eq!(rows[1].date_read, None);
}

#[tokio::test]
async fn a_file_that_is_not_a_goodreads_export_says_so() {
    let (tmp, engine) = common::engine().await;
    let path = tmp_csv(tmp.path(), "nope.csv", "a,b,c\n1,2,3\n");
    let err = engine
        .import_goodreads(&path, plain())
        .await
        .expect_err("no Title column");
    assert!(err.to_string().contains("Title"), "{err}");
}

// ---- import ----------------------------------------------------------------

#[tokio::test]
async fn the_recorded_export_lands_where_it_should() {
    let (_tmp, engine) = common::engine().await;
    let report = engine
        .import_goodreads(&recorded("library-export.csv"), plain())
        .await
        .expect("imports");

    assert_eq!(report.rows, 5);
    assert_eq!(report.books.len(), 5);
    assert_eq!(report.created(), 5, "an empty library creates every row");
    assert!(report.unmatched.is_empty());

    // `read`, once, with a date and a review and a rating.
    let pachinko = book(&engine, "Pachinko").await;
    let readings = engine
        .storage()
        .list_readings(pachinko.id.unwrap())
        .await
        .unwrap();
    assert_eq!(readings.len(), 1);
    assert!(readings[0].finished_at.is_some());
    assert_eq!(readings[0].source, "goodreads");
    assert_eq!(readings[0].status, "finished");
    assert_eq!(report.books[0].rating, Some(5));
    assert_eq!(report.books[0].review, TextOutcome::Written);

    // `Read Count 3` really is three readings.
    let veg = book(&engine, "The Vegetarian").await;
    let readings = engine
        .storage()
        .list_readings(veg.id.unwrap())
        .await
        .unwrap();
    assert_eq!(readings.len(), 3, "Read Count 3 means three readings");
    assert!(
        readings.iter().all(|r| r.started_at.is_none()),
        "Goodreads' CSV has no start date, and inventing one is the thing this refuses"
    );
    // Rated with no review: the review note is opened anyway, because a rating
    // has nowhere else to live.
    assert_eq!(report.books[1].rating, Some(4));

    // `currently-reading` is an open reading.
    let kokoro = book(&engine, "Kokoro").await;
    let readings = engine
        .storage()
        .list_readings(kokoro.id.unwrap())
        .await
        .unwrap();
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0].finished_at, None, "open");
    assert_eq!(readings[0].status, "reading");

    // `to-read` is a book with **no reading at all** — the honest encoding, and
    // why to-read needs no collection to live in. Its explicit `0` rating is
    // Goodreads for "unrated" and is not stored as a zero.
    let left = book(&engine, "The Left Hand of Darkness").await;
    assert!(
        engine
            .storage()
            .list_readings(left.id.unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(report.books[3].rating, None);

    // `Bookshelves` are inert provenance, raw value and all.
    let tags = engine.storage().book_tags(left.id.unwrap()).await.unwrap();
    assert_eq!(
        tags.iter().map(|t| t.tag.as_str()).collect::<Vec<_>>(),
        vec!["sci-fi", "to-read"]
    );
    assert!(tags.iter().all(|t| t.source == "goodreads"));

    // `Private Notes` are a note of their own — neither a review nor a
    // reflection, because folding them into either would put words in the
    // user's reflection that they did not write there.
    let catcher = book(&engine, "Catcher in the Rye").await;
    let notes = engine.list_notes(catcher.id).await.unwrap();
    assert!(
        notes
            .iter()
            .any(|n| n.title.starts_with("Goodreads notes:") && n.kind == "note"),
        "{notes:?}"
    );
    assert!(notes.iter().any(|n| n.kind == "review"));
}

/// The whole reason `external_ids` exists. Re-importing the same file must add
/// no reading, no review and no tag row.
#[tokio::test]
async fn re_importing_the_same_export_changes_nothing() {
    let (_tmp, engine) = common::engine().await;
    let path = recorded("library-export.csv");
    engine.import_goodreads(&path, plain()).await.unwrap();
    let before = snapshot(&engine).await;

    let again = engine.import_goodreads(&path, plain()).await.unwrap();
    assert_eq!(again.created(), 0, "no second copy of anything");
    assert!(again.books.iter().all(|b| b.readings_added == 0));
    assert!(again.books.iter().all(|b| b.tags_added == 0));
    assert!(
        again.books.iter().all(|b| b.rating.is_none()),
        "a rating already stored identically is not a change"
    );
    assert!(
        again
            .books
            .iter()
            .all(|b| b.review != TextOutcome::KeptOurs),
        "an unedited review is Unchanged, not a conflict"
    );
    assert_eq!(snapshot(&engine).await, before);
}

/// A book already here is found, not duplicated — and by its ISBN, which is the
/// branch that has to work before the fuzzy one is ever reached.
#[tokio::test]
async fn an_isbn_finds_the_book_already_on_the_shelf() {
    let (_tmp, engine) = common::engine().await;
    let existing = engine
        .save_book(&Book {
            // A title nothing in the file comes near — not even into the
            // candidate band — so only the ISBN can find it.
            title: Some("Bluets".into()),
            isbn_13: Some("9781455563937".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    let report = engine
        .import_goodreads(&recorded("library-export.csv"), plain())
        .await
        .unwrap();
    assert_eq!(report.books[0].matched_by, GoodreadsMatch::Isbn);
    assert_eq!(report.books[0].book_id, existing.id);
    assert_eq!(report.created(), 4, "the other four are new");
}

/// Below the auto-match band, a row is a **decision** rather than a duplicate.
/// The candidates are what make it one.
#[tokio::test]
async fn a_near_miss_is_reported_with_candidates_rather_than_duplicated() {
    let (_tmp, engine) = common::engine().await;
    // jaro-winkler on short titles is generous, and a shared prefix is enough
    // to land in the band — which is exactly the case where acting silently
    // used to mean a second copy of a book already on the shelf.
    let near = engine
        .save_book(&Book {
            title: Some("Pacific Ocean Notes".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    let path = recorded("importer-columns.csv");
    let report = engine.import_goodreads(&path, plain()).await.unwrap();
    let unmatched = report
        .unmatched
        .iter()
        .find(|u| u.title == "Pachinko")
        .expect("Pachinko is in the band, not over it");
    assert_eq!(unmatched.candidates.len(), 1);
    assert_eq!(unmatched.candidates[0].book_id, near.id.unwrap());
    assert!(
        (0.60..0.85).contains(&unmatched.candidates[0].score),
        "the fixture has drifted out of the candidate band: {}",
        unmatched.candidates[0].score
    );
    assert!(
        engine.resolve_books("Pachinko").await.unwrap().is_empty(),
        "nothing was created behind the user's back"
    );

    // And the escape hatch does what it says.
    let forced = engine.import_goodreads(&path, creating()).await.unwrap();
    assert!(forced.unmatched.is_empty());
    assert_eq!(forced.created(), 1);
}

/// A dry run answers the same questions and writes nothing at all.
#[tokio::test]
async fn a_dry_run_writes_nothing() {
    let (_tmp, engine) = common::engine().await;
    let path = recorded("library-export.csv");
    let preview = engine
        .import_goodreads(
            &path,
            ImportOptions {
                dry_run: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(preview.dry_run);
    assert_eq!(preview.books.len(), 5);
    assert!(preview.books.iter().all(|b| b.book_id.is_none()));
    assert_eq!(
        preview.books[1].readings_added, 3,
        "the preview counts what the import would write"
    );
    assert!(
        engine
            .storage()
            .list_books(50, readingbuddy::BookSort::Title)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(engine.list_notes(None).await.unwrap().is_empty());

    // And then the real thing agrees with it.
    let real = engine.import_goodreads(&path, plain()).await.unwrap();
    for (p, r) in preview.books.iter().zip(&real.books) {
        assert_eq!(p.title, r.title);
        assert_eq!(p.readings_added, r.readings_added, "{}", p.title);
        assert_eq!(p.tags_added, r.tags_added, "{}", p.title);
        assert_eq!(p.rating, r.rating, "{}", p.title);
    }
}

/// A review rewritten here is not the CSV's any more. The words came from
/// Goodreads, but they live in the vault now, and an import that overwrote them
/// would be the one outcome with no undo.
#[tokio::test]
async fn a_review_edited_here_survives_a_re_import() {
    let (_tmp, engine) = common::engine().await;
    let path = recorded("library-export.csv");
    engine.import_goodreads(&path, plain()).await.unwrap();

    let pachinko = book(&engine, "Pachinko").await;
    let review = engine
        .list_notes(pachinko.id)
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.kind == "review")
        .expect("imported review");
    engine
        .update_note_body(&review, "Rewritten here, months later.")
        .await
        .unwrap();

    let again = engine.import_goodreads(&path, plain()).await.unwrap();
    assert_eq!(again.books[0].review, TextOutcome::KeptOurs);
    assert!(
        again
            .warnings
            .iter()
            .any(|d| matches!(d.kind, DiagnosticKind::GoodreadsReviewDiverged { .. })),
        "silence would look exactly like success"
    );
    assert_eq!(
        engine.note_body(&review).unwrap(),
        "Rewritten here, months later."
    );
}

/// A rating on the user's own scale is a different number on a different axis,
/// and the map is many-to-one so the two cannot even be compared.
#[tokio::test]
async fn a_rating_on_our_own_scale_is_not_overwritten() {
    let (_tmp, engine) = common::engine().await;
    let path = recorded("library-export.csv");
    engine.import_goodreads(&path, plain()).await.unwrap();

    let pachinko = book(&engine, "Pachinko").await;
    let review = engine
        .list_notes(pachinko.id)
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.kind == "review")
        .unwrap();
    // The active scale is still `default` — which is the whole point of
    // `is_default` — so this is the user's own axis.
    engine.set_rating(review.id, 4.5).await.unwrap();

    let again = engine.import_goodreads(&path, plain()).await.unwrap();
    assert_eq!(again.books[0].rating, None);
    assert!(
        again
            .warnings
            .iter()
            .any(|d| matches!(d.kind, DiagnosticKind::GoodreadsRatingDiverged { .. }))
    );
    let kept = engine
        .storage()
        .review_rating(review.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.value, 4.5);
    assert_eq!(kept.scale.name, "default");
}

/// `Read Count 3` with nothing to close the earlier readings at. A reading with
/// no end date *is* an open one, and `idx_readings_one_open` permits one per
/// book — so the rest are reported rather than silently lost.
#[tokio::test]
async fn undatable_rereads_are_reported_not_dropped_in_silence() {
    let (tmp, engine) = common::engine().await;
    let path = tmp_csv(
        tmp.path(),
        "undated.csv",
        "Title,Exclusive Shelf,Read Count\nSalt,read,3\n",
    );
    let report = engine.import_goodreads(&path, plain()).await.unwrap();

    let salt = book(&engine, "Salt").await;
    let readings = engine
        .storage()
        .list_readings(salt.id.unwrap())
        .await
        .unwrap();
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0].finished_at, None);
    assert_eq!(readings[0].status, "finished", "closed in status, undated");

    let dropped = report
        .warnings
        .iter()
        .find_map(|d| match &d.kind {
            DiagnosticKind::GoodreadsUndatableRereads { dropped, .. } => Some(*dropped),
            _ => None,
        })
        .expect("said so");
    assert_eq!(dropped, 2);
}

// ---- export ----------------------------------------------------------------

/// A rounded star is precisely the failure the explicit lookup table exists to
/// refuse, so the row goes rather than the truth.
#[tokio::test]
async fn an_unmapped_rating_skips_its_row_and_says_why() {
    let (_tmp, engine) = common::engine().await;
    let id = common::seed_book(&engine, "Pachinko").await;
    let review = engine.open_review(id, None).await.unwrap();
    // Half a star on the seeded `default` scale, which migration `0007`
    // deliberately leaves unmapped.
    engine.set_rating(review.id, 4.5).await.unwrap();

    let (csv, warnings) = engine.export_goodreads().await.unwrap();
    assert!(!csv.contains("Pachinko"), "the row was skipped:\n{csv}");
    assert!(
        warnings
            .iter()
            .any(|d| matches!(d.kind, DiagnosticKind::GoodreadsRatingUnmapped { .. })),
        "{warnings:?}"
    );
    // And the fix is one command away, so mapping it brings the row back.
    let scale = engine
        .storage()
        .active_rating_scale()
        .await
        .unwrap()
        .unwrap();
    engine.storage().map_rating(&scale, 4.5, 5).await.unwrap();
    let (csv, warnings) = engine.export_goodreads().await.unwrap();
    assert!(csv.contains("Pachinko"));
    assert!(warnings.is_empty());
}

/// Goodreads' importable CSV has no read-count column. Truncating in silence
/// would look like data loss on the far side.
#[tokio::test]
async fn a_reread_exports_its_latest_reading_and_reports_the_rest() {
    let (_tmp, engine) = common::engine().await;
    engine
        .import_goodreads(&recorded("library-export.csv"), plain())
        .await
        .unwrap();

    let (csv, warnings) = engine.export_goodreads().await.unwrap();
    assert_eq!(csv.matches("The Vegetarian").count(), 1);
    let dropped = warnings
        .iter()
        .find_map(|d| match &d.kind {
            DiagnosticKind::GoodreadsRereadsDropped { title, dropped }
                if title == "The Vegetarian" =>
            {
                Some(*dropped)
            }
            _ => None,
        })
        .expect("said so");
    assert_eq!(dropped, 2);
}

// ---- the round trip --------------------------------------------------------

/// **The property worth having**: export → import → export is stable.
///
/// It covers the mapping table, the armour stripping and the reading
/// reconstruction in one assertion — and it is run into a *fresh* library, which
/// is the version that can actually fail. Re-importing into the library the file
/// came from proves only that the matcher found what it just wrote.
#[tokio::test]
async fn export_import_export_is_stable() {
    let (tmp_a, a) = common::engine().await;
    a.import_goodreads(&recorded("library-export.csv"), creating())
        .await
        .unwrap();
    let (first, _) = a.export_goodreads().await.unwrap();

    let (_tmp_b, b) = common::engine().await;
    let path = tmp_csv(tmp_a.path(), "round-trip.csv", &first);
    b.import_goodreads(&path, creating()).await.unwrap();
    let (second, _) = b.export_goodreads().await.unwrap();

    assert_eq!(first, second, "the round trip moved something");
    assert!(
        first.contains(r#""=""9781455563937""""#),
        "armoured:\n{first}"
    );
    assert!(
        first.contains("2019/03/12"),
        "the reading came back:\n{first}"
    );
}

/// The same property at the corpus's size, over the generated fixture: forty
/// rows, every shelf, ratings across the range. Five hand-written rows can be
/// five that happen to work.
#[tokio::test]
async fn the_generated_library_round_trips_and_re_imports_idempotently() {
    let (tmp, a) = common::engine().await;
    let report = a
        .import_goodreads(&generated(), creating())
        .await
        .expect("imports");
    assert_eq!(report.rows, 40);
    assert!(report.unmatched.is_empty());

    let before = snapshot(&a).await;
    let again = a.import_goodreads(&generated(), creating()).await.unwrap();
    assert_eq!(again.created(), 0);
    assert_eq!(snapshot(&a).await, before, "a second pass moved something");

    let (first, _) = a.export_goodreads().await.unwrap();
    let (_tmp_b, b) = common::engine().await;
    let path = tmp_csv(tmp.path(), "corpus-round-trip.csv", &first);
    b.import_goodreads(&path, creating()).await.unwrap();
    let (second, _) = b.export_goodreads().await.unwrap();
    assert_eq!(first, second);
}

// ---- helpers ---------------------------------------------------------------

async fn book(engine: &Engine, selector: &str) -> Book {
    let mut hits = engine.resolve_books(selector).await.expect("resolve");
    assert_eq!(hits.len(), 1, "{selector} resolved to {} books", hits.len());
    hits.remove(0)
}

/// Everything a second import could plausibly disturb, as one comparable value.
async fn snapshot(engine: &Engine) -> Vec<String> {
    let mut out = Vec::new();
    for b in engine
        .storage()
        .list_books(1000, readingbuddy::BookSort::Title)
        .await
        .unwrap()
    {
        let id = b.id.unwrap();
        let readings = engine.storage().list_readings(id).await.unwrap();
        let tags = engine.storage().book_tags(id).await.unwrap();
        let mut notes: Vec<String> = engine
            .list_notes(Some(id))
            .await
            .unwrap()
            .into_iter()
            .map(|n| format!("{}:{}", n.kind, n.title))
            .collect();
        notes.sort();
        out.push(format!(
            "{}|{:?}|{:?}|{:?}|{notes:?}",
            b.display_title(),
            b.any_isbn(),
            readings
                .iter()
                .map(|r| (r.status.clone(), r.finished_at))
                .collect::<Vec<_>>(),
            tags.iter().map(|t| t.tag.clone()).collect::<Vec<_>>(),
        ));
    }
    out.sort();
    out
}
