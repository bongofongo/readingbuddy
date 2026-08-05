//! Owned files and the three dedup levels (item 12, migration `0010`).
//!
//! The levels are asserted **separately and by name**, because conflating them
//! is the failure `docs/decisions.md` calls out: level 1 is bytes, level 2 is a
//! book with more than one file, level 3 is a file whose book is unknown. A
//! suite that only ever imports the same epub twice tests the first and claims
//! all three.
//!
//! Offline throughout. Every book that gets created here is created from an
//! **ISBN-less** epub or from a filename, because `import_epub` looks a found
//! ISBN up through the providers and this suite makes no network calls; the
//! ISBN *matching* test seeds the book first, so the match happens before
//! anything would be looked up.

use std::path::{Path, PathBuf};

use readingbuddy::{Book, Engine, FileImportOptions, FileMatch, FileOutcome};

mod common;
use common::{engine, write_isbnless_epub};

/// The committed epub whose OPF carries a real ISBN.
fn isbn_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/koreader/synthetic/Gen-Isbn-Match.epub")
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}

/// Every file under the content store, so a test can say "nothing was written"
/// and mean it.
fn stored_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries {
        let p = entry.unwrap().path();
        if p.is_dir() {
            out.extend(stored_files(&p));
        } else {
            out.push(p);
        }
    }
    out.sort();
    out
}

fn files_dir(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("database/files")
}

async fn library_size(engine: &Engine) -> usize {
    engine
        .storage()
        .list_books(1000, readingbuddy::BookSort::Title)
        .await
        .unwrap()
        .len()
}

// ---- the store itself -------------------------------------------------------

/// The claim the whole item rests on: after an import the bytes are ours, at an
/// address derived from their own content, with the original filename kept as a
/// column rather than as the path.
#[tokio::test]
async fn an_imported_file_is_owned_at_its_own_content_address() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("Some Downloaded Name (1).epub");
    write_isbnless_epub(&src, "A Book With No ISBN");

    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();
    assert_eq!(report.outcome, FileOutcome::Stored);
    assert!(report.created_book);
    let book_id = report.book_id.unwrap();

    let files = engine.book_files(book_id).await.unwrap();
    assert_eq!(files.len(), 1);
    let file = &files[0];
    assert_eq!(file.format, "epub");
    assert_eq!(
        file.original_name.as_deref(),
        Some("Some Downloaded Name (1).epub"),
        "the name it arrived under is provenance, and is kept"
    );
    assert_eq!(file.size, std::fs::metadata(&src).unwrap().len() as i64);

    let stored = engine.file_path(file);
    assert_eq!(report.stored_path.as_ref(), Some(&stored));
    assert!(stored.starts_with(files_dir(&tmp).join(&file.sha256[..2])));
    assert_eq!(
        stored.file_name().unwrap().to_string_lossy(),
        format!("{}.epub", file.sha256)
    );
    assert_eq!(
        std::fs::read(&stored).unwrap(),
        std::fs::read(&src).unwrap(),
        "the copy is the file, byte for byte"
    );
    // The source is untouched — importing is copying, not moving. A user's
    // library directory is not ours to rearrange.
    assert!(src.is_file());
}

/// The file also arrives carrying KOReader's own identity for it, so a sidecar
/// that shows up later matches by `md5` rather than guessing at a title. Same
/// hook `import_epub` has, and the same reason.
#[tokio::test]
async fn an_imported_file_records_the_identity_the_device_will_use() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("offline.epub");
    write_isbnless_epub(&src, "A Book With No ISBN");

    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();
    let md5 = readingbuddy::partial_md5(&src).unwrap();
    let linked = engine
        .storage()
        .find_book_by_partial_md5(&md5)
        .await
        .unwrap()
        .expect("the file's device identity must be recorded");
    assert_eq!(linked.id, report.book_id);
}

// ---- level 1: same bytes ----------------------------------------------------

