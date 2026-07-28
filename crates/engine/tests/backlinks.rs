//! Walking the note graph in both directions (item 9a).
//!
//! `note_links` has been written since `0001_init.sql` and read almost nowhere:
//! `open_anchored` pulled a note's outgoing titles to fill `CreatedNote.links`
//! and that was all. These tests are about the other direction — what links
//! *here* — and about the one property that decides the shape of the query.
//!
//! Reflections rather than plain notes wherever the test allows it: a reflection
//! is what the graph is meant to hub on, so the interesting edges are between
//! reflections, and a test on notes alone would not exercise the titles the
//! reread suffix produces.

mod common;

use common::{engine, seed_book};
use readingbuddy::{NewNoteInput, NoteKind};

/// A note's inbound and outbound sets are genuinely different collections, and a
/// note that links to itself appears once on each side rather than twice on
/// either.
///
/// The self-link is not a curiosity. `note_links` is keyed
/// `(from_note, target_title)`, so the row exists exactly once, and both queries
/// select it — a `JOIN` written carelessly (or a union with the dangling case)
/// is how it would come back doubled.
#[tokio::test]
async fn inbound_and_outbound_are_different_sets_and_a_self_link_counts_once() {
    let (_tmp, engine) = engine().await;
    let pachinko = seed_book(&engine, "Pachinko").await;
    let millionaires = seed_book(&engine, "Free Food for Millionaires").await;

    let a = engine.open_reflection(pachinko, None).await.unwrap();
    let b = engine.open_reflection(millionaires, None).await.unwrap();
    let a_rec = engine.storage.get_note(a.id).await.unwrap().unwrap();
    let b_rec = engine.storage.get_note(b.id).await.unwrap().unwrap();

    // A points at B *and* at itself; B points only at A.
    engine
        .update_note_body(
            &a_rec,
            &format!(
                "Same preoccupations as [[{}]]. Restating [[{}]] to see it again.",
                b.title, a.title
            ),
        )
        .await
        .unwrap();
    engine
        .update_note_body(&b_rec, &format!("The later book: [[{}]].", a.title))
        .await
        .unwrap();

    let out: Vec<_> = engine
        .outgoing_links(a.id)
        .await
        .unwrap()
        .into_iter()
        .map(|l| (l.target_title, l.to.map(|n| n.id)))
        .collect();
    assert_eq!(
        out,
        vec![(b.title.clone(), Some(b.id)), (a.title.clone(), Some(a.id)),],
        "outgoing follows the body, in the order the body wrote it"
    );

    let inbound: Vec<i64> = engine
        .backlinks(a.id)
        .await
        .unwrap()
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(inbound.len(), 2, "B links here, and so does A itself");
    assert!(inbound.contains(&b.id));
    assert_eq!(
        inbound.iter().filter(|id| **id == a.id).count(),
        1,
        "a self-link is one edge, not two"
    );

    // B's own sides are the mirror image, which is what makes them different
    // sets rather than the same list read twice.
    assert_eq!(engine.backlinks(b.id).await.unwrap().len(), 1);
    assert_eq!(engine.outgoing_links(b.id).await.unwrap().len(), 1);
}

/// **The property that decides the shape of `backlinks`.**
///
/// `write_links` back-resolves dangling edges when their target is written, so
/// `to_note` is complete once the target exists — which is what lets `backlinks`
/// be a plain `WHERE to_note = ?` instead of also unioning the
/// dangling-by-title case. An unexplained `OR` in that query is the kind of
/// thing nobody later dares delete, so it is worth proving rather than assuming.
///
/// The forward reference is written *first*, before its target exists at all,
/// and it names a reflection — whose title the writer has to predict, which is
/// exactly the case the reread suffix (`Reflection: X (2)`) exists to keep
/// unambiguous.
#[tokio::test]
async fn a_forward_reference_resolves_when_its_target_is_written() {
    let (_tmp, engine) = engine().await;
    let pachinko = seed_book(&engine, "Pachinko").await;

    let early = engine
        .create_note(NewNoteInput {
            kind: NoteKind::Note,
            title: Some("Diaspora, generally".into()),
            body: "Whatever I end up saying in [[Reflection: Pachinko]] starts here.".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    // Dangling until the target exists: kept as text, reported as text.
    let out = engine.outgoing_links(early.id).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].target_title, "Reflection: Pachinko");
    assert!(out[0].to.is_none(), "nothing to resolve to yet");

    let reflection = engine.open_reflection(pachinko, None).await.unwrap();
    assert_eq!(reflection.title, "Reflection: Pachinko");

    // Writing the target back-resolved the older edge — so the backlink is
    // reachable through `to_note` alone.
    let inbound: Vec<i64> = engine
        .backlinks(reflection.id)
        .await
        .unwrap()
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(
        inbound,
        vec![early.id],
        "a plain `WHERE to_note = ?` has to find the forward reference"
    );
    let out = engine.outgoing_links(early.id).await.unwrap();
    assert_eq!(
        out[0].to.as_ref().map(|n| n.id),
        Some(reflection.id),
        "and both directions must agree the edge resolved"
    );
}

