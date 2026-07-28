//! The API crate against a real engine, in-process.
//!
//! What these assert is not "the engine works" — the engine's own suite does
//! that — but the two claims item 14 rests on:
//!
//! 1. **The typed method and the dispatch arm are the same call.** If they ever
//!    diverge, iOS (typed, in-process) and a daemon client (dispatched) get
//!    different behaviour from the same request, and the boundary has stopped
//!    being the API.
//! 2. **No domain handle crosses the seam.** Everything is addressed by id, and
//!    the row is re-read on this side.

use std::path::PathBuf;
use std::sync::Arc;

use readingbuddy::{Engine, EngineConfig};
use readingbuddy_api::{
    Api, ApiError, BookDto, ErrorCode, NewNoteDto, NoteKindDto, Outcome, Request, Response,
};

/// A library in a tempdir with an in-memory database, like every other suite
/// here. The `TempDir` comes back so the vault outlives the test body.
async fn api() -> (Api, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = EngineConfig {
        db_url: "sqlite::memory:".into(),
        images_dir: tmp.path().join("images"),
        files_dir: tmp.path().join("files"),
        vault_dir: tmp.path().join("vault"),
        log_dir: tmp.path().join("logs"),
        google_api_key: None,
        calibre_bin_dir: None,
    };
    let engine = Engine::open(config).await.expect("engine");
    (Api::new(Arc::new(engine)), tmp)
}

async fn seed(api: &Api) -> i64 {
    api.save_book(BookDto {
        title: Some("Station Eleven".into()),
        authors: vec!["Emily St. John Mandel".into()],
        isbn_13: Some("9781447268963".into()),
        page_count: Some(333),
        ..Default::default()
    })
    .await
    .expect("save")
    .id
    .expect("saved book has an id")
}

fn ok(response: Result<Response, ApiError>) -> Response {
    response.expect("dispatch succeeded")
}

/// The daemon holds one `Api` and hands `&self` to every connection at once, so
/// this is a compile-time requirement rather than a nicety. It is why
/// `Engine::set_google_api_key` had to stop taking `&mut self`.
#[test]
fn the_api_can_be_shared_across_connections() {
    fn assert_send_sync<T: Send + Sync + Clone + 'static>() {}
    assert_send_sync::<Api>();
}

/// Claim 1, for a read: the typed method and the dispatch arm must produce the
/// same answer, because `dispatch` is meant to be pure fan-out.
#[tokio::test]
async fn dispatch_and_the_typed_method_agree() {
    let (api, _tmp) = api().await;
    let id = seed(&api).await;

    let typed = api.get_book(id).await.unwrap();
    match ok(api.dispatch(Request::GetBook { id }).await) {
        Response::Book(dispatched) => assert_eq!(dispatched, typed),
        other => panic!("{other:?}"),
    }

    let typed = api.list_books(10, Default::default()).await.unwrap();
    match ok(api
        .dispatch(Request::ListBooks {
            limit: 10,
            sort: Default::default(),
        })
        .await)
    {
        Response::Books(dispatched) => assert_eq!(dispatched, typed),
        other => panic!("{other:?}"),
    }
}

/// And for a write. A rule that lived in the dispatch arm would show up here as
/// the two paths disagreeing about what a call did.
#[tokio::test]
async fn dispatch_and_the_typed_method_agree_on_a_write() {
    let (api, _tmp) = api().await;
    let id = seed(&api).await;

    let typed = api.update_progress(id, Some(40), None).await.unwrap();
    assert_eq!(typed.current_page, Some(40));

    match ok(api
        .dispatch(Request::UpdateProgress {
            book_id: id,
            page: Some(41),
            finished: None,
        })
        .await)
    {
        Response::Book(Some(b)) => assert_eq!(b.current_page, Some(41)),
        other => panic!("{other:?}"),
    }
    // One reading, not two: the second call must have gone through the same
    // `update_progress` the first did.
    assert_eq!(api.list_readings(id).await.unwrap().len(), 1);
}

