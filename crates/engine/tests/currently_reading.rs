//! What you are reading, through the facade — the surface the home screen uses.
//!
//! The SQL itself is exercised beside the query in `storage/readings.rs`, where
//! the pool is reachable and a `last_modified` can be set by hand. What is
//! tested here is the pair of promises a frontend actually leans on: the facade
//! answers without reaching into `engine.storage`, and opening a reflection
//! hands back the record every editor path takes.
//!
//! Offline, `sqlite::memory:` plus a `TempDir` vault, and nothing left behind.

mod common;
use common::{engine, seed_book};

/// The home screen's list: books with a reading open, and only those. A book
/// merely added to the library is not something you are reading, and a finished
/// one has stopped being one.
#[tokio::test]
async fn the_facade_lists_what_is_open_and_nothing_else() {
    let (_tmp, engine) = engine().await;
    let pachinko = seed_book(&engine, "Pachinko").await;
    let kokoro = seed_book(&engine, "Kokoro").await;

    assert!(
        engine.currently_reading(20).await.unwrap().is_empty(),
        "an untouched library is an empty home screen, not a full one"
    );

    engine
        .storage()
        .update_progress(pachinko, Some(120), None)
        .await
        .unwrap();
    let open = engine.currently_reading(20).await.unwrap();
    assert_eq!(open.len(), 1);
    let (book, reading) = &open[0];
    assert_eq!(book.id, Some(pachinko));
    assert_eq!(reading.book_id, pachinko);
    assert_eq!(reading.finished_at, None);
    assert!(!open.iter().any(|(b, _)| b.id == Some(kokoro)));

    engine
        .storage()
        .update_progress(pachinko, None, Some(true))
        .await
        .unwrap();
    assert!(engine.currently_reading(20).await.unwrap().is_empty());

    // A reread is how a finished book comes back, and it comes back as its own
    // reading rather than the closed one the book still projects.
    let second = engine.storage().reread(pachinko).await.unwrap();
    let open = engine.currently_reading(20).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].1.id, second);
}

/// The row the screen draws has to be internally consistent: the progress bar
/// comes off `Book`'s projections and the state off the `Reading` beside it, and
/// if those two ever resolved "current" differently nothing on screen would look
/// wrong.
#[tokio::test]
async fn the_book_and_its_reading_are_the_same_reading() {
    let (_tmp, engine) = engine().await;
    let book_id = seed_book(&engine, "Pachinko").await;
    engine
        .storage()
        .update_progress(book_id, Some(490), Some(true))
        .await
        .unwrap();
    engine.storage().reread(book_id).await.unwrap();
    engine
        .storage()
        .update_progress(book_id, Some(40), None)
        .await
        .unwrap();

    let (book, reading) = engine.currently_reading(20).await.unwrap().remove(0);
    assert_eq!(book.current_page, reading.current_page);
    assert_eq!(book.current_page, Some(40));
    assert_eq!(book.date_started, reading.started_at);
    assert_eq!(book.date_finished, reading.finished_at);
    assert!(!book.finished);
}

/// The home screen's whole action. `open_reflection` returns a `CreatedNote`,
/// which is the wrong shape for an editor — every editing path in both frontends
/// takes a `NoteRecord` — so the record-returning twin has to be the *same note*,
/// not a second one.
#[tokio::test]
async fn the_record_wrapper_opens_the_same_reflection() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;

    let created = engine.open_reflection(book, None).await.unwrap();
    let record = engine.open_reflection_record(book, None).await.unwrap();
    assert_eq!(record.id, created.id);
    assert_eq!(record.title, created.title);
    assert_eq!(record.kind, "reflection");
    assert_eq!(engine.list_notes(Some(book)).await.unwrap().len(), 1);

    // And it is a record an editor can actually use: read the body, write it
    // back, read it again.
    assert_eq!(engine.note_body(&record).unwrap(), "");
    engine
        .update_note_body(&record, "Sunja's dignity, chapter by chapter.")
        .await
        .unwrap();

    // Accretion holds through the wrapper too — the second call must not blank
    // what the first one wrote.
    let again = engine.open_reflection_record(book, None).await.unwrap();
    assert_eq!(again.id, record.id);
    assert_eq!(
        engine.note_body(&again).unwrap(),
        "Sunja's dignity, chapter by chapter."
    );
}

/// The review twin, and the fact that it is a *different* note: a review is a
/// rewrite for a different audience, never a slice of the private reflection.
#[tokio::test]
async fn the_review_twin_is_its_own_note() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;

    let reflection = engine.open_reflection_record(book, None).await.unwrap();
    let review = engine.open_review_record(book, None).await.unwrap();
    assert_ne!(review.id, reflection.id);
    assert_eq!(review.kind, "review");
    assert_eq!(review.reading_id, reflection.reading_id);
    assert_eq!(
        engine.open_review_record(book, None).await.unwrap().id,
        review.id,
        "one review per reading, and reopening finds it"
    );
    // Two notes on one reading, and the two openings did not start two readings.
    assert_eq!(engine.list_notes(Some(book)).await.unwrap().len(), 2);
    assert_eq!(engine.storage().list_readings(book).await.unwrap().len(), 1);
}

/// A reflection opened from the home screen must not start a reread of a book
/// the user finished — `open_anchored` resolves through `ensure_reading`, and
/// the wrapper inherits that rather than deciding for itself.
#[tokio::test]
async fn opening_from_a_finished_book_does_not_reopen_it() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;
    engine
        .storage()
        .update_progress(book, Some(490), Some(true))
        .await
        .unwrap();

    let record = engine.open_reflection_record(book, None).await.unwrap();
    let readings = engine.storage().list_readings(book).await.unwrap();
    assert_eq!(readings.len(), 1);
    assert_eq!(record.reading_id, Some(readings[0].id));
    assert!(
        engine.currently_reading(20).await.unwrap().is_empty(),
        "reflecting on a finished book must not put it back on the shelf"
    );
}
