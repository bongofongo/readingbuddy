//! Narrative workflows — whole journeys through the facade, not single methods.
//!
//! Every other integration file here tests one surface: `engine_facade.rs` is
//! per-method, `device_scan.rs` is the scanner, `reflection_review.rs` is item 7.
//! Each of those seeds its own world with whatever is convenient, which means
//! the *seams* between features — features that were, in this repo, built in
//! separate threads and merged separately — are the one thing nothing covers.
//!
//! So the rule for this file: a test here must cross at least two build items,
//! and its doc comment must name what it catches that a per-module test cannot.
//! A story that could be written against one module belongs in that module.
//!
//! Sample data is the committed tier-1 corpus staged into a tempdir, because a
//! scan mutates what it reads. When a device-scale tier lands
//! (`docs/decisions.md` item 1's follow-up) it arrives through `common::place`
//! and nothing else here changes.
//!
//! Offline throughout: `sqlite::memory:`, a `TempDir` vault, no network.

use readingbuddy::device::DeviceState;
use readingbuddy::storage::BookSort;
use readingbuddy::{Book, NewNoteInput, NoteKind};

mod common;
use common::{engine, place, rewrite_sidecar, seed_book};

/// Mount → scan → pull → annotate → reflect → cite → rate → review.
///
/// The join nobody owns. Item 4 answered "does a reading need to exist before
/// highlights import?" by fiat: an import opens one when the sidecar carries
/// device state (`summary` / `percent_finished`). Item 7 resolves a reflection's
/// anchor through `ensure_reading`, which opens one only when the book has
/// *none*. Both are tested — separately, each against a book the test seeded by
/// hand. Nothing checks they agree, and if they did not, a reflection written
/// after an import would silently anchor to a second, empty reading.
#[tokio::test]
async fn a_book_pulled_off_the_device_reflects_on_the_reading_the_import_opened() {
    let (_tmp, engine) = engine().await;
    let device = tempfile::tempdir().unwrap();
    let root = device.path().join("KOBOeReader");
    let sidecar = place(&root, "Gen-Summary.sdr", "Rated.sdr");

    // Nothing in the library: this book arrives from the device, which is the
    // path `pull_book_from_sidecar` exists for and which no other test in the
    // repo follows all the way to a reflection.
    let scan = engine.scan_device(&root).await.unwrap();
    assert_eq!(scan.books.len(), 1);
    assert!(matches!(scan.books[0].state, DeviceState::New { .. }));

    let report = engine.pull_book_from_sidecar(&sidecar).await.unwrap();
    let book = report.stats.book_id;
    assert_eq!(report.stats.inserted, 1);

    // The sidecar carried `summary` + `percent_finished`, so the import opened a
    // reading and mirrored the device's state onto it.
    let readings = engine.storage.list_readings(book).await.unwrap();
    assert_eq!(readings.len(), 1, "the import opened exactly one reading");
    assert_eq!(readings[0].ko_status.as_deref(), Some("complete"));
    assert_eq!(readings[0].source, "koreader");

    // …and the reflection anchors to *that* reading rather than minting another.
    let reflection = engine.open_reflection(book, None).await.unwrap();
    let record = engine
        .storage
        .get_note(reflection.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        record.reading_id,
        Some(readings[0].id),
        "the reflection must attach to the reading the import opened, not a new one"
    );
    assert_eq!(
        engine.storage.list_readings(book).await.unwrap().len(),
        1,
        "reflecting must not start a second reading"
    );

    // A review of the same reading, with a rating, and a citation of a real
    // imported highlight — the full item 7 surface over device-sourced data.
    let highlights = engine.storage.list_highlights(book).await.unwrap();
    assert_eq!(highlights.len(), 1);
    let review = engine.open_review(book, None).await.unwrap();
    engine.cite(review.id, highlights[0].id).await.unwrap();
    engine.set_rating(review.id, 4.0).await.unwrap();

    assert_eq!(engine.goodreads_rating(review.id).await.unwrap(), Some(4));
    let cited = engine.citations_for(review.id).await.unwrap();
    assert_eq!(cited.len(), 1);
    assert_eq!(cited[0].text, "a passage in a finished book");
}