/// Claim 2. `Engine::update_note_body` takes a `&NoteRecord`; nothing here
/// does, and a client never holds one.
#[tokio::test]
async fn a_note_is_written_and_read_back_by_id_alone() {
    let (api, _tmp) = api().await;
    let book_id = seed(&api).await;

    let created = api
        .create_note(NewNoteDto {
            book_id: Some(book_id),
            body: "A note about the [[Symphony]].".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(
        api.note_body(created.id).await.unwrap(),
        "A note about the [[Symphony]]."
    );
    api.update_note_body(created.id, "Rewritten, still about the [[Symphony]].")
        .await
        .unwrap();
    assert!(
        api.note_body(created.id)
            .await
            .unwrap()
            .starts_with("Rewritten")
    );

    // The edge survived the rewrite, which is what re-indexing on update buys.
    let links = api.outgoing_links(created.id).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target_title, "Symphony");
    // Nobody has written that note, so the target dangles — a forward
    // reference, carried as text rather than dropped.
    assert!(links[0].note.is_none());

    api.delete_note(created.id).await.unwrap();
    assert!(api.get_note(created.id).await.unwrap().is_none());
}

/// A note id that names nothing must be `not_found` rather than a panic or a
/// database error leaking through as `internal`.
#[tokio::test]
async fn an_absent_note_is_not_found_and_not_something_vaguer() {
    let (api, _tmp) = api().await;
    let err = api.note_body(4242).await.expect_err("no such note");
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(err.message.contains("4242"), "{}", err.message);
}

/// `docs/decisions.md` makes calibre feature-detected. Absent is an answer, and
/// a client that got an error here would show a failure the user is meant to
/// fix — which is exactly what the CLI's `calibre status` refuses to do.
#[tokio::test]
async fn an_absent_calibre_is_a_status_and_never_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp.path().join("no-binaries-here");
    std::fs::create_dir_all(&empty).unwrap();
    let config = EngineConfig {
        db_url: "sqlite::memory:".into(),
        images_dir: tmp.path().join("images"),
        files_dir: tmp.path().join("files"),
        vault_dir: tmp.path().join("vault"),
        log_dir: tmp.path().join("logs"),
        google_api_key: None,
        calibre_bin_dir: Some(empty),
    };
    let api = Api::new(Arc::new(Engine::open(config).await.unwrap()));

    // Not `Err`, whatever this machine happens to have installed.
    match ok(api.dispatch(Request::CalibreStatus).await) {
        Response::CalibreStatus(_) => {}
        other => panic!("{other:?}"),
    }
}

/// The rating rule, across the seam: an unmapped value is a code, never a
/// rounded star. The lookup table exists precisely to refuse that, and a
/// transport is not a reason to start guessing.
#[tokio::test]
async fn an_unmapped_rating_is_a_code_not_a_rounded_star() {
    let (api, _tmp) = api().await;
    let book_id = seed(&api).await;

    // Migration `0007` seeds a 0–5 step-0.5 scale mapped at the whole stars
    // only, so a half is on the scale and off the map — which is the case this
    // is about.
    let review = api.open_review(book_id, None).await.unwrap();
    api.set_rating(review.id, 4.5).await.unwrap();

    let stored = api.review_rating(review.id).await.unwrap().expect("rated");
    assert_eq!(stored.value, 4.5);
    // The scale travels with the value, or the number means nothing.
    assert_eq!(stored.scale.step, 0.5);

    let err = api
        .goodreads_rating(review.id)
        .await
        .expect_err("4.5 has no Goodreads integer");
    assert_eq!(err.code, ErrorCode::UnmappedRating);
}

