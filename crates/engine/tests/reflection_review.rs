//! Reflection and Review through the facade — the surface a frontend uses.
//!
//! Offline, `sqlite::memory:` plus a `TempDir` vault, and nothing left behind.

use readingbuddy::{Book, Engine, EngineError, NewNoteInput, NoteKind};

mod common;
use common::{engine, highlight, seed_book};

async fn seeded(engine: &Engine) -> i64 {
    seed_book(engine, "Pachinko").await
}

// ---- accretion ------------------------------------------------------------

/// "Accretes" means exactly this: the second call is the first note, not a
/// second one. It is the whole reason a reflection can be opened mid-book.
#[tokio::test]
async fn opening_a_reflection_twice_returns_the_same_note_and_file() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;

    let first = engine.open_reflection(book, None).await.unwrap();
    assert!(first.file.exists());
    let record = engine.storage().get_note(first.id).await.unwrap().unwrap();
    engine
        .update_note_body(&record, "Sunja's dignity, chapter by chapter.")
        .await
        .unwrap();

    let again = engine.open_reflection(book, None).await.unwrap();
    assert_eq!(again.id, first.id);
    assert_eq!(again.file, first.file);
    assert_eq!(
        engine.note_body(&record).unwrap(),
        "Sunja's dignity, chapter by chapter.",
        "reopening must not blank what was written"
    );
    assert_eq!(engine.list_notes(Some(book), None).await.unwrap().len(), 1);
}

/// A reflection is opened *while reading*, so a book that was never marked as
/// started gets a reading rather than an error.
#[tokio::test]
async fn a_reflection_opens_a_reading_when_the_book_has_none() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    assert!(
        engine
            .storage()
            .list_readings(book)
            .await
            .unwrap()
            .is_empty()
    );

    let note = engine.open_reflection(book, None).await.unwrap();
    let readings = engine.storage().list_readings(book).await.unwrap();
    assert_eq!(readings.len(), 1);
    let record = engine.storage().get_note(note.id).await.unwrap().unwrap();
    assert_eq!(record.reading_id, Some(readings[0].id));
    assert_eq!(record.kind, "reflection");
}

/// It attaches to the reading the user is *in*, not a fresh one — including
/// after the book is finished, when the current reading is the one just closed.
#[tokio::test]
async fn a_finished_book_reflects_on_the_reading_it_just_finished() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    engine
        .storage()
        .update_progress(book, Some(490), Some(true))
        .await
        .unwrap();
    let readings = engine.storage().list_readings(book).await.unwrap();

    let note = engine.open_reflection(book, None).await.unwrap();
    let record = engine.storage().get_note(note.id).await.unwrap().unwrap();
    assert_eq!(record.reading_id, Some(readings[0].id));
    assert_eq!(
        engine.storage().list_readings(book).await.unwrap().len(),
        1,
        "reflecting on a finished book must not start a reread"
    );
}

/// A reread is a different reading, so it gets its own pair — and the first
/// one survives untouched, dates and all. That is what anchoring to a reading
/// rather than a book buys.
#[tokio::test]
async fn a_reread_gets_its_own_pair_and_the_first_survives() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;

    let first_reflection = engine.open_reflection(book, None).await.unwrap();
    let first_review = engine.open_review(book, None).await.unwrap();
    let record = engine
        .storage()
        .get_note(first_reflection.id)
        .await
        .unwrap()
        .unwrap();
    engine
        .update_note_body(&record, "First time through.")
        .await
        .unwrap();
    engine
        .storage()
        .update_progress(book, Some(490), Some(true))
        .await
        .unwrap();

    engine.storage().reread(book).await.unwrap();
    let second_reflection = engine.open_reflection(book, None).await.unwrap();
    let second_review = engine.open_review(book, None).await.unwrap();

    assert_ne!(second_reflection.id, first_reflection.id);
    assert_ne!(second_review.id, first_review.id);
    assert_ne!(second_reflection.file, first_reflection.file);
    // The titles are wikilink targets; a reread's must not collide with the
    // first reading's or a link would resolve to whichever came first.
    assert_ne!(second_reflection.title, first_reflection.title);

    let readings = engine.storage().list_readings(book).await.unwrap();
    assert_eq!(readings.len(), 2);
    let notes = engine.list_notes(Some(book), None).await.unwrap();
    assert_eq!(notes.len(), 4);
    assert_eq!(
        engine.note_body(&record).unwrap(),
        "First time through.",
        "the first reading's reflection is untouched by the second"
    );
    assert!(readings[0].finished_at.is_some());
}