/// A note edited on the device, seen through a *scan* rather than a direct
/// import — with a citation and a note anchored to the highlight it touches.
///
/// The nearest existing test (`citations_survive_a_device_refresh_and_cascade_on_delete`)
/// calls the import path directly, which walks straight past `sidecar_seen`. The
/// cache is the one component that can serve a stale digest and make a device
/// edit *invisible*: it is exactly the failure `DeviceDigest` replaced a row
/// count to fix. Nothing has ever driven that path with a citation and a note
/// anchor hanging off the row being refreshed — which is where a
/// delete-and-reinsert refresh would cascade a flashcard away and null an anchor.
#[tokio::test]
async fn a_device_note_edit_reaches_us_through_the_cache_and_spares_everything_ours() {
    let (_tmp, engine) = engine().await;
    let device = tempfile::tempdir().unwrap();
    let root = device.path().join("KOBOeReader");
    let sidecar = place(&root, "Pachinko.sdr", "Pachinko.sdr");
    seed_book(&engine, "Pachinko").await;

    // Scan before syncing, which is the order the device screen uses and the
    // only order that populates `sidecar_seen` — the cache records what a
    // *scan* read, so syncing first would leave the next scan doing the first
    // parse and the "unchanged costs nothing" assertion below would be
    // measuring the wrong thing.
    let first = engine.scan_device(&root).await.unwrap();
    assert_eq!(first.parsed, 1);
    engine
        .sync_device(std::slice::from_ref(&sidecar))
        .await
        .unwrap();
    let book = engine
        .storage
        .list_books(10, BookSort::LastModified)
        .await
        .unwrap()[0]
        .id
        .unwrap();

    // Attach everything of ours that a careless refresh would take with it.
    let highlights = engine.storage.list_highlights(book).await.unwrap();
    let target = highlights
        .iter()
        .find(|h| h.text.starts_with("History has failed us"))
        .expect("the opening line is in the fixture");
    let before_id = target.id;
    engine
        .storage
        .set_annotation(before_id, Some("mine, and the device may not touch it"))
        .await
        .unwrap();
    let review = engine.open_review(book, None).await.unwrap();
    engine.cite(review.id, before_id).await.unwrap();
    let anchored = engine
        .create_note(NewNoteInput {
            book_id: Some(book),
            reading_id: None,
            title: Some("On the opening line".into()),
            body: "It sets the register for the whole book.".into(),
            kind: NoteKind::Note,
            page: None,
            location: None,
            highlight_id: Some(before_id),
        })
        .await
        .unwrap();

    // The device edits the note on that annotation. Same identity hash, new
    // payload — so a count-based cache would see nothing move.
    let second = engine.scan_device(&root).await.unwrap();
    assert_eq!(
        second.parsed, 0,
        "unmodified: the pre-filter should skip it"
    );
    rewrite_sidecar(&sidecar, |s| {
        s.replace(
            "Opening line - sets the whole register.",
            "Rewritten on the device, weeks later.",
        )
    });

    let third = engine.scan_device(&root).await.unwrap();
    assert_eq!(third.parsed, 1, "the bytes moved, so it must be re-read");
    assert_eq!(
        third.books[0].state,
        DeviceState::Updated {
            new_highlights: 0,
            refreshed: 1
        },
        "a rewritten note is a refresh, not a new highlight"
    );

    engine
        .sync_device(std::slice::from_ref(&sidecar))
        .await
        .unwrap();

    let after = engine.storage.list_highlights(book).await.unwrap();
    let refreshed = after.iter().find(|h| h.id == before_id).expect(
        "the row must be updated in place — notes.highlight_id and \
         flashcards.highlight_id are foreign keys",
    );
    assert_eq!(
        refreshed.ko_note.as_deref(),
        Some("Rewritten on the device, weeks later."),
        "theirs tracks the device"
    );
    assert_eq!(
        refreshed.annotation.as_deref(),
        Some("mine, and the device may not touch it"),
        "ours survives"
    );
    assert_eq!(refreshed.created_at, target.created_at);
    assert_eq!(refreshed.source, target.source);

    // The citation is by reference, so it followed the refresh rather than
    // pointing at a row that no longer exists.
    let cited = engine.citations_for(review.id).await.unwrap();
    assert_eq!(cited.len(), 1);
    assert_eq!(cited[0].id, before_id);
    assert_eq!(
        cited[0].ko_note.as_deref(),
        Some("Rewritten on the device, weeks later."),
        "a citation reads through to the live row"
    );

    let note = engine.storage.get_note(anchored.id).await.unwrap().unwrap();
    assert_eq!(
        note.highlight_id,
        Some(before_id),
        "the note anchor must not have been nulled"
    );

    // And the whole thing settles: a fourth scan sees nothing to do.
    let fourth = engine.scan_device(&root).await.unwrap();
    assert_eq!(fourth.books[0].state, DeviceState::Unchanged);
}