/// A reflection accretes: the second call finds the first call's note. Worth
/// pinning here because a transport makes "call it twice" the ordinary case
/// rather than the unusual one.
#[tokio::test]
async fn opening_a_reflection_twice_opens_the_same_note() {
    let (api, _tmp) = api().await;
    let book_id = seed(&api).await;

    let first = api.open_reflection(book_id, None).await.unwrap();
    let again = api.open_reflection(book_id, None).await.unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(first.file, again.file);

    // And it is findable from the reading, which is what anchors it.
    let reading = api.active_reading(book_id).await.unwrap().expect("open");
    let found = api
        .note_for_reading(reading.id, NoteKindDto::Reflection)
        .await
        .unwrap()
        .expect("the reflection");
    assert_eq!(found.id, first.id);
    // A review is a different note entirely — never a slice of the private one.
    let review = api.open_review(book_id, None).await.unwrap();
    assert_ne!(review.id, first.id);
}

/// The key is settable and its presence is readable; the key itself is not.
#[tokio::test]
async fn a_key_can_be_set_but_never_read_back() {
    let (api, _tmp) = api().await;
    assert!(!api.has_google_api_key());

    api.set_google_api_key(Some("AIzaSUPERSECRET".into()));
    assert!(api.has_google_api_key());

    // The whole reply, serialized, must not contain it — a `bool` is the only
    // thing this method can answer with.
    let response = ok(api.dispatch(Request::GoogleApiKey).await);
    let json = serde_json::to_string(&response).unwrap();
    assert!(!json.contains("SUPERSECRET"), "{json}");
    assert_eq!(response, Response::Bool(true));

    api.set_google_api_key(None);
    assert!(!api.has_google_api_key());
}

/// The paths a settings screen shows come off the engine's accessors, so they
/// follow `--data-dir` rather than a second copy of the layout.
#[tokio::test]
async fn the_paths_are_the_engines_own() {
    let (api, tmp) = api().await;
    let paths = api.paths();
    assert_eq!(PathBuf::from(&paths.vault_dir), tmp.path().join("vault"));
    assert_eq!(PathBuf::from(&paths.files_dir), tmp.path().join("files"));
    assert_eq!(paths.db_url, "sqlite::memory:");
}

/// The whole vocabulary must survive a round trip through JSON, because that is
/// what the transport does to it. A sample rather than all seventy: what this
/// is really pinning is that the tagging scheme works for unit variants, for
/// nested DTOs and for the optional-field defaults all at once.
#[test]
fn the_vocabulary_survives_json() {
    let calls = vec![
        Request::ApiVersion,
        Request::CandidateMounts,
        Request::GetBook { id: 3 },
        Request::SaveBook {
            book: BookDto {
                title: Some("t".into()),
                ..Default::default()
            },
        },
        Request::CreateNote {
            note: NewNoteDto {
                book_id: Some(1),
                kind: NoteKindDto::Reflection,
                body: "a body\nwith a newline".into(),
                ..Default::default()
            },
        },
        Request::SyncDevice {
            paths: vec!["/Volumes/KOBOeReader/a.sdr".into()],
        },
        Request::MapRating {
            scale_id: 1,
            value: 4.5,
            goodreads: 5,
        },
    ];
    for call in calls {
        let json = serde_json::to_string(&call).unwrap();
        assert_eq!(
            serde_json::from_str::<Request>(&json).unwrap(),
            call,
            "{json}"
        );
    }
}

/// An error crosses the seam as a reply, not as a dropped connection: `call`
/// never fails, whatever the engine did.
#[tokio::test]
async fn a_failing_call_still_produces_a_reply() {
    let (api, _tmp) = api().await;
    let reply = api
        .call(readingbuddy_api::Call {
            id: 99,
            request: Request::GetReading { id: -1 },
        })
        .await;
    assert_eq!(reply.id, 99);
    // A missing reading is `None`, not an error — the failure below is the one
    // that has to survive.
    assert!(matches!(reply.outcome, Outcome::Ok { .. }));

    let reply = api
        .call(readingbuddy_api::Call {
            id: 100,
            request: Request::DeleteBook { id: -1 },
        })
        .await;
    assert_eq!(reply.id, 100);
}
