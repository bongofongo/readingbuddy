//! The `Engine` facade — the crate's actual public API, and previously the
//! single largest untested surface in the engine: 25 methods, zero tests.
//!
//! Everything here runs against a `TempDir` with an in-memory database, so the
//! suite stays offline and leaves nothing behind.

use std::path::PathBuf;

use readingbuddy::{Book, Engine, EngineConfig, NewNoteInput, NoteKind};
use tempfile::TempDir;

/// Build a config *literally* rather than via `EngineConfig::rooted_at`, which
/// reads `GOOGLE_BOOKS_API_KEY` from the ambient environment — that would make
/// these tests pass or fail depending on the developer's shell.
async fn engine() -> (TempDir, Engine) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = EngineConfig {
        db_url: "sqlite::memory:".into(),
        images_dir: tmp.path().join("database/images"),
        vault_dir: tmp.path().join("vault"),
        log_dir: tmp.path().join("logs"),
        google_api_key: None,
    };
    let engine = Engine::open(config).await.expect("engine opens");
    (tmp, engine)
}

fn book(title: &str) -> Book {
    Book {
        title: Some(title.into()),
        authors: vec!["Min Jin Lee".into()],
        ..Default::default()
    }
}

#[tokio::test]
async fn open_creates_its_directories_and_migrates() {
    let (tmp, engine) = engine().await;
    assert!(tmp.path().join("database/images").is_dir());
    assert!(tmp.path().join("vault").is_dir());
    // Migrations ran, so the schema is queryable.
    assert!(engine.list_notes(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn save_book_round_trips_and_assigns_an_id() {
    let (_tmp, engine) = engine().await;
    let saved = engine.save_book(&book("Pachinko")).await.unwrap();
    assert!(saved.id.is_some());
    assert_eq!(saved.title.as_deref(), Some("Pachinko"));
}

/// `resolve_books` is what every CLI subcommand and the TUI use to turn a
/// user-typed selector into a book, so all four of its branches matter.
#[tokio::test]
async fn resolve_books_covers_id_isbn_title_and_miss() {
    let (_tmp, engine) = engine().await;
    let saved = engine
        .save_book(&Book {
            isbn_13: Some("9781784161880".into()),
            ..book("Pachinko")
        })
        .await
        .unwrap();
    let id = saved.id.unwrap();

    // by numeric id
    let by_id = engine.resolve_books(&id.to_string()).await.unwrap();
    assert_eq!(by_id.len(), 1);
    assert_eq!(by_id[0].id, Some(id));

    // by ISBN, including a hyphenated form — normalize_isbn is on this path
    for sel in ["9781784161880", "978-1-78416-188-0"] {
        let hit = engine.resolve_books(sel).await.unwrap();
        assert_eq!(hit.len(), 1, "ISBN selector {sel:?} did not resolve");
        assert_eq!(hit[0].id, Some(id));
    }

    // by title fragment
    let by_title = engine.resolve_books("pach").await.unwrap();
    assert_eq!(by_title.len(), 1);

    // no match is an empty vec, never an error
    assert!(
        engine
            .resolve_books("nothing like this")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_ambiguous_title_returns_every_candidate() {
    let (_tmp, engine) = engine().await;
    engine.save_book(&book("The Long Way")).await.unwrap();
    engine.save_book(&book("The Long Goodbye")).await.unwrap();

    let hits = engine.resolve_books("The Long").await.unwrap();
    assert_eq!(hits.len(), 2, "ambiguity must be reported, not resolved");
}

#[tokio::test]
async fn delete_book_removes_the_row_and_its_cover_file() {
    let (tmp, engine) = engine().await;
    let cover = tmp.path().join("database/images/pachinko.jpg");
    std::fs::create_dir_all(cover.parent().unwrap()).unwrap();
    std::fs::write(&cover, b"not really a jpeg").unwrap();

    let saved = engine
        .save_book(&Book {
            cover_path: Some(cover.display().to_string()),
            ..book("Pachinko")
        })
        .await
        .unwrap();

    engine.delete_book(saved.id.unwrap()).await.unwrap();
    assert!(engine.resolve_books("Pachinko").await.unwrap().is_empty());
    assert!(!cover.exists(), "cover file was orphaned on disk");
}

#[tokio::test]
async fn deleting_a_book_whose_cover_is_already_gone_is_not_an_error() {
    let (tmp, engine) = engine().await;
    let saved = engine
        .save_book(&Book {
            cover_path: Some(tmp.path().join("nope.jpg").display().to_string()),
            ..book("Pachinko")
        })
        .await
        .unwrap();
    engine
        .delete_book(saved.id.unwrap())
        .await
        .expect("missing cover must not fail");
}

// ---- notes -----------------------------------------------------------------

#[tokio::test]
async fn a_note_survives_a_full_create_edit_reread_delete_cycle() {
    let (tmp, engine) = engine().await;
    let saved = engine.save_book(&book("Pachinko")).await.unwrap();

    let created = engine
        .create_note(NewNoteInput {
            book_id: saved.id,
            page: Some(42),
            kind: NoteKind::Note,
            title: Some("Opening line".into()),
            body: "History has failed us, but no matter.".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        created.file.exists(),
        "note file was not written to the vault"
    );

    let notes = engine.list_notes(saved.id).await.unwrap();
    assert_eq!(notes.len(), 1);
    let note = &notes[0];

    // note_body returns the body without the frontmatter header.
    let body = engine.note_body(note).unwrap();
    assert_eq!(body, "History has failed us, but no matter.");
    assert!(!body.contains("---"), "frontmatter leaked into the body");

    // The rewrite must preserve the header verbatim — that is the whole
    // contract of update_note_body.
    let before = std::fs::read_to_string(&created.file).unwrap();
    let header_before = before.split_once("\n---\n").map(|(h, _)| h.to_string());
    engine
        .update_note_body(note, "Rewritten entirely.")
        .await
        .unwrap();

    let after = std::fs::read_to_string(&created.file).unwrap();
    assert_eq!(
        after.split_once("\n---\n").map(|(h, _)| h.to_string()),
        header_before,
        "frontmatter was not preserved across the edit"
    );
    assert!(after.contains("Rewritten entirely."));
    assert!(after.contains("page: 42"), "the anchor was lost");
    assert_eq!(engine.note_body(note).unwrap(), "Rewritten entirely.");

    // FTS picked up the new body, not the old one.
    assert!(
        !engine
            .search_notes("Rewritten", 10)
            .await
            .unwrap()
            .is_empty()
    );

    engine.delete_note(note).await.unwrap();
    assert!(!created.file.exists(), "vault file outlived its row");
    assert!(engine.list_notes(saved.id).await.unwrap().is_empty());
    let _ = tmp;
}

/// A user can delete a note from Obsidian; the DB row must still go.
#[tokio::test]
async fn deleting_a_note_whose_file_is_already_gone_still_clears_the_row() {
    let (_tmp, engine) = engine().await;
    let saved = engine.save_book(&book("Pachinko")).await.unwrap();
    let created = engine
        .create_note(NewNoteInput {
            book_id: saved.id,
            body: "some thought".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    std::fs::remove_file(&created.file).unwrap();
    let note = &engine.list_notes(saved.id).await.unwrap()[0];
    engine
        .delete_note(note)
        .await
        .expect("a missing vault file must not block the row delete");
    assert!(engine.list_notes(saved.id).await.unwrap().is_empty());
}

/// Obsidian edits the file behind our back; the FTS index has to catch up.
#[tokio::test]
async fn refresh_note_from_disk_reindexes_an_external_edit() {
    let (_tmp, engine) = engine().await;
    let saved = engine.save_book(&book("Pachinko")).await.unwrap();
    let created = engine
        .create_note(NewNoteInput {
            book_id: saved.id,
            body: "original wording".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let note = engine.list_notes(saved.id).await.unwrap().remove(0);
    let raw = std::fs::read_to_string(&created.file).unwrap();
    std::fs::write(
        &created.file,
        raw.replace("original wording", "externally rewritten"),
    )
    .unwrap();

    engine.refresh_note_from_disk(&note).await.unwrap();
    assert!(
        !engine
            .search_notes("externally", 10)
            .await
            .unwrap()
            .is_empty(),
        "external edit never reached the index"
    );
}

// ---- flashcards ------------------------------------------------------------

#[tokio::test]
async fn export_flashcards_marks_what_it_exported() {
    let (_tmp, engine) = engine().await;
    let saved = engine.save_book(&book("Pachinko")).await.unwrap();
    let id = saved.id.unwrap();

    engine
        .storage
        .insert_flashcard(id, None, "pachinko", Some("a game"))
        .await
        .unwrap();
    engine
        .storage
        .insert_flashcard(id, None, "hanko", None)
        .await
        .unwrap();

    assert_eq!(engine.list_flashcards(false).await.unwrap().len(), 2);
    assert_eq!(engine.list_flashcards_for_book(id).await.unwrap().len(), 2);

    let (tsv, count) = engine.export_flashcards(false).await.unwrap();
    assert_eq!(count, 2);
    assert!(tsv.starts_with("#separator:tab"));
    assert!(tsv.contains("pachinko"));

    // Exported cards drop out of the default listing but are still there when
    // explicitly asked for — otherwise a second export would re-emit the deck.
    assert!(
        engine.list_flashcards(false).await.unwrap().is_empty(),
        "cards were not marked exported"
    );
    assert_eq!(engine.list_flashcards(true).await.unwrap().len(), 2);
    assert_eq!(engine.export_flashcards(false).await.unwrap().1, 0);
}

// ---- input validation ------------------------------------------------------

#[tokio::test]
async fn a_malformed_isbn_is_rejected_before_any_network_call() {
    let (_tmp, engine) = engine().await;
    // Bad checksum: must fail on validation, not by attempting a lookup.
    let err = engine.lookup_isbn("9781784161881").await.unwrap_err();
    assert!(
        matches!(err, readingbuddy::EngineError::InvalidIsbn(_)),
        "expected InvalidIsbn, got {err:?}"
    );
}

#[tokio::test]
async fn importing_a_missing_epub_errors_rather_than_panicking() {
    let (_tmp, engine) = engine().await;
    let err = engine
        .import_epub(&PathBuf::from("/definitely/not/here.epub"))
        .await
        .unwrap_err();
    assert!(matches!(err, readingbuddy::EngineError::Epub(_)), "{err:?}");
}

#[tokio::test]
async fn importing_koreader_from_an_empty_dir_warns_rather_than_failing() {
    let (tmp, engine) = engine().await;
    let empty = tmp.path().join("empty-library");
    std::fs::create_dir_all(&empty).unwrap();

    let report = engine.import_koreader(&empty, false).await.unwrap();
    assert!(report.imported.is_empty());
    assert_eq!(report.warnings.len(), 1);
    assert!(
        report.warnings[0]
            .to_string()
            .starts_with("no KOReader sidecars found under")
    );
}

// ---- device identity (partial_md5) -----------------------------------------

/// A valid, ~2 KB epub with **no ISBN identifier**.
///
/// The missing ISBN is the point, not an oversight: `import_epub` looks a found
/// ISBN up through the providers, and this suite makes no network calls. Built
/// rather than committed, for the same reason the corpus generator builds its
/// own — what is inside it stays readable in the diff.
fn write_isbnless_epub(path: &std::path::Path, title: &str) {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    // `mimetype` must be first and STORED per the epub spec.
    zip.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(b"application/epub+zip").unwrap();

    let opts = SimpleFileOptions::default();
    zip.start_file("META-INF/container.xml", opts).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#,
    )
    .unwrap();

    zip.start_file("OEBPS/content.opf", opts).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">urn:uuid:00000000-0000-4000-8000-000000000000</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator>A Test Author</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
  </spine>
</package>
"#
        )
        .as_bytes(),
    )
    .unwrap();

    zip.start_file("OEBPS/ch1.xhtml", opts).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title></head>
<body><h1>{title}</h1><p>A single paragraph, so the spine is not empty.</p></body></html>
"#
        )
        .as_bytes(),
    )
    .unwrap();
    zip.finish().unwrap();
}

/// What a book is linked to, through the public API — `Storage::pool` is
/// `pub(crate)`, so an integration test reads the mapping the way a frontend
/// would. `linked_by` is not on this surface at all; that it survives a scan
/// unrelabelled is `device_books.rs`'s own test.
async fn links_for(engine: &Engine, book_id: i64) -> Vec<String> {
    engine.storage.device_links_for_book(book_id).await.unwrap()
}

/// The whole point of item 5: an epub imported here already carries the file
/// identity the device will name it by, so its sidecar matches on `md5` rather
/// than on a fuzzy title guess.
#[tokio::test]
async fn importing_an_epub_records_the_identity_koreader_will_use() {
    let (tmp, engine) = engine().await;
    let path = tmp.path().join("offline.epub");
    write_isbnless_epub(&path, "A Book With No ISBN");

    let saved = engine.import_epub(&path).await.unwrap();
    let md5 = readingbuddy::partial_md5(&path).unwrap();

    let linked = engine
        .storage
        .find_book_by_partial_md5(&md5)
        .await
        .unwrap()
        .expect("the imported epub must be reachable by its device identity");
    assert_eq!(linked.id, saved.id);
    assert_eq!(links_for(&engine, saved.id.unwrap()).await, [md5]);
}

/// Re-importing writes no second mapping. (It does create a second *book* —
/// an ISBN-less epub has no upsert key, which is exactly the duplicate
/// `Storage::merge_books` exists to fold back in. The mapping must not follow
/// it, or the sidecar would silently start pointing at the newer copy.)
#[tokio::test]
async fn re_importing_the_same_epub_writes_one_mapping() {
    let (tmp, engine) = engine().await;
    let path = tmp.path().join("offline.epub");
    write_isbnless_epub(&path, "A Book With No ISBN");

    let first = engine.import_epub(&path).await.unwrap();
    let second = engine.import_epub(&path).await.unwrap();
    assert_ne!(first.id, second.id, "no ISBN means no upsert key");

    let md5 = readingbuddy::partial_md5(&path).unwrap();
    assert_eq!(links_for(&engine, first.id.unwrap()).await, [md5]);
    assert!(
        links_for(&engine, second.id.unwrap()).await.is_empty(),
        "the mapping must stay on the copy it was first recorded against"
    );
}

/// A manual link is the user's decision. Importing the file again is a scan,
/// and a scan must never repoint it or relabel it as a guess — which is why
/// the hook calls `link_device_book` and not `set_device_link`.
#[tokio::test]
async fn importing_does_not_repoint_a_link_the_user_made() {
    let (tmp, engine) = engine().await;
    let path = tmp.path().join("offline.epub");
    write_isbnless_epub(&path, "A Book With No ISBN");
    let md5 = readingbuddy::partial_md5(&path).unwrap();

    let chosen = engine
        .save_book(&book("The Copy The User Chose"))
        .await
        .unwrap();
    engine
        .storage
        .set_device_link(
            &md5,
            chosen.id.unwrap(),
            readingbuddy::storage::LinkedBy::Manual,
        )
        .await
        .unwrap();

    engine.import_epub(&path).await.unwrap();

    assert_eq!(links_for(&engine, chosen.id.unwrap()).await, [md5.as_str()]);
    let created = engine
        .storage
        .find_book_by_partial_md5(&md5)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created.id, chosen.id, "a scan must not repoint");
}