/// Deleting the target degrades the edge to dangling text rather than losing it.
///
/// `note_links.to_note` is `ON DELETE SET NULL` and `from_note` is `CASCADE`,
/// which is deliberate and asymmetric: the note that *wrote* the link owns the
/// row, so the link survives its target and goes back to being what the body
/// says. Recreating a note under the same title picks it straight back up.
#[tokio::test]
async fn deleting_the_target_leaves_the_link_as_text() {
    let (_tmp, engine) = engine().await;
    let pachinko = seed_book(&engine, "Pachinko").await;

    let target = engine
        .create_note(NewNoteInput {
            kind: NoteKind::Note,
            title: Some("Han".into()),
            body: "Korean concept of grief.".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let source = engine
        .create_note(NewNoteInput {
            book_id: Some(pachinko),
            kind: NoteKind::Note,
            title: Some("Sunja".into()),
            body: "Her whole life is [[Han]].".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(engine.backlinks(target.id).await.unwrap().len(), 1);

    let target_rec = engine.storage.get_note(target.id).await.unwrap().unwrap();
    engine.delete_note(&target_rec).await.unwrap();

    let out = engine.outgoing_links(source.id).await.unwrap();
    assert_eq!(out.len(), 1, "the edge is the source's, and it stays");
    assert_eq!(out[0].target_title, "Han");
    assert!(out[0].to.is_none(), "it went back to being text");

    // And a new note under that title takes the edge over, which is the same
    // back-resolution the forward-reference case relies on.
    let again = engine
        .create_note(NewNoteInput {
            kind: NoteKind::Note,
            title: Some("Han".into()),
            body: "Second attempt at the same idea.".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let inbound: Vec<i64> = engine
        .backlinks(again.id)
        .await
        .unwrap()
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(inbound, vec![source.id]);
}

/// The one case where back-resolution does *not* reach an edge — and the reason
/// `backlinks` still does not union the dangling-by-title branch.
///
/// `notes.title` is not unique (two library duplicates of one book each get a
/// `Reflection: <title>`, and they can outlive the merge), so an edge can resolve
/// to one of two same-titled notes. Delete that one and the edge goes `NULL`
/// while a note with that title still exists: nothing re-runs `write_links` for
/// a title merely because a *different* note was deleted.
///
/// What matters is that both directions still say the same thing — the source
/// reports dangling text, the survivor reports no inbound link. A union in
/// `backlinks` would resolve this edge from one end only, and the pane would
/// claim an inbound link the linking note denies writing. Agreeing is worth more
/// than guessing, so this is pinned rather than fixed.
#[tokio::test]
async fn a_title_shared_by_two_notes_is_where_back_resolution_stops() {
    let (_tmp, engine) = engine().await;

    let first = engine
        .create_note(NewNoteInput {
            kind: NoteKind::Note,
            title: Some("Han".into()),
            body: "One reading of it.".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let second = engine
        .create_note(NewNoteInput {
            kind: NoteKind::Note,
            title: Some("Han".into()),
            body: "Another, under the same name.".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let source = engine
        .create_note(NewNoteInput {
            kind: NoteKind::Note,
            title: Some("Sunja".into()),
            body: "Her whole life is [[Han]].".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let out = engine.outgoing_links(source.id).await.unwrap();
    assert_eq!(
        out[0].to.as_ref().map(|n| n.id),
        Some(first.id),
        "an ambiguous title resolves to one of them, and it is the older"
    );

    let first_rec = engine.storage.get_note(first.id).await.unwrap().unwrap();
    engine.delete_note(&first_rec).await.unwrap();

    let out = engine.outgoing_links(source.id).await.unwrap();
    assert!(
        out[0].to.is_none(),
        "the edge dangles again even though a note with that title survives"
    );
    assert!(
        engine.backlinks(second.id).await.unwrap().is_empty(),
        "and the surviving namesake must report the same thing, not a link the \
         source no longer claims to have"
    );
}

/// A body rewritten without a link loses the backlink too.
///
/// `set_note_links` replaces rather than merges, and `workflows.rs` already
/// leans on that from the outgoing side. This is the same rule seen from the
/// other end, which is where a stale edge would actually be noticed: the pane
/// on the *target* would keep claiming an inbound link from a note that no
/// longer mentions it.
#[tokio::test]
async fn a_rewritten_body_drops_the_backlink_it_no_longer_writes() {
    let (_tmp, engine) = engine().await;
    let pachinko = seed_book(&engine, "Pachinko").await;
    let millionaires = seed_book(&engine, "Free Food for Millionaires").await;

    let a = engine.open_reflection(pachinko, None).await.unwrap();
    let b = engine.open_reflection(millionaires, None).await.unwrap();
    let a_rec = engine.storage.get_note(a.id).await.unwrap().unwrap();

    engine
        .update_note_body(&a_rec, &format!("Twenty years earlier: [[{}]].", b.title))
        .await
        .unwrap();
    assert_eq!(engine.backlinks(b.id).await.unwrap().len(), 1);

    engine
        .update_note_body(&a_rec, "On reflection, they are not much alike.")
        .await
        .unwrap();
    assert!(
        engine.backlinks(b.id).await.unwrap().is_empty(),
        "a link the user deleted has to leave the inbound side too"
    );
    assert!(engine.outgoing_links(a.id).await.unwrap().is_empty());
}