/// A scan that has already been synced must cost nothing, measured at the
/// facade rather than at `scan_device`'s own counter.
///
/// `device_scan.rs` proves this against `device::scan_device` directly. The
/// facade adds `Engine`'s own storage and vault, and the prune runs on every
/// scan — so "the cache holds across an `Engine` round trip, and the prune does
/// not quietly forget rows it should keep" is a different claim from the one
/// already tested. Asserted as parse counts, never as durations.
#[tokio::test]
async fn a_synced_device_costs_nothing_to_look_at_again() {
    let (_tmp, engine) = engine().await;
    let device = tempfile::tempdir().unwrap();
    let root = device.path().join("KOBOeReader");
    place(&root, "Pachinko.sdr", "Pachinko.sdr");
    place(&root, "Multi-Chapter.sdr", "Odyssey.sdr");
    place(&root, "Gen-Summary.sdr", "Rated.sdr");
    seed_book(&engine, "Pachinko").await;
    seed_book(&engine, "The Odyssey").await;

    let first = engine.scan_device(&root).await.unwrap();
    assert_eq!(first.parsed, 3);
    assert_eq!(first.cached, 0);

    let paths: Vec<_> = first.syncable().map(|b| b.path.clone()).collect();
    engine.sync_device(&paths).await.unwrap();

    for round in 0..3 {
        let again = engine.scan_device(&root).await.unwrap();
        assert_eq!(again.parsed, 0, "round {round}: nothing changed on disk");
        assert_eq!(again.cached, 3, "round {round}");
        assert!(
            again
                .books
                .iter()
                .all(|b| b.state == DeviceState::Unchanged),
            "round {round}: {:?}",
            again.books.iter().map(|b| &b.state).collect::<Vec<_>>()
        );
    }
}

