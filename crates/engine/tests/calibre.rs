//! Calibre, driven through fake binaries.
//!
//! Nothing here needs calibre installed, and that is the point: the CI runners
//! do not have it, and a feature that is only exercised on the developer's
//! laptop is a feature with no tests. What is faked is the *program*; everything
//! else — process spawn, exit codes, stdout, argv, JSON parsing, matching,
//! upserting, covers, `external_ids` — is the real path.
//!
//! Two things are deliberately **not** faked with a `PATH` edit, which is the
//! obvious way to do this:
//!
//! * `std::env::set_var` is `unsafe` in edition 2024 and is a data race with
//!   every other test in this binary, which cargo runs on threads.
//! * `EngineConfig::calibre_bin_dir` is searched before `PATH`, so pointing at a
//!   directory achieves the same thing with no global state at all.
//!
//! The `PATH` half — that detection finds a tool the ordinary way — is covered
//! from outside the process in `crates/cli/tests/cli.rs`, where the environment
//! belongs to a child and mutating it is safe.
//!
//! Unix-only: the fakes are shell scripts. The workspace is already unix-only
//! (`/Volumes`, `/run/media`), so this costs no coverage.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use readingbuddy::calibre::{CalibreMatch, ImportOptions};
use readingbuddy::{Book, EngineError};

mod common;

const LIBRARY_JSON: &str = include_str!("fixtures/calibre/recorded/library.json");

/// A directory holding fake `ebook-convert` / `calibredb` scripts.
struct Fake {
    dir: tempfile::TempDir,
}

impl Fake {
    fn new() -> Fake {
        Fake {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }

    fn path(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    /// Write an executable `/bin/sh` script under `name`.
    fn tool(&self, name: &str, body: &str) -> &Fake {
        use std::os::unix::fs::PermissionsExt;
        let p = self.dir.path().join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).expect("write fake");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        self
    }

    /// A `calibredb` that records its argv and prints `json`.
    fn calibredb(&self, json: &str) -> &Fake {
        let out = self.dir.path().join("argv.txt");
        self.tool(
            "calibredb",
            &format!(
                "printf '%s\\n' \"$@\" > {argv}\ncat <<'RB_EOF'\n{json}\nRB_EOF",
                argv = out.display()
            ),
        )
    }

    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.txt"))
            .expect("the fake ran")
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// A calibre library directory: `metadata.db` is the marker `library_root`
/// insists on, and the cover/format files the JSON names.
fn library(root: &Path, covers: &[(&str, &[u8])]) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("metadata.db"), b"not really sqlite").unwrap();
    for (rel, bytes) in covers {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
    }
    root.to_path_buf()
}

/// Rewrite the recorded fixture's absolute paths to point inside `root`, so the
/// cover copy and the `partial_md5` link run against files that exist.
fn json_rooted(root: &Path) -> String {
    LIBRARY_JSON.replace("/books/calibre", &root.display().to_string())
}

// ---- tier (i) ---------------------------------------------------------------

/// The conversion path end to end: argv reaches the program, a written output is
/// success, and a program that exits non-zero is a typed error carrying what it
/// said.
#[tokio::test]
async fn a_conversion_runs_the_tool_and_is_judged_by_what_it_wrote() {
    let fake = Fake::new();
    let work = tempfile::tempdir().unwrap();
    let input = work.path().join("in.epub");
    let output = work.path().join("out.azw3");
    std::fs::write(&input, b"epub").unwrap();

    // Writes its second argument, which is what `ebook-convert IN OUT` means.
    fake.tool("ebook-convert", "cp \"$1\" \"$2\"");
    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    assert!(engine.calibre().can_convert());

    let written = engine.convert_ebook(&input, &output, false).await.unwrap();
    assert_eq!(written, output);
    assert!(output.is_file());

    // …and it refuses to do it again over the top.
    let e = engine
        .convert_ebook(&input, &output, false)
        .await
        .unwrap_err();
    assert!(
        matches!(&e, EngineError::InvalidInput(m) if m.contains("already exists")),
        "{e:?}"
    );
    // Told to, it does.
    engine.convert_ebook(&input, &output, true).await.unwrap();
}