/// Identical bytes are one file, whatever they are called and however many
/// times they are imported — and the second import says so rather than
/// reporting success.
#[tokio::test]
async fn level_1_the_same_bytes_are_owned_once() {
    let (tmp, engine) = engine().await;
    let first = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&first, "A Book With No ISBN");
    let second = tmp.path().join("a-book-with-no-isbn (copy).epub");
    std::fs::copy(&first, &second).unwrap();

    let a = engine
        .import_file(&first, FileImportOptions::default())
        .await
        .unwrap();
    let b = engine
        .import_file(&second, FileImportOptions::default())
        .await
        .unwrap();

    assert_eq!(b.outcome, FileOutcome::AlreadyOwned);
    assert_eq!(b.matched_by, Some(FileMatch::Sha256));
    assert_eq!(b.book_id, a.book_id);
    assert!(!b.created_book);
    assert_eq!(a.sha256, b.sha256);

    assert_eq!(
        library_size(&engine).await,
        1,
        "the second copy must not become a second book"
    );
    assert_eq!(
        engine.book_files(a.book_id.unwrap()).await.unwrap().len(),
        1
    );
    assert_eq!(stored_files(&files_dir(&tmp)).len(), 1);
}

/// Level 1 outranks a caller's own instruction: naming a book does not move
/// content that is already owned. Two books holding one file means the *books*
/// are duplicates, and the move for that is `merge_books`.
#[tokio::test]
async fn level_1_holds_even_when_the_caller_names_another_book() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&src, "A Book With No ISBN");
    let first = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();

    let other = engine
        .save_book(&Book {
            title: Some("A Completely Different Book".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let report = engine
        .add_file_to_book(other.id.unwrap(), &src)
        .await
        .unwrap();

    assert_eq!(report.outcome, FileOutcome::AlreadyOwned);
    assert_eq!(report.book_id, first.book_id);
    assert!(
        engine
            .book_files(other.id.unwrap())
            .await
            .unwrap()
            .is_empty()
    );
}

// ---- level 2: one book, several files ---------------------------------------

/// epub + azw3 = two files, one book. Nothing about the second file's *content*
/// is compared: level 2 is `book_id` and nothing else.
#[tokio::test]
async fn level_2_two_formats_are_two_files_of_one_book() {
    let (tmp, engine) = engine().await;
    let epub = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&epub, "A Book With No ISBN");
    let first = engine
        .import_file(&epub, FileImportOptions::default())
        .await
        .unwrap();
    let book_id = first.book_id.unwrap();

    let azw3 = tmp.path().join("A Book With No ISBN.azw3");
    write(&azw3, b"a different container of the same book");
    let second = engine.add_file_to_book(book_id, &azw3).await.unwrap();

    assert_eq!(second.outcome, FileOutcome::Stored);
    assert_eq!(second.book_id, Some(book_id));
    assert_ne!(first.sha256, second.sha256);
    assert_eq!(library_size(&engine).await, 1);

    let mut formats: Vec<String> = engine
        .book_files(book_id)
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.format)
        .collect();
    formats.sort();
    assert_eq!(formats, ["azw3", "epub"]);
    assert_eq!(stored_files(&files_dir(&tmp)).len(), 2);
}

/// The same thing reached by import rather than by an explicit attach: a second
/// format whose *title* matches lands on the book that already exists, not on a
/// new one.
#[tokio::test]
async fn level_2_is_reached_by_the_title_rung_too() {
    let (tmp, engine) = engine().await;
    let epub = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&epub, "A Book With No ISBN");
    let first = engine
        .import_file(&epub, FileImportOptions::default())
        .await
        .unwrap();

    let azw3 = tmp.path().join("A Book With No ISBN.azw3");
    write(&azw3, b"the azw3 of the same book");
    let second = engine
        .import_file(&azw3, FileImportOptions::default())
        .await
        .unwrap();

    assert_eq!(second.matched_by, Some(FileMatch::Title));
    assert_eq!(second.book_id, first.book_id);
    assert!(!second.created_book);
    assert_eq!(library_size(&engine).await, 1);
}

// ---- level 3: same book, unknown file ---------------------------------------