/// Merging a duplicate that arrived from the device, with reflections on both.
///
/// `merge_books` (item 3) predates reflections (item 7) — different threads,
/// merged separately, and nothing sits between them. The interesting case is
/// specific: both books carry an *open* reading, so the merge closes the older
/// as `abandoned` and repoints it, and both reflections then survive
/// `idx_one_reflection` because their `reading_id`s differ. That leaves two
/// reflections on one book, and their titles are wikilink targets.
///
/// This test pins what actually happens, because "two reflections on the
/// surviving book" is either correct — a reread's worth of thinking, kept — or a
/// bug, and until something asserts it, it is neither.
#[tokio::test]
async fn merging_a_duplicate_keeps_both_reflections_and_every_citation() {
    let (_tmp, engine) = engine().await;
    let device = tempfile::tempdir().unwrap();
    let root = device.path().join("KOBOeReader");

    // The library's copy, with a reading and a reflection of its own.
    let dst = seed_book(&engine, "Pachinko").await;
    let dst_reflection = engine.open_reflection(dst, None).await.unwrap();
    let dst_record = engine
        .storage
        .get_note(dst_reflection.id)
        .await
        .unwrap()
        .unwrap();
    engine
        .update_note_body(&dst_record, "What I thought reading it here.")
        .await
        .unwrap();

    // The device's copy, pulled in as a *new* book — which is what happens when
    // the title does not match closely enough, and is why merge_books exists at
    // all: the ISBN-less insert path guarantees duplicates however good matching
    // gets.
    let sidecar = place(&root, "Unmatched.sdr", "Stranger.sdr");
    let pulled = engine.pull_book_from_sidecar(&sidecar).await.unwrap();
    let src = pulled.stats.book_id;
    assert_ne!(src, dst);

    let src_highlights = engine.storage.list_highlights(src).await.unwrap();
    assert!(
        !src_highlights.is_empty(),
        "the fixture carries annotations"
    );
    let src_review = engine.open_review(src, None).await.unwrap();
    engine
        .cite(src_review.id, src_highlights[0].id)
        .await
        .unwrap();
    let src_reflection = engine.open_reflection(src, None).await.unwrap();
    let src_record = engine
        .storage
        .get_note(src_reflection.id)
        .await
        .unwrap()
        .unwrap();
    engine
        .update_note_body(&src_record, "What I thought reading it there.")
        .await
        .unwrap();

    let report = engine.merge_books(src, dst).await.unwrap();
    assert!(report.src_existed);
    assert_eq!(report.highlights_moved, src_highlights.len());
    assert!(report.readings_moved >= 1);

    // Both reflections survive, on the survivor, with their bodies intact. The
    // partial unique index permits it because they anchor to different readings.
    let notes = engine.list_notes(Some(dst)).await.unwrap();
    let reflections: Vec<_> = notes.iter().filter(|n| n.kind == "reflection").collect();
    assert_eq!(
        reflections.len(),
        2,
        "one per reading, which is what anchoring to a reading means"
    );
    let readings: Vec<_> = reflections.iter().map(|n| n.reading_id).collect();
    assert_ne!(readings[0], readings[1], "…and they must not share one");
    assert_eq!(
        engine.note_body(&src_record).unwrap(),
        "What I thought reading it there.",
        "the merged-in reflection keeps its text"
    );
    assert_eq!(
        engine.note_body(&dst_record).unwrap(),
        "What I thought reading it here."
    );

    // The citation was repointed at the surviving highlight rather than
    // cascading away with the deleted book.
    let cited = engine.citations_for(src_review.id).await.unwrap();
    assert_eq!(
        cited.len(),
        1,
        "a citation must survive its book being merged"
    );
    assert_eq!(cited[0].book_id, dst);

    // Idempotent: doing it again finds nothing and is not an error, which is
    // what makes a retry — or a device screen firing twice — safe.
    let again = engine.merge_books(src, dst).await.unwrap();
    assert!(!again.src_existed);
    assert_eq!(again.highlights_moved, 0);
}

/// Two books joined reflection-to-reflection, then one body rewritten.
///
/// Book-to-book connection running through reflections is the whole reason item
/// 7 exists, and `update_note_body` re-indexing edges was a late fix — without
/// it the hub of the graph is the one note with no edges, since a reflection is
/// created empty and written afterwards. What is untested is the *rewrite*: that
/// dropping a link from a body removes the edge rather than merging into it, and
/// that the FTS index follows the same write.
#[tokio::test]
async fn a_reflection_links_to_another_and_the_graph_follows_the_body() {
    let (_tmp, engine) = engine().await;
    let pachinko = seed_book(&engine, "Pachinko").await;
    let other = engine
        .save_book(&Book {
            title: Some("Free Food for Millionaires".into()),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
        .unwrap();

    let a = engine.open_reflection(pachinko, None).await.unwrap();
    let b = engine.open_reflection(other, None).await.unwrap();
    let a_rec = engine.storage.get_note(a.id).await.unwrap().unwrap();

    engine
        .update_note_body(
            &a_rec,
            &format!(
                "Same preoccupations as [[{}]], twenty years earlier.",
                b.title
            ),
        )
        .await
        .unwrap();

    let links = engine.storage.note_links(a.id).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].0, b.title);
    assert_eq!(
        links[0].1,
        Some(b.id),
        "the target exists, so the edge must resolve rather than dangle"
    );

    // FTS follows the same write.
    let hits = engine.search_notes("preoccupations", 10).await.unwrap();
    assert!(
        hits.iter().any(|h| h.note.id == a.id),
        "a rewritten body has to reach the search index"
    );

    // Replace, not merge: a link removed from the body leaves the graph.
    engine
        .update_note_body(&a_rec, "On reflection, they are not much alike.")
        .await
        .unwrap();
    assert!(
        engine.storage.note_links(a.id).await.unwrap().is_empty(),
        "a deleted wikilink must leave the graph too"
    );
    assert!(
        engine
            .search_notes("preoccupations", 10)
            .await
            .unwrap()
            .is_empty(),
        "and the old body must leave the index"
    );
}

