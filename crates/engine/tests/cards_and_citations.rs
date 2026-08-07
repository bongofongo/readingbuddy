//! Items 45 and 46 through the facade: a card can be made, and "which passages
//! are already cited" can be asked once for a page of notes.
//!
//! Offline, `sqlite::memory:` plus a `TempDir` vault, like every other suite
//! here.

use readingbuddy::{Engine, EngineError};

mod common;
use common::{engine, highlight, seed_book};

/// Insert `n` highlights into `book` and return their ids in insertion order.
async fn highlights(engine: &Engine, book: i64, texts: &[&str]) -> Vec<i64> {
    let mut ids = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        // Distinct device stamps, so `CITATION_ORDER`'s first two keys are
        // meaningful rather than a wall of ties.
        let when = format!("2026-01-{:02} 09:00:00", i + 1);
        ids.push(
            engine
                .storage()
                .insert_highlight(book, &highlight(text, &when))
                .await
                .unwrap()
                .expect("newly inserted"),
        );
    }
    ids
}

// ---- item 45: a flashcard can be made --------------------------------------

/// The point of the item, end to end: a card minted through the facade comes
/// back out of both list queries carrying the handles that let a frontend show
/// it beside the passage it was taken from.
#[tokio::test]
async fn a_card_round_trips_its_book_and_its_passage() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;
    let hs = highlights(&engine, book, &["pachinko"]).await;

    assert!(
        engine
            .create_flashcard(book, Some(hs[0]), "pachinko", Some("Ch 1"))
            .await
            .unwrap(),
        "the first card is new"
    );

    for card in [
        engine.list_flashcards(true).await.unwrap().remove(0),
        engine
            .list_flashcards_for_book(book)
            .await
            .unwrap()
            .remove(0),
    ] {
        assert_eq!(card.book_id, book);
        assert_eq!(card.highlight_id, Some(hs[0]));
        assert_eq!(card.book_title, "Pachinko");
        assert_eq!(card.context.as_deref(), Some("Ch 1"));
    }
}

/// *You already had this one* and *a card now exists* are different facts, and
/// the second attempt must not quietly repoint the first card at a different
/// passage — `UNIQUE(book_id, word)` dedupes, and `DO NOTHING` is what makes
/// the returned bool mean what it says.
#[tokio::test]
async fn a_repeat_word_reports_not_new_and_leaves_the_first_card_alone() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;
    let hs = highlights(&engine, book, &["pachinko", "hanko"]).await;

    assert!(
        engine
            .create_flashcard(book, Some(hs[0]), "pachinko", Some("first"))
            .await
            .unwrap()
    );
    assert!(
        !engine
            .create_flashcard(book, Some(hs[1]), "pachinko", Some("second"))
            .await
            .unwrap(),
        "the second is not new"
    );

    let cards = engine.list_flashcards_for_book(book).await.unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].context.as_deref(), Some("first"));
    assert_eq!(
        cards[0].highlight_id,
        Some(hs[0]),
        "a repeat capture must not move the card onto another passage"
    );
}

/// Handles do not cross. Nothing in the schema stops a client pairing a book
/// with another book's highlight, and a card that took the pair on trust would
/// sit beside a passage from somewhere else for ever, with nothing on screen
/// looking wrong.
#[tokio::test]
async fn a_card_cannot_be_anchored_to_another_books_passage() {
    let (_tmp, engine) = engine().await;
    let pachinko = seed_book(&engine, "Pachinko").await;
    let station = seed_book(&engine, "Station Eleven").await;
    let elsewhere = highlights(&engine, station, &["survival"]).await;

    assert!(matches!(
        engine
            .create_flashcard(pachinko, Some(elsewhere[0]), "survival", None)
            .await,
        Err(EngineError::InvalidInput(_))
    ));
    assert!(
        engine
            .list_flashcards_for_book(pachinko)
            .await
            .unwrap()
            .is_empty(),
        "a refused card writes nothing"
    );

    // The neighbouring failures are their own answers rather than a raw
    // foreign-key error out of the driver: a frontend offering a list can
    // always name a row another pane has just deleted.
    assert!(matches!(
        engine
            .create_flashcard(pachinko, Some(9_999), "ghost", None)
            .await,
        Err(EngineError::NotFound(_))
    ));
    assert!(matches!(
        engine.create_flashcard(9_999, None, "ghost", None).await,
        Err(EngineError::NotFound(_))
    ));
    assert!(matches!(
        engine.create_flashcard(pachinko, None, "   ", None).await,
        Err(EngineError::InvalidInput(_))
    ));
}

/// Unanchored is ordinary — a card typed rather than captured — and the word is
/// trimmed on the way in, because `UNIQUE(book_id, word)` is the dedup and
/// `" mot"` is not a second word.
#[tokio::test]
async fn a_card_needs_no_passage_and_its_word_is_trimmed() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;

    assert!(
        engine
            .create_flashcard(book, None, "  mot  ", None)
            .await
            .unwrap()
    );
    assert!(
        !engine
            .create_flashcard(book, None, "mot", None)
            .await
            .unwrap(),
        "the trim is what makes this the same card"
    );

    let cards = engine.list_flashcards_for_book(book).await.unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].word, "mot");
    assert_eq!(cards[0].highlight_id, None);
}

// ---- item 46: which passages are already cited -----------------------------