/// The ISBN rung. The book is seeded first, so the match happens against the
/// library and nothing is looked up over the network.
#[tokio::test]
async fn level_3_matches_on_isbn_before_anything_else() {
    let (_tmp, engine) = engine().await;
    let existing = engine
        .save_book(&Book {
            // Deliberately nothing like the epub's own title, so only the ISBN
            // can be what matched.
            title: Some("Zzz Nothing Like The Title".into()),
            isbn_13: Some("9780316769488".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    let report = engine
        .import_file(&isbn_fixture(), FileImportOptions::default())
        .await
        .unwrap();
    assert_eq!(report.matched_by, Some(FileMatch::Isbn));
    assert_eq!(report.book_id, existing.id);
    assert_eq!(library_size(&engine).await, 1);
}

/// The `partial_md5` rung — the middle one, and the only rung that works for a
/// file whose format we cannot read and whose name tells us nothing. It reads
/// the *existing* `device_books` mapping rather than a column of its own.
#[tokio::test]
async fn level_3_matches_on_the_device_identity_next() {
    let (tmp, engine) = engine().await;
    let file = tmp.path().join("B00KF1L3.azw3");
    write(&file, b"an ebook the device already knows");

    let existing = engine
        .save_book(&Book {
            title: Some("Zzz Nothing Like The Filename".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let md5 = readingbuddy::partial_md5(&file).unwrap();
    engine
        .storage()
        .link_device_book(
            &md5,
            existing.id.unwrap(),
            readingbuddy::storage::LinkedBy::Manual,
        )
        .await
        .unwrap();

    let report = engine
        .import_file(&file, FileImportOptions::default())
        .await
        .unwrap();
    assert_eq!(report.matched_by, Some(FileMatch::Md5));
    assert_eq!(report.book_id, existing.id);
    assert_eq!(library_size(&engine).await, 1);
}

/// The title rung is the **existing** matcher, band and all — the same scan a
/// sidecar and a Goodreads row get. A near miss is therefore a decision, not a
/// duplicate: nothing is created, nothing is copied, and the candidates come
/// back so the caller has somewhere to go.
#[tokio::test]
async fn level_3_refuses_to_create_over_a_candidate_and_writes_nothing() {
    let (tmp, engine) = engine().await;
    let near = engine
        .save_book(&Book {
            title: Some("Pachinko: A Novel of Korea and Japan".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let src = tmp.path().join("Pachinko.epub");
    write_isbnless_epub(&src, "Pachinko");

    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();
    assert_eq!(report.outcome, FileOutcome::Unmatched);
    assert_eq!(report.book_id, None);
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].book_id, near.id.unwrap());
    assert!(
        (0.60..0.85).contains(&report.candidates[0].score),
        "the pair has drifted out of the candidate band: {}",
        report.candidates[0].score
    );

    assert_eq!(library_size(&engine).await, 1, "nothing was created");
    assert!(
        stored_files(&files_dir(&tmp)).is_empty(),
        "a refusal must not leave the bytes behind either"
    );
    assert!(report.stored_path.is_none());
}

/// `new` is the escape hatch, with the same name and meaning `ko pull` gives
/// it: the user has looked at the candidates and said this really is another
/// book.
#[tokio::test]
async fn level_3_creates_when_the_caller_overrides_the_candidates() {
    let (tmp, engine) = engine().await;
    engine
        .save_book(&Book {
            title: Some("Pachinko: A Novel of Korea and Japan".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let src = tmp.path().join("Pachinko.epub");
    write_isbnless_epub(&src, "Pachinko");

    let report = engine
        .import_file(&src, FileImportOptions { new: true })
        .await
        .unwrap();
    assert_eq!(report.outcome, FileOutcome::Stored);
    assert!(report.created_book);
    assert_eq!(library_size(&engine).await, 2);
    assert_eq!(stored_files(&files_dir(&tmp)).len(), 1);
}

/// With nothing in the library there is nothing to refuse over, so a file the
/// matcher cannot place becomes a book. Its title is the epub's own.
#[tokio::test]
async fn an_unrecognised_file_becomes_a_book_when_there_is_nothing_to_confuse_it_with() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("whatever-the-download-called-it.epub");
    write_isbnless_epub(&src, "A Book With No ISBN");

    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();
    assert!(report.created_book);
    let book = engine
        .storage()
        .get_book(report.book_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(book.title.as_deref(), Some("A Book With No ISBN"));
}

/// A format with no metadata reader still gets owned. The filename is the only
/// name there is, and saying so beats refusing the file.
#[tokio::test]
async fn a_format_we_cannot_read_is_still_ours_to_keep() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("Long Walk to Freedom.azw3");
    write(&src, b"mobi-ish bytes we have no reader for");

    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();
    assert_eq!(report.outcome, FileOutcome::Stored);
    let book = engine
        .storage()
        .get_book(report.book_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(book.title.as_deref(), Some("Long Walk to Freedom"));
    assert_eq!(
        engine.book_files(book.id.unwrap()).await.unwrap()[0].format,
        "azw3"
    );
}

// ---- identify is read-only --------------------------------------------------

/// A frontend has to be able to ask before it commits — so `identify` copies no
/// bytes and writes no rows, and asking twice cannot change the answer.
#[tokio::test]
async fn identify_answers_without_writing_anything() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&src, "A Book With No ISBN");

    let id = engine.identify_file(&src).await.unwrap();
    assert!(id.matched.is_none());
    assert_eq!(id.format, "epub");
    assert_eq!(id.title.as_deref(), Some("A Book With No ISBN"));
    assert_eq!(id.sha256.len(), 64);
    assert_eq!(id.partial_md5.len(), 32);

    assert_eq!(library_size(&engine).await, 0);
    assert!(stored_files(&files_dir(&tmp)).is_empty());
    assert_eq!(engine.identify_file(&src).await.unwrap().sha256, id.sha256);
}

#[tokio::test]
async fn identifying_something_that_is_not_a_file_is_an_error_not_a_panic() {
    let (tmp, engine) = engine().await;
    let err = engine.identify_file(tmp.path()).await.unwrap_err();
    assert!(
        matches!(err, readingbuddy::EngineError::InvalidInput(_)),
        "{err:?}"
    );
}

// ---- the rest of the library has to keep up ---------------------------------

/// `book_files` cascades on `books`, so a merge that ignored it would *delete*
/// the rows with `src` and leave the bytes owned by nothing. The whole reason
/// the item touches `merge_books` at all.
#[tokio::test]
async fn merging_two_books_carries_their_files_across() {
    let (tmp, engine) = engine().await;
    let a = tmp.path().join("Pachinko.epub");
    write_isbnless_epub(&a, "Pachinko");
    // A name nothing in the library is close to, so it really does become a
    // second book — which is the duplicate the merge then folds back in.
    let b = tmp.path().join("Zzz An Unhelpfully Named Scan.azw3");
    write(&b, b"the other copy of the same book");

    let first = engine
        .import_file(&a, FileImportOptions::default())
        .await
        .unwrap();
    let second = engine
        .import_file(&b, FileImportOptions::default())
        .await
        .unwrap();
    let (src, dst) = (second.book_id.unwrap(), first.book_id.unwrap());
    assert_ne!(src, dst, "the fixture must produce two books to merge");

    let report = engine.merge_books(src, dst).await.unwrap();
    assert_eq!(report.files_moved, 1);

    let files = engine.book_files(dst).await.unwrap();
    assert_eq!(files.len(), 2, "both files are now the survivor's");
    for file in &files {
        assert!(
            engine.file_path(file).is_file(),
            "and the bytes are still there"
        );
    }
    assert_eq!(stored_files(&files_dir(&tmp)).len(), 2);
}

/// Deleting a book takes its files with it — rows by cascade, bytes by us. The
/// content address is global, so nothing else can be holding them.
#[tokio::test]
async fn deleting_a_book_removes_the_files_it_owned() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&src, "A Book With No ISBN");
    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();
    let stored = report.stored_path.clone().unwrap();
    assert!(stored.is_file());

    engine.delete_book(report.book_id.unwrap()).await.unwrap();

    assert!(!stored.exists(), "the bytes go with the book");
    assert!(stored_files(&files_dir(&tmp)).is_empty());
    assert!(
        engine
            .storage()
            .book_file(&report.sha256)
            .await
            .unwrap()
            .is_none()
    );
}

/// Giving a file up is exact, and doing it twice is a no-op rather than an
/// error — the same shape every other idempotent operation here has.
#[tokio::test]
async fn a_file_can_be_given_up_and_giving_it_up_twice_is_a_no_op() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&src, "A Book With No ISBN");
    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();

    assert!(engine.remove_file(&report.sha256).await.unwrap());
    assert!(!engine.remove_file(&report.sha256).await.unwrap());
    assert!(stored_files(&files_dir(&tmp)).is_empty());
    assert!(
        engine
            .book_files(report.book_id.unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    // The book itself stays — giving up a file is not giving up the book.
    assert_eq!(library_size(&engine).await, 1);
}

#[tokio::test]
async fn attaching_to_a_book_that_does_not_exist_is_an_error() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&src, "A Book With No ISBN");
    let err = engine.add_file_to_book(9_999, &src).await.unwrap_err();
    assert!(
        matches!(err, readingbuddy::EngineError::NotFound(_)),
        "{err:?}"
    );
    assert!(stored_files(&files_dir(&tmp)).is_empty());
}

// ---- the table of contents (item 32) ---------------------------------------

/// The committed epub whose whole point is its `toc.ncx`.
fn toc_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/koreader/synthetic/Gen-Toc-Chapters.epub")
}

/// **The chapter list is derived from the owned file, never stored**, and this
/// is the assertion that says so: attach a *better* file to the same book and
/// the answer changes, with no migration, no cache and nothing to invalidate.
///
/// A stored copy would still be reporting the first file's chapters here, and
/// nothing in the row would look wrong.
#[tokio::test]
async fn a_chapter_list_follows_the_file_the_book_owns() {
    let (tmp, engine) = engine().await;

    let plain = tmp.path().join("A Book With No ISBN.epub");
    write_isbnless_epub(&plain, "A Book With No ISBN");
    let report = engine
        .import_file(&plain, FileImportOptions::default())
        .await
        .unwrap();
    let book_id = report.book_id.unwrap();

    // An epub with no ncx: the file answers "no navigable table of contents",
    // which is an empty list and not an absent one.
    let first = engine.table_of_contents(book_id).await.unwrap().unwrap();
    assert!(first.entries.is_empty());
    assert_eq!(first.sha256, report.sha256);

    // The book is given up and re-imported from a file that *does* have one —
    // the ordinary "I found a better copy" move.
    engine.remove_file(&report.sha256).await.unwrap();
    assert!(
        engine.table_of_contents(book_id).await.unwrap().is_none(),
        "a book owning no file this can read has no chapter list to give"
    );
    let better = engine
        .add_file_to_book(book_id, &toc_fixture())
        .await
        .unwrap();

    let toc = engine.table_of_contents(book_id).await.unwrap().unwrap();
    assert_eq!(
        toc.sha256, better.sha256,
        "the answer names the file it read"
    );
    assert_eq!(
        toc.entries
            .iter()
            .map(|e| e.label.as_str())
            .collect::<Vec<_>>(),
        vec![
            "One: The Beginning",
            "Two: The Middle",
            "Two, part two",
            "Three: The End",
        ]
    );
    assert_eq!(toc.entries[2].depth, 1);
}

/// A book with no files at all — every sidecar-seeded book, which is most of a
/// real shelf — has no chapter list, and that is `None` rather than an error or
/// an empty one. The two answers are different and a frontend shows different
/// things for them.
#[tokio::test]
async fn a_book_with_no_epub_has_no_chapter_list() {
    let (_tmp, engine) = engine().await;
    let book = engine
        .save_book(&Book {
            title: Some("Nothing Owned".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        engine
            .table_of_contents(book.id.unwrap())
            .await
            .unwrap()
            .is_none()
    );
}

// ---- item 22: a pdf, read here ----------------------------------------------
//
// The "reading here" ownership row, from the file end. The unit tests in
// `pdf.rs` cover the parser; these cover the wiring — that what the file said
// reaches the book, that it is attributed, and that it never overwrites a claim
// somebody else already made.

/// The point of the whole item: a locally-attached PDF gives the book a
/// denominator it did not have, so `Progress` can report a fraction instead of
/// a bare page.
#[tokio::test]
async fn an_attached_pdf_gives_the_book_its_length() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("Long Walk to Freedom.pdf");
    write(&src, &readingbuddy::pdf::synthetic_pdf(312, None));

    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();
    assert_eq!(report.outcome, FileOutcome::Stored);
    let id = report.book_id.unwrap();

    let book = engine.storage().get_book(id).await.unwrap().unwrap();
    assert_eq!(book.page_count, Some(312));
    // No metadata reader means no author and no ISBN; the filename is the name.
    assert_eq!(book.title.as_deref(), Some("Long Walk to Freedom"));
    assert_eq!(engine.book_files(id).await.unwrap()[0].format, "pdf");

    // And the claim is attributed, not anonymous. `pdf` beside `epub` rather
    // than one `file`, because the two answer different questions.
    let prov = engine.field_provenance(id).await.unwrap();
    let page_count = prov.iter().find(|f| f.field == "page_count").unwrap();
    assert_eq!(page_count.source, "pdf");
}

/// A PDF that will not say how long it is leaves the column NULL. **Never a
/// zero** — item 17 spent an item establishing that absence is not zero, and a
/// zero denominator is the one value `Progress` must never be handed.
#[tokio::test]
async fn a_pdf_that_will_not_say_leaves_the_length_absent() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("Unreadable.pdf");
    write(&src, &readingbuddy::pdf::synthetic_pdf(0, None));

    let report = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap();
    let book = engine
        .storage()
        .get_book(report.book_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(book.page_count, None, "a length we could not read is NULL");
}

/// Attaching a PDF to a book that already has a page count does not rewrite it.
///
/// `fill_book`'s **stored row wins** merge, chosen for `calibre.rs`'s reason:
/// the pattern follows whether the record is complete. A page count on the book
/// is a provider's claim about a specific edition; a PDF is one more partial
/// record beside it, not an authority over it. The device's straight-assignment
/// pattern here would let any attached scan silently redefine the book's length.
#[tokio::test]
async fn attaching_a_pdf_fills_a_gap_and_never_overwrites_an_answer() {
    let (tmp, engine) = engine().await;
    let book = engine
        .save_book(&Book {
            title: Some("Pachinko".into()),
            page_count: Some(490),
            ..Default::default()
        })
        .await
        .unwrap();
    let id = book.id.unwrap();

    let src = tmp.path().join("scan.pdf");
    write(&src, &readingbuddy::pdf::synthetic_pdf(512, None));
    engine.add_file_to_book(id, &src).await.unwrap();

    let after = engine.storage().get_book(id).await.unwrap().unwrap();
    assert_eq!(after.page_count, Some(490), "the stored answer wins");
}

/// A producer's echo of its source filename is not a title, so the filename
/// stem is used instead. `Some("")` — which two of three real PDFs on the
/// author's machine returned — is absence for the same reason.
#[tokio::test]
async fn a_pdf_titled_after_its_word_document_falls_back_to_the_filename() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("Critique of Pure Reason.pdf");
    write(
        &src,
        &readingbuddy::pdf::synthetic_pdf(9, Some(b"Microsoft Word - kant_final_v2.doc")),
    );

    let id = engine.identify_file(&src).await.unwrap();
    assert_eq!(id.title.as_deref(), Some("Critique of Pure Reason"));
    assert_eq!(id.page_count, Some(9));
}

/// `identify` answers the length too, and still writes nothing. A frontend
/// showing "312 pages" before the user commits to the import is the shape
/// `docs/decisions.md` asks for — a decision, not a dead end.
#[tokio::test]
async fn identify_reports_a_pdfs_length_without_writing_anything() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("Anna Karenina.pdf");
    write(
        &src,
        &readingbuddy::pdf::synthetic_pdf(864, Some(b"Anna Karenina")),
    );

    let id = engine.identify_file(&src).await.unwrap();
    assert_eq!(id.page_count, Some(864));
    assert_eq!(id.title.as_deref(), Some("Anna Karenina"));
    assert_eq!(library_size(&engine).await, 0);
    assert!(stored_files(&files_dir(&tmp)).is_empty());
}

/// An epub reports no length at all, and that is not a regression: `epub
/// =2.1.4` exposes no pagination, and an epub has none to expose. The field is
/// PDF-shaped and honest about it.
#[tokio::test]
async fn an_epub_reports_no_length_because_it_has_none() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("No ISBN Here.epub");
    write_isbnless_epub(&src, "No ISBN Here");
    assert_eq!(engine.identify_file(&src).await.unwrap().page_count, None);
}

/// `format_of` sanitizes rather than trusts, and a `.PDF` from a Windows share
/// or a half-finished `.pdf.part` from a download is exactly what it is for. A
/// `.pdf.part` is deliberately **not** a pdf: it is `part`, so no PDF reader
/// runs on a file that is still being written.
#[tokio::test]
async fn the_extension_decides_the_format_and_case_does_not() {
    let (tmp, engine) = engine().await;
    let bytes = readingbuddy::pdf::synthetic_pdf(4, None);

    let shouty = tmp.path().join("Shouty.PDF");
    write(&shouty, &bytes);
    let id = engine.identify_file(&shouty).await.unwrap();
    assert_eq!(id.format, "pdf");
    assert_eq!(id.page_count, Some(4), "case must not hide the reader");

    let partial = tmp.path().join("Half Downloaded.pdf.part");
    write(&partial, &bytes);
    let id = engine.identify_file(&partial).await.unwrap();
    assert_eq!(id.format, "part");
    assert_eq!(
        id.page_count, None,
        "a partial download is not a pdf, whatever its bytes look like"
    );
}

// ---- item 22: a named reading's own progress --------------------------------

/// The gap item 22 opened and closed: a screen showing one *specific* reading
/// must not print the book's projection.
///
/// `Progress::of_book` reads `books.current_page`, which is a projection of the
/// **current** read — so on a reread it prints today's numbers under the first
/// read's heading. `Progress::of_reading` was already the right answer and had
/// no way onto the API, which left a frontend computing `current_page /
/// page_count` itself. `Engine::readings_with_progress` is the pairing, done
/// where the page count lives.
#[tokio::test]
async fn each_reading_carries_its_own_progress_and_not_the_books() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("Pachinko.pdf");
    write(&src, &readingbuddy::pdf::synthetic_pdf(500, None));
    let id = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap()
        .book_id
        .unwrap();

    // Read it once to page 500 and finish; then start again and get to 100.
    engine
        .update_progress(id, Some(500), Some(true))
        .await
        .unwrap();
    engine.reread(id).await.unwrap();
    engine.update_progress(id, Some(100), None).await.unwrap();

    let reads = engine.readings_with_progress(id).await.unwrap();
    assert_eq!(reads.len(), 2, "a reread is a row, not a flag");

    // The first read is finished; the second is a fifth of the way in — and the
    // fraction exists at all only because the PDF supplied a denominator.
    assert_eq!(reads[0].1, readingbuddy::Progress::Finished);
    assert_eq!(reads[1].1.percent(), Some(20));
    assert_eq!(reads[1].1.page(), Some(100));

    // The book's own projection is the *current* read's, which is the number
    // that would have been printed under the first read's heading.
    let book = engine.storage().get_book(id).await.unwrap().unwrap();
    assert_eq!(readingbuddy::Progress::of_book(&book).percent(), Some(20));
}