#[tokio::test]
async fn an_explicit_reading_must_belong_to_the_book() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let other = engine
        .save_book(&Book {
            title: Some("1Q84".into()),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
        .unwrap();
    let stray = engine
        .storage()
        .open_reading(other, Some(1), "manual")
        .await
        .unwrap();

    assert!(matches!(
        engine.open_reflection(book, Some(stray)).await,
        Err(EngineError::InvalidInput(_))
    ));
    assert!(matches!(
        engine.open_reflection(book, Some(9_999)).await,
        Err(EngineError::NotFound(_))
    ));
}

/// The two are separate objects with separate bodies. A review is a rewrite for
/// a different audience, never a slice of the private one.
#[tokio::test]
async fn a_reflection_and_a_review_never_share_a_body() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;

    let reflection = engine.open_reflection(book, None).await.unwrap();
    let review = engine.open_review(book, None).await.unwrap();
    assert_ne!(reflection.id, review.id);
    assert_ne!(reflection.file, review.file);

    let r_rec = engine
        .storage()
        .get_note(reflection.id)
        .await
        .unwrap()
        .unwrap();
    let v_rec = engine.storage().get_note(review.id).await.unwrap().unwrap();
    engine
        .update_note_body(&r_rec, "Private: it made me think about my mother.")
        .await
        .unwrap();
    engine
        .update_note_body(&v_rec, "Public: a patient, generational novel.")
        .await
        .unwrap();

    assert_eq!(
        engine.note_body(&r_rec).unwrap(),
        "Private: it made me think about my mother."
    );
    assert_eq!(
        engine.note_body(&v_rec).unwrap(),
        "Public: a patient, generational novel."
    );
    assert_eq!(r_rec.reading_id, v_rec.reading_id);
}

/// The frontmatter is the vault's own record of the anchor, and the vault is
/// the source of the body — so an Obsidian user sees what this note belongs to.
#[tokio::test]
async fn the_vault_file_records_the_kind_and_the_reading() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let note = engine.open_reflection(book, None).await.unwrap();
    let record = engine.storage().get_note(note.id).await.unwrap().unwrap();

    let content = std::fs::read_to_string(&note.file).unwrap();
    assert!(content.contains("kind: reflection"), "{content}");
    assert!(
        content.contains(&format!("reading: {}", record.reading_id.unwrap())),
        "{content}"
    );
    assert!(content.contains("book-title: \"Pachinko\""), "{content}");
}

// ---- the graph ------------------------------------------------------------

/// The claim that decides the whole design: reflections are notes so that
/// `note_links` — the graph — carries them, and book-to-book connection runs
/// reflection-to-reflection. Asserted rather than assumed.
#[tokio::test]
async fn wikilinks_from_a_reflection_land_in_the_graph_and_back_resolve() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;

    let reflection = engine.open_reflection(book, None).await.unwrap();
    let record = engine
        .storage()
        .get_note(reflection.id)
        .await
        .unwrap()
        .unwrap();
    // Written after the note was opened — which is the ordinary case, and the
    // reason indexing edges only at creation would leave the hub edgeless.
    engine
        .update_note_body(
            &record,
            "Rhymes with [[Han]] and with [[Reflection: 1Q84]].",
        )
        .await
        .unwrap();

    let links = engine.storage().note_links(reflection.id).await.unwrap();
    assert_eq!(links.len(), 2);
    assert!(links.iter().all(|(_, to)| to.is_none()), "both dangle yet");

    // A note by that title appears later: the dangling edge resolves to it.
    let han = engine
        .create_note(NewNoteInput {
            kind: NoteKind::Note,
            title: Some("Han".into()),
            body: "The Korean concept of grief.".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let links = engine.storage().note_links(reflection.id).await.unwrap();
    assert_eq!(
        links.iter().find(|(t, _)| t == "Han").unwrap().1,
        Some(han.id)
    );

    // Reflection to reflection is the book-to-book edge, and it resolves the
    // same way — the other book's reflection is just another note.
    let other = engine
        .save_book(&Book {
            title: Some("1Q84".into()),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
        .unwrap();
    let other_reflection = engine.open_reflection(other, None).await.unwrap();
    assert_eq!(other_reflection.title, "Reflection: 1Q84");
    let links = engine.storage().note_links(reflection.id).await.unwrap();
    assert_eq!(
        links
            .iter()
            .find(|(t, _)| t == "Reflection: 1Q84")
            .unwrap()
            .1,
        Some(other_reflection.id)
    );
}

/// A link the user deleted has to leave the graph too, or the hub accumulates
/// edges it no longer claims.
#[tokio::test]
async fn rewriting_a_body_replaces_its_edges_rather_than_adding_to_them() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let note = engine.open_reflection(book, None).await.unwrap();
    let record = engine.storage().get_note(note.id).await.unwrap().unwrap();

    engine
        .update_note_body(&record, "See [[Han]] and [[Diaspora]].")
        .await
        .unwrap();
    assert_eq!(engine.storage().note_links(note.id).await.unwrap().len(), 2);

    engine
        .update_note_body(&record, "See [[Han]].")
        .await
        .unwrap();
    let links = engine.storage().note_links(note.id).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].0, "Han");
}

/// The FTS body cache follows the vault file, so a reflection is searchable
/// like every other note.
#[tokio::test]
async fn a_reflection_is_searchable() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let note = engine.open_reflection(book, None).await.unwrap();
    let record = engine.storage().get_note(note.id).await.unwrap().unwrap();
    engine
        .update_note_body(&record, "The register shifts at the boarding house.")
        .await
        .unwrap();

    let hits = engine.search_notes("boarding", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].note.id, note.id);
}