/// The agreement that makes the batch safe to prefer: for **any** set of note
/// ids, its answer for each note is exactly what `citations_for` returns for
/// that note alone — same members, same order.
///
/// Without this the batch is a second implementation of a question the engine
/// already answers, and the two would drift the first time either query
/// changed.
#[tokio::test]
async fn the_batch_and_the_single_note_call_cannot_disagree() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;
    let hs = highlights(
        &engine,
        book,
        &["history has failed us", "pachinko", "a woman's lot", "home"],
    )
    .await;

    let reflection = engine.open_reflection(book, None).await.unwrap();
    let review = engine.open_review(book, None).await.unwrap();
    // Cited out of reading order on purpose: the reply is ordered by the book,
    // never by when the citation was made.
    for h in [hs[3], hs[0], hs[2]] {
        engine.cite(reflection.id, h).await.unwrap();
    }
    engine.cite(review.id, hs[1]).await.unwrap();

    let asked = [review.id, reflection.id, 9_999, reflection.id];
    let batch = engine.citations_for_notes(&asked).await.unwrap();
    assert_eq!(batch.len(), asked.len());
    for (row, note_id) in batch.iter().zip(asked) {
        assert_eq!(row.note_id, note_id);
        let alone: Vec<i64> = engine
            .citations_for(note_id)
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.id)
            .collect();
        assert_eq!(row.highlight_ids, alone, "note {note_id}");
    }
}

/// One row per requested id, in the order asked, including for a note that
/// cites nothing and for an id that is not a note at all — so a caller can zip
/// the reply against the page it already holds, and "nothing behind this one"
/// is an answer rather than a missing row.
#[tokio::test]
async fn every_requested_note_gets_a_row_in_the_order_asked() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;
    let hs = highlights(&engine, book, &["pachinko"]).await;

    let cites = engine.open_reflection(book, None).await.unwrap();
    let silent = engine.open_review(book, None).await.unwrap();
    engine.cite(cites.id, hs[0]).await.unwrap();

    let asked = [silent.id, 4_242, cites.id, silent.id];
    let batch = engine.citations_for_notes(&asked).await.unwrap();

    assert_eq!(
        batch.iter().map(|r| r.note_id).collect::<Vec<_>>(),
        asked.to_vec(),
        "duplicates included, order preserved"
    );
    assert!(batch[0].highlight_ids.is_empty(), "cites nothing");
    assert!(batch[1].highlight_ids.is_empty(), "not a note at all");
    assert_eq!(batch[2].highlight_ids, vec![hs[0]]);
    assert_eq!(batch[3].highlight_ids, Vec::<i64>::new());

    assert!(
        engine.citations_for_notes(&[]).await.unwrap().is_empty(),
        "nothing asked, nothing answered"
    );
}

/// More ids than one statement is allowed to bind, so the split is crossed
/// three times over: 1,203 is more than two chunks and not a multiple of one.
///
/// **This does not prove the chunk exists** — measured, not assumed: with the
/// split removed the test still passes, because the SQLite sqlx bundles binds
/// 32,766 parameters and only an older build refuses. What it proves is that
/// crossing the boundary does not lose, duplicate or reorder a row, which is
/// the bug a *present* chunk introduces. That the chunk is there at all is
/// `no_batch_statement_exceeds_the_parameter_ceiling`, in `storage/notes.rs`,
/// which reads the generated SQL for item 18's reason.
#[tokio::test]
async fn more_ids_than_one_statement_can_bind() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;
    let hs = highlights(&engine, book, &["pachinko"]).await;

    let real = engine.open_reflection(book, None).await.unwrap();
    engine.cite(real.id, hs[0]).await.unwrap();

    // One real note among 1,202 ids that are not notes. The absent ones still
    // owe a row each, which is what makes the count assertion meaningful.
    let mut asked: Vec<i64> = (100_000..101_202).collect();
    asked.insert(700, real.id);

    let batch = engine.citations_for_notes(&asked).await.unwrap();
    assert_eq!(batch.len(), asked.len());
    assert_eq!(batch.iter().map(|r| r.note_id).collect::<Vec<_>>(), asked);
    assert_eq!(batch[700].highlight_ids, vec![hs[0]]);
    assert_eq!(
        batch.iter().filter(|r| !r.highlight_ids.is_empty()).count(),
        1,
    );
}

/// A citation is by reference, so the batch is live in exactly the way
/// `citations_for` is: uncite one and the mark goes; delete the book and the
/// cascade takes the rest, leaving a row that says *nothing* rather than a
/// missing row.
#[tokio::test]
async fn the_batch_follows_the_citation_rather_than_a_copy_of_it() {
    let (_tmp, engine) = engine().await;
    let book = seed_book(&engine, "Pachinko").await;
    let hs = highlights(&engine, book, &["pachinko", "hanko"]).await;
    let note = engine.open_reflection(book, None).await.unwrap();
    engine.cite(note.id, hs[0]).await.unwrap();
    engine.cite(note.id, hs[1]).await.unwrap();

    assert_eq!(
        engine.citations_for_notes(&[note.id]).await.unwrap()[0]
            .highlight_ids
            .len(),
        2
    );

    assert!(engine.uncite(note.id, hs[0]).await.unwrap());
    assert_eq!(
        engine.citations_for_notes(&[note.id]).await.unwrap()[0].highlight_ids,
        vec![hs[1]]
    );

    engine.delete_book(book).await.unwrap();
    let after = engine.citations_for_notes(&[note.id]).await.unwrap();
    assert_eq!(after.len(), 1, "still one row per id asked");
    assert!(after[0].highlight_ids.is_empty());
}