/// A book read once: the two forms agree, which is what makes the pair form
/// safe to use everywhere rather than only on rereads.
#[tokio::test]
async fn one_read_makes_the_two_progress_forms_agree() {
    let (tmp, engine) = engine().await;
    let src = tmp.path().join("Anna.pdf");
    write(&src, &readingbuddy::pdf::synthetic_pdf(200, None));
    let id = engine
        .import_file(&src, FileImportOptions::default())
        .await
        .unwrap()
        .book_id
        .unwrap();
    engine.update_progress(id, Some(50), None).await.unwrap();

    let book = engine.storage().get_book(id).await.unwrap().unwrap();
    let reads = engine.readings_with_progress(id).await.unwrap();
    assert_eq!(reads[0].1, readingbuddy::Progress::of_book(&book));

    let active = engine.active_reading_with_progress(id).await.unwrap();
    assert_eq!(active.unwrap().1, reads[0].1);
    let one = engine.reading_with_progress(reads[0].0.id).await.unwrap();
    assert_eq!(one.unwrap().1, reads[0].1);
}

/// A book with no length still reports a page, and no fraction. The case
/// `pdf.rs` exists to keep reachable: `Started { page: Some(n), of: None }`.
#[tokio::test]
async fn a_reading_with_no_denominator_reports_a_page_and_no_fraction() {
    let (_tmp, engine) = engine().await;
    let book = engine
        .save_book(&Book {
            title: Some("Unmeasured".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let id = book.id.unwrap();
    engine.update_progress(id, Some(42), None).await.unwrap();

    let p = engine.readings_with_progress(id).await.unwrap()[0].1;
    assert_eq!(p.page(), Some(42));
    assert_eq!(p.percent(), None, "no denominator is no percentage, not 0%");
}