// ---- citations ------------------------------------------------------------

/// By reference, so a device refresh — which rewrites `ko_note`, colour,
/// chapter and page toward the sidecar — leaves every citation standing.
#[tokio::test]
async fn citations_survive_a_device_refresh_and_cascade_on_delete() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let hid = engine
        .storage()
        .insert_highlight(
            book,
            &highlight("History has failed us", "2026-01-05 21:14:08"),
        )
        .await
        .unwrap()
        .expect("newly inserted");

    let review = engine.open_review(book, None).await.unwrap();
    engine.cite(review.id, hid).await.unwrap();
    // Idempotent: citing the same highlight twice is one citation.
    engine.cite(review.id, hid).await.unwrap();
    assert_eq!(engine.citations_for(review.id).await.unwrap().len(), 1);

    // The device says something different about its own fields.
    let mut refreshed = highlight("History has failed us", "2026-01-05 21:14:08");
    refreshed.note = Some("added on the device".into());
    refreshed.color = Some("blue".into());
    assert!(
        engine
            .storage()
            .refresh_device_fields(book, &refreshed)
            .await
            .unwrap()
    );

    let cited = engine.citations_for(review.id).await.unwrap();
    assert_eq!(cited.len(), 1, "the citation is by id, so it holds");
    assert_eq!(cited[0].id, hid);
    assert_eq!(cited[0].ko_note.as_deref(), Some("added on the device"));

    // A deleted highlight takes its citations with it — a citation of nothing
    // is worse than a missing one. Reached through the book, which is the only
    // way a highlight is ever deleted today.
    engine.delete_book(book).await.unwrap();
    assert!(
        engine.citations_for(review.id).await.unwrap().is_empty(),
        "citations must cascade rather than dangle"
    );
    assert!(
        engine
            .storage()
            .get_note(review.id)
            .await
            .unwrap()
            .is_some(),
        "the review itself survives its book, as every note does"
    );
}

// ---- ratings --------------------------------------------------------------

/// The rating is the review's, and a reflection must not borrow the column: a
/// private score is a different number, and this is the seam that keeps it so.
#[tokio::test]
async fn only_a_review_can_be_rated() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let reflection = engine.open_reflection(book, None).await.unwrap();
    let review = engine.open_review(book, None).await.unwrap();

    assert!(matches!(
        engine.set_rating(reflection.id, 4.0).await,
        Err(EngineError::InvalidInput(_))
    ));
    engine.set_rating(review.id, 4.0).await.unwrap();
    assert_eq!(engine.goodreads_rating(review.id).await.unwrap(), Some(4));
}

/// Goodreads takes an integer 0–5. A value the user has never mapped reports
/// itself; it is never rounded to the nearest star.
#[tokio::test]
async fn an_unmapped_value_reports_rather_than_rounds() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let review = engine.open_review(book, None).await.unwrap();

    engine.set_rating(review.id, 4.5).await.unwrap();
    match engine.goodreads_rating(review.id).await {
        Err(EngineError::UnmappedRating { value, scale }) => {
            assert_eq!(value, 4.5);
            assert_eq!(scale, "default");
        }
        other => panic!("expected an unmapped-rating report, got {other:?}"),
    }

    // The raw value is what was stored, so the user's own decision changes what
    // it exports as — without touching the review.
    let scale = engine
        .storage()
        .active_rating_scale()
        .await
        .unwrap()
        .unwrap();
    engine.storage().map_rating(&scale, 4.5, 5).await.unwrap();
    assert_eq!(engine.goodreads_rating(review.id).await.unwrap(), Some(5));

    let stored = engine
        .storage()
        .review_rating(review.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.value, 4.5, "the raw value is what round-trips");
}

#[tokio::test]
async fn an_unrated_review_is_not_an_error() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let review = engine.open_review(book, None).await.unwrap();
    assert_eq!(engine.goodreads_rating(review.id).await.unwrap(), None);
}

/// The scale is the user's, and a value off its grid is refused rather than
/// snapped — snapping is the same sin as rounding, one layer down.
#[tokio::test]
async fn a_rating_must_sit_on_the_scale() {
    let (_tmp, engine) = engine().await;
    let book = seeded(&engine).await;
    let review = engine.open_review(book, None).await.unwrap();

    assert!(matches!(
        engine.set_rating(review.id, 4.3).await,
        Err(EngineError::InvalidInput(_))
    ));
    assert!(matches!(
        engine.set_rating(review.id, 7.0).await,
        Err(EngineError::InvalidInput(_))
    ));

    engine
        .storage()
        .put_rating_scale("tenths", 0.0, 10.0, 0.1)
        .await
        .unwrap();
    engine.set_rating(review.id, 7.3).await.unwrap();
    let stored = engine
        .storage()
        .review_rating(review.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.value, 7.3);
    assert_eq!(stored.scale.name, "tenths");
}