/// Calibre failing is not readingbuddy failing, and the message has to survive.
#[tokio::test]
async fn a_failing_conversion_is_typed_and_keeps_calibres_last_words() {
    let fake = Fake::new();
    let work = tempfile::tempdir().unwrap();
    let input = work.path().join("in.epub");
    std::fs::write(&input, b"epub").unwrap();
    fake.tool(
        "ebook-convert",
        "echo 'Traceback (most recent call last):' >&2\necho 'ValueError: no such format' >&2\nexit 1",
    );

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let e = engine
        .convert_ebook(&input, &work.path().join("out.xyz"), false)
        .await
        .unwrap_err();
    match &e {
        EngineError::Calibre { tool, message } => {
            assert_eq!(tool, "ebook-convert");
            assert!(message.contains("no such format"), "{message}");
        }
        other => panic!("expected a typed calibre failure, got {other:?}"),
    }

    // A tool that exits 0 and writes nothing is still a failure — an exit code
    // is a claim, and the file is the evidence.
    fake.tool("ebook-convert", "exit 0");
    let (_tmp2, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let e = engine
        .convert_ebook(&input, &work.path().join("nothing.azw3"), false)
        .await
        .unwrap_err();
    assert!(matches!(e, EngineError::Calibre { .. }), "{e:?}");
}

// The absent-calibre path is asserted in `calibre.rs`'s own tests
// (`an_absent_calibre_is_a_named_error_and_never_a_panic`) and deliberately not
// here: detection falls through to the real `PATH`, so on a developer machine
// with calibre installed a test that claimed to observe "absent" would be
// observing a real install and passing for the wrong reason. The facade methods
// are one-line delegations to the two functions that test covers.

// ---- tier (ii) --------------------------------------------------------------

/// The whole import, once: argv, parse, create, cover, tags, `external_ids`.
#[tokio::test]
async fn a_library_import_builds_the_command_line_and_lands_the_books() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(
        &work.path().join("lib"),
        &[
            ("Min Jin Lee/Pachinko (1)/cover.jpg", b"jpegbytes"),
            (
                "Min Jin Lee/Pachinko (1)/Pachinko - Min Jin Lee.epub",
                b"epub-one",
            ),
            (
                "Min Jin Lee/Pachinko (1)/Pachinko - Min Jin Lee.azw3",
                b"azw3-one",
            ),
            (
                "Emily St. John Mandel/Station Eleven (2)/Station Eleven - Emily St. John Mandel.epub",
                b"epub-two",
            ),
        ],
    );
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let report = engine
        .import_calibre_library(&ImportOptions {
            library: Some(lib.clone()),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(
        fake.argv(),
        [
            "--with-library",
            lib.to_str().unwrap(),
            "list",
            "--for-machine",
            "--fields",
            "all"
        ],
        "the command line is the whole interface to another program"
    );

    assert_eq!(report.rows, 3);
    assert_eq!(report.books.len(), 3);
    assert_eq!(report.created(), 3);
    assert!(report.unmatched.is_empty());

    let pachinko = report.books.iter().find(|b| b.title == "Pachinko").unwrap();
    assert_eq!(pachinko.matched_by, CalibreMatch::New);
    assert_eq!(pachinko.tags_added, 2);
    assert!(pachinko.cover, "calibre's cover filled an empty slot");
    assert_eq!(pachinko.files_linked, 2, "every format, not just the epub");

    let book = engine
        .storage()
        .get_book(pachinko.book_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(book.authors, ["Min Jin Lee", "Deborah Smith"]);
    assert_eq!(book.isbn_13.as_deref(), Some("9781455563937"));
    assert_eq!(book.publish_year, Some(2017));
    assert_eq!(book.language.as_deref(), Some("en"));
    // Series used to be dropped outright for want of a column (migration
    // `0013`). Calibre is the one origin here that curates it by hand.
    assert_eq!(book.series.as_deref(), Some("Saga"));
    assert_eq!(book.series_index, Some(1.0));
    // …and its tags are still **shelves**, not subjects: they go to
    // `book_tags`, which is a different table with a different meaning.
    assert!(
        book.subjects.is_empty(),
        "a calibre tag is a minted shelf, never a bibliographic subject"
    );
    // The cover was copied into our own images dir, under a name that cannot
    // collide — every cover in a calibre library is called `cover.jpg`.
    let cover = PathBuf::from(book.cover_path.expect("a cover path"));
    assert!(cover.is_file());
    assert!(cover.starts_with(engine.images_dir()));
    assert_eq!(std::fs::read(&cover).unwrap(), b"jpegbytes");

    // Provenance: calibre's uuid, and the shelves as it wrote them.
    assert_eq!(
        engine
            .storage()
            .book_for_external_id("calibre", "c47437a8-d73c-4719-838d-ff362479bf63")
            .await
            .unwrap(),
        pachinko.book_id
    );
    let tags = engine.storage().book_tags(book.id.unwrap()).await.unwrap();
    assert_eq!(
        tags.iter().map(|t| t.tag.as_str()).collect::<Vec<_>>(),
        ["fiction", "korean-lit"]
    );

    // Field provenance (item 29), beside the row provenance above: every column
    // this import wrote is calibre's, and `page_count` — which `calibredb list`
    // does not carry at all — is claimed by nobody rather than by calibre.
    let claimed = engine
        .storage()
        .field_provenance(book.id.unwrap())
        .await
        .unwrap();
    assert!(
        claimed.iter().all(|f| f.source == "calibre"),
        "nothing else has written to this book: {claimed:?}"
    );
    let fields: Vec<&str> = claimed.iter().map(|f| f.field.as_str()).collect();
    assert!(fields.contains(&"title") && fields.contains(&"isbn_13"));
    assert!(
        !fields.contains(&"page_count"),
        "calibre said nothing about the page count, so it claims nothing: {fields:?}"
    );

    // The undated book's sentinel `pubdate` did not become the year 101.
    let se = report
        .books
        .iter()
        .find(|b| b.title == "Station Eleven")
        .unwrap();
    let se_book = engine
        .storage()
        .get_book(se.book_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(se_book.publish_year, None);
}

/// Re-running the import must find the same books, not make second copies of
/// them — and the uuid is what makes that true for a book with no ISBN.
#[tokio::test]
async fn importing_the_same_library_twice_changes_nothing() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(
        &work.path().join("lib"),
        &[
            ("Min Jin Lee/Pachinko (1)/cover.jpg", b"jpegbytes"),
            (
                "Min Jin Lee/Pachinko (1)/Pachinko - Min Jin Lee.epub",
                b"epub-one",
            ),
        ],
    );
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let opts = ImportOptions {
        library: Some(lib),
        ..Default::default()
    };
    let first = engine.import_calibre_library(&opts).await.unwrap();
    assert_eq!(first.created(), 3);

    let again = engine.import_calibre_library(&opts).await.unwrap();
    assert_eq!(again.created(), 0, "a second run creates nothing");
    assert!(
        again
            .books
            .iter()
            .all(|b| b.matched_by == CalibreMatch::Uuid),
        "every row found itself by uuid: {:?}",
        again.books.iter().map(|b| b.matched_by).collect::<Vec<_>>()
    );
    // Every count on the report is what the import *changed*. A second run that
    // announced a tag, a cover and a file against every book would be reporting
    // work it did not do — the same rule the Goodreads import applies to a
    // rating it did not rewrite.
    for b in &again.books {
        assert_eq!(b.tags_added, 0, "{}: tags", b.title);
        assert!(!b.cover, "{}: cover", b.title);
        assert_eq!(b.files_linked, 0, "{}: files", b.title);
    }
    // …and the first run did report all three, so the zeroes above are the
    // second run being quiet rather than the fields never being set.
    let first_pachinko = first.books.iter().find(|b| b.title == "Pachinko").unwrap();
    assert_eq!(first_pachinko.tags_added, 2);
    assert!(first_pachinko.cover);
    assert_eq!(first_pachinko.files_linked, 1);
    assert_eq!(
        engine
            .storage()
            .list_books(&readingbuddy::BookQuery::new(
                100,
                readingbuddy::BookSort::Title
            ))
            .await
            .unwrap()
            .len(),
        3
    );
}

/// A book we already have is matched, not duplicated — and the rung that found
/// it is reported, because "matched by title" and "matched by isbn" are
/// different amounts of confidence.
#[tokio::test]
async fn an_existing_book_is_matched_by_isbn_and_never_duplicated() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(&work.path().join("lib"), &[]);
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    // Same ISBN, a different title entirely — so only the ISBN rung can find it.
    let existing = engine
        .save_book(&Book {
            title: Some("Pachinko (Head of Zeus paperback)".into()),
            isbn_13: Some("9781455563937".into()),
            page_count: Some(490),
            ..Default::default()
        })
        .await
        .unwrap();

    let report = engine
        .import_calibre_library(&ImportOptions {
            library: Some(lib),
            ..Default::default()
        })
        .await
        .unwrap();
    let matched = report
        .books
        .iter()
        .find(|b| b.book_id == existing.id)
        .unwrap();
    assert_eq!(matched.matched_by, CalibreMatch::Isbn);

    let after = engine
        .storage()
        .get_book(existing.id.unwrap())
        .await
        .unwrap()
        .unwrap();
    // The **provider no-clobber merge**, not the device's straight assignment:
    // a `calibredb list` row carries no page count at all, so assigning it
    // straight would blank a field calibre has no opinion about.
    assert_eq!(
        after.page_count,
        Some(490),
        "calibre must not blank what it does not carry"
    );
    // And it filled what was empty.
    assert_eq!(after.publisher.as_deref(), Some("Grand Central Publishing"));
}

/// A near-miss is a decision, not a dead end — and `--new` is the escape hatch,
/// exactly as `ko pull --new` and `goodreads import --new` are.
#[tokio::test]
async fn a_near_miss_is_offered_rather_than_duplicated_unless_asked() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(&work.path().join("lib"), &[]);
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    // Close enough to be suspicious, far enough not to auto-match (0.845, and
    // the band is 0.60..0.85), and with no ISBN so the title rung is the one
    // that decides. A subtitle the library carries and the origin does not is
    // the shape that used to become a duplicate in silence.
    engine
        .save_book(&Book {
            title: Some("Station Eleven: A Novel of Survival and the Travelling Symphony".into()),
            // Stored the way the origin spelled it; the row the user reads must
            // not be.
            authors: vec!["Mandel, Emily St. John".into()],
            publish_year: Some(2014),
            ..Default::default()
        })
        .await
        .unwrap();

    let opts = ImportOptions {
        library: Some(lib),
        ..Default::default()
    };
    let report = engine.import_calibre_library(&opts).await.unwrap();
    let offered = report
        .unmatched
        .iter()
        .find(|u| u.title == "Station Eleven")
        .expect("the near-miss is reported rather than swallowed");
    assert!(
        !offered.candidates.is_empty(),
        "and it says what it probably is"
    );
    // Item 36: and says it in full. Two editions of one title tie on the title
    // alone, which is the whole reason a chooser exists.
    assert_eq!(
        offered.candidates[0].authors_display,
        ["Emily St. John Mandel"]
    );
    assert_eq!(offered.candidates[0].publish_year, Some(2014));
    assert!(offered.uuid.is_some());

    // Told to, it creates it anyway.
    let forced = engine
        .import_calibre_library(&ImportOptions {
            create_ambiguous: true,
            ..opts
        })
        .await
        .unwrap();
    assert!(forced.unmatched.is_empty());
    assert!(
        forced
            .books
            .iter()
            .any(|b| b.title == "Station Eleven" && b.matched_by == CalibreMatch::New)
    );
}

/// `--dry-run` answers the same questions and writes nothing — including no
/// covers, which is the half that is easy to get wrong because it happens on
/// the filesystem rather than in the database.
#[tokio::test]
async fn a_dry_run_reports_the_same_thing_and_writes_nothing() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(
        &work.path().join("lib"),
        &[("Min Jin Lee/Pachinko (1)/cover.jpg", b"jpegbytes")],
    );
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let report = engine
        .import_calibre_library(&ImportOptions {
            library: Some(lib),
            dry_run: true,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(report.dry_run);
    assert_eq!(report.created(), 3);
    assert!(report.books.iter().all(|b| b.book_id.is_none()));
    assert_eq!(
        engine
            .storage()
            .list_books(&readingbuddy::BookQuery::new(
                100,
                readingbuddy::BookSort::Title
            ))
            .await
            .unwrap()
            .len(),
        0
    );
    let images = engine
        .images_dir()
        .read_dir()
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(images, 0, "a dry run must not copy covers either");
}

/// The guard that keeps a mistyped `--library` from creating a calibre library
/// on the user's disk. Verified against calibre 7.26, which does exactly that:
/// it writes `metadata.db`, reports `[]`, and exits 0.
#[tokio::test]
async fn a_mistyped_library_path_is_refused_before_calibredb_is_run() {
    let work = tempfile::tempdir().unwrap();
    let typo = work.path().join("Calbire Library");
    std::fs::create_dir_all(&typo).unwrap();
    let fake = Fake::new();
    fake.calibredb("[]");

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let e = engine
        .import_calibre_library(&ImportOptions {
            library: Some(typo),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(
        matches!(&e, EngineError::InvalidInput(m) if m.contains("metadata.db")),
        "{e:?}"
    );
    assert!(
        !fake.dir.path().join("argv.txt").exists(),
        "the refusal must come before the program runs, or calibre has already \
         created the library the refusal is about"
    );
}

/// The middle rung of dedup level 3, observed from both ends: an imported
/// calibre file is identified by `partial_md5`, so a book later matched on that
/// hash is *this* book — which is what makes a KOReader sidecar off the device
/// land here instead of guessing at a title.
#[tokio::test]
async fn calibres_files_are_identified_so_the_device_can_find_them_later() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(
        &work.path().join("lib"),
        &[(
            "Min Jin Lee/Pachinko (1)/Pachinko - Min Jin Lee.epub",
            b"the bytes koreader would open",
        )],
    );
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let report = engine
        .import_calibre_library(&ImportOptions {
            library: Some(lib.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    let pachinko = report.books.iter().find(|b| b.title == "Pachinko").unwrap();
    assert_eq!(pachinko.files_linked, 1, "the azw3 is not on disk here");

    let md5 = readingbuddy::partial_md5(
        &lib.join("Min Jin Lee/Pachinko (1)/Pachinko - Min Jin Lee.epub"),
    )
    .unwrap();
    let found = engine
        .storage()
        .find_book_by_partial_md5(&md5)
        .await
        .unwrap()
        .expect("the file is identified");
    assert_eq!(found.id, pachinko.book_id);
}

/// Rows the import cannot fully stand behind are reported, never silently
/// dropped or silently duplicated.
#[tokio::test]
async fn a_titleless_row_is_skipped_and_an_id_less_row_says_it_will_recur() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(&work.path().join("lib"), &[]);
    let fake = Fake::new();
    fake.calibredb(
        r#"[
            {"id": 7, "authors": "Nobody"},
            {"id": 8, "title": "No uuid here"}
        ]"#,
    );

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let report = engine
        .import_calibre_library(&ImportOptions {
            library: Some(lib),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(report.rows, 2);
    assert_eq!(report.books.len(), 1, "the titleless row imported nothing");
    let said: Vec<String> = report.warnings.iter().map(|w| w.to_string()).collect();
    assert!(
        said.iter()
            .any(|w| w.contains("calibre #7") && w.contains("nothing to match on")),
        "{said:?}"
    );
    assert!(
        said.iter()
            .any(|w| w.contains("calibre #8") && w.contains("again on a second run")),
        "{said:?}"
    );
}

// ---- one row at a time ------------------------------------------------------

/// `ImportOptions::only` is what lets a frontend show a shelf and import one
/// line of it. The filter has to reach the *listing*, not the report: a user who
/// imported one book of three must not be told the import read three.
#[tokio::test]
async fn only_imports_the_row_it_was_asked_for_and_counts_only_that_row() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(&work.path().join("lib"), &[]);
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let report = engine
        .import_calibre_library(&ImportOptions {
            library: Some(lib),
            only: vec![2],
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(report.rows, 1, "rows is what this import considered");
    assert_eq!(report.books.len(), 1);
    assert_eq!(report.books[0].calibre_id, 2);
    assert_eq!(report.books[0].title, "Station Eleven");
    let shelf = engine
        .list_books(&readingbuddy::BookQuery::new(
            100,
            readingbuddy::BookSort::Title,
        ))
        .await
        .unwrap();
    assert_eq!(shelf.len(), 1, "the other two rows were left alone");
}

/// An empty `only` is every row — the default, and what keeps every caller that
/// predates the field unchanged.
#[tokio::test]
async fn an_empty_only_is_the_whole_library() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(&work.path().join("lib"), &[]);
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    let report = engine
        .import_calibre_library(&ImportOptions {
            library: Some(lib),
            only: vec![],
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(report.rows, 3);
    assert_eq!(report.books.len(), 3);
}

/// Every report line names the calibre row it came from, in a dry run too — the
/// dry run being the one a shelf screen renders, where a line that cannot be
/// tied to a row is a line the user cannot act on.
#[tokio::test]
async fn every_report_line_names_its_calibre_row() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(&work.path().join("lib"), &[]);
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    for dry_run in [true, false] {
        let report = engine
            .import_calibre_library(&ImportOptions {
                library: Some(lib.clone()),
                dry_run,
                ..Default::default()
            })
            .await
            .unwrap();
        let mut ids: Vec<i64> = report.books.iter().map(|b| b.calibre_id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2, 3], "dry_run = {dry_run}");
    }
}

// ---- linking by hand --------------------------------------------------------

/// The escape hatch the shelf screen's `l` needs: say once that a calibre book
/// is a book we already have, and the next import matches it by uuid instead of
/// offering the same guess again.
#[tokio::test]
async fn linking_a_calibre_book_by_hand_is_matched_by_uuid_from_then_on() {
    let work = tempfile::tempdir().unwrap();
    let lib = library(&work.path().join("lib"), &[]);
    let fake = Fake::new();
    fake.calibredb(&json_rooted(&work.path().join("lib")));

    let (_tmp, engine) = common::engine_with_calibre(Some(fake.path())).await;
    // A book whose title is nothing like calibre's, so no rung but the link can
    // possibly find it.
    let mine = engine
        .save_book(&Book {
            title: Some("A Novel About Snow".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    engine
        .link_calibre_book("0b4a2d15-6f31-4c88-9a2e-1f5b7c9d0e33", mine.id.unwrap())
        .await
        .unwrap();

    let report = engine
        .import_calibre_library(&ImportOptions {
            library: Some(lib),
            only: vec![2],
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(report.books[0].matched_by, CalibreMatch::Uuid);
    assert_eq!(report.books[0].book_id, mine.id);
    assert_eq!(
        engine
            .list_books(&readingbuddy::BookQuery::new(
                100,
                readingbuddy::BookSort::Title
            ))
            .await
            .unwrap()
            .len(),
        1,
        "linking then importing must not make a second copy"
    );
}

/// Repointing is the whole reason `external_ids` upserts rather than inserts: a
/// link made by hand and then corrected has to move, or the correction is the
/// one write that silently does nothing.
#[tokio::test]
async fn a_hand_link_can_be_corrected() {
    let (_tmp, engine) = common::engine_with_calibre(None).await;
    let first = engine.save_book(&common::book("Wrong One")).await.unwrap();
    let second = engine.save_book(&common::book("Right One")).await.unwrap();

    engine
        .link_calibre_book("uuid-abc", first.id.unwrap())
        .await
        .unwrap();
    engine
        .link_calibre_book("uuid-abc", second.id.unwrap())
        .await
        .unwrap();

    assert_eq!(
        engine
            .storage()
            .book_for_external_id("calibre", "uuid-abc")
            .await
            .unwrap(),
        second.id
    );
}

/// A candidate list can name a book another pane has since deleted, so the
/// answer is typed rather than a foreign-key error naming a constraint.
#[tokio::test]
async fn linking_to_a_book_that_is_gone_says_so() {
    let (_tmp, engine) = common::engine_with_calibre(None).await;
    let err = engine
        .link_calibre_book("uuid-abc", 9999)
        .await
        .expect_err("there is no book 9999");
    assert!(matches!(err, EngineError::NotFound(_)), "{err:?}");
}

/// calibre ids are per-library and reused after a delete, so a row with no uuid
/// has nothing durable to link by and is refused rather than linked by its id.
#[tokio::test]
async fn a_calibre_book_with_no_uuid_cannot_be_linked() {
    let (_tmp, engine) = common::engine_with_calibre(None).await;
    let mine = engine.save_book(&common::book("Pachinko")).await.unwrap();
    let err = engine
        .link_calibre_book("   ", mine.id.unwrap())
        .await
        .expect_err("an empty uuid is not an identity");
    assert!(matches!(err, EngineError::InvalidInput(_)), "{err:?}");
}