/// An Obsidian edit — the file changed underneath us, nothing else did.
///
/// The vault is openable in Obsidian by design, so this is a real path, not a
/// hypothetical: the user edits a reflection in another editor and adds a
/// wikilink. `refresh_note_from_disk` re-indexes both the FTS body and the
/// edges, and this pins that. Worth pinning because the method's summary line
/// mentions only the FTS body, so the edge half reads like an implementation
/// detail that a future refactor could drop without any test noticing.
#[tokio::test]
async fn an_edit_made_outside_the_app_reaches_the_index_and_the_graph() {
    let (tmp, engine) = engine().await;
    let pachinko = seed_book(&engine, "Pachinko").await;
    let other = seed_book(&engine, "Free Food for Millionaires").await;

    let a = engine.open_reflection(pachinko, None).await.unwrap();
    let b = engine.open_reflection(other, None).await.unwrap();
    let a_rec = engine.storage.get_note(a.id).await.unwrap().unwrap();

    // Write straight to the vault file, exactly as another editor would —
    // frontmatter preserved, body replaced.
    let file = tmp.path().join("vault").join(&a_rec.file_path);
    let existing = std::fs::read_to_string(&file).unwrap();
    let (frontmatter, _) = existing.split_at(existing.rfind("---\n").unwrap() + 4);
    std::fs::write(
        &file,
        format!(
            "{frontmatter}\nEdited elsewhere. See [[{}]] — antechamber.\n",
            b.title
        ),
    )
    .unwrap();

    // Before the refresh the app knows nothing about it. That is correct, and
    // asserting it is what makes the next block mean something.
    assert!(
        engine
            .search_notes("antechamber", 10)
            .await
            .unwrap()
            .is_empty(),
        "an outside edit is invisible until we are told to re-read it"
    );

    engine.refresh_note_from_disk(&a_rec).await.unwrap();

    let hits = engine.search_notes("antechamber", 10).await.unwrap();
    assert!(hits.iter().any(|h| h.note.id == a.id), "the index followed");

    let links = engine.storage.note_links(a.id).await.unwrap();
    assert_eq!(links.len(), 1, "and so did the graph");
    assert_eq!(links[0].1, Some(b.id));
}

/// Unmatched is a decision, and the decision has to survive the next scan.
///
/// `link_sidecar` records the user's choice through `set_device_link`, which
/// *repoints* — deliberately unlike `link_device_book`, which a scan uses and
/// which must never relabel a manual link as a guess. CLAUDE.md states that
/// rule; nothing has ever made a manual link and then run a scan over it, which
/// is the only way to observe a scan overwriting one.
#[tokio::test]
async fn a_link_the_user_made_by_hand_survives_every_later_scan() {
    let (_tmp, engine) = engine().await;
    let device = tempfile::tempdir().unwrap();
    let root = device.path().join("KOBOeReader");
    let sidecar = place(&root, "Unmatched.sdr", "Stranger.sdr");

    // A book the matcher will not connect to this sidecar on its own.
    let book = seed_book(&engine, "Pachinko").await;
    let scan = engine.scan_device(&root).await.unwrap();
    assert!(
        matches!(scan.books[0].state, DeviceState::New { .. }),
        "precondition: the matcher does not find it"
    );
    assert_eq!(scan.books[0].book_id, None);

    // The user decides.
    engine.link_sidecar(&sidecar, book).await.unwrap();

    // Every scan from here on must honour it rather than guess again.
    for round in 0..3 {
        let after = engine.scan_device(&root).await.unwrap();
        assert_eq!(
            after.books[0].book_id,
            Some(book),
            "round {round}: the recorded decision must be used"
        );
        assert_eq!(
            after.books[0].matched_by,
            Some(readingbuddy::MatchMethod::Md5),
            "round {round}: matched by the recorded link, not re-guessed by title"
        );
        assert_ne!(
            after.books[0].state,
            DeviceState::New {
                candidates: Vec::new()
            },
            "round {round}: a linked sidecar is never New again"
        );
    }
}
