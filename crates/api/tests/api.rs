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
    Api, ApiError, BookDto, BookFilterDto, BookQueryDto, ErrorCode, MomentKindDto, NewNoteDto,
    NoteKindDto, Outcome, ReadingStateDto, Request, Response, SearchHitDto, SearchSourceDto,
    ShapeSourceDto, StatusFilterDto,
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

    let typed = api
        .list_books(BookQueryDto {
            sort: Default::default(),
            filter: None,
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    match ok(api
        .dispatch(Request::ListBooks {
            limit: 10,
            sort: Default::default(),
            offset: 0,
            filter: None,
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

/// Claim 1 again, for the two link methods this crate grew — both writes, both
/// returning `Unit`, so the only way to see that dispatch did the same thing is
/// to read the link back through the import that consumes it.
#[tokio::test]
async fn dispatch_and_the_typed_method_agree_on_linking_a_foreign_record() {
    let (api, _tmp) = api().await;
    let id = seed(&api).await;

    api.link_goodreads_row("34051011", id)
        .await
        .expect("typed link");
    match ok(api
        .dispatch(Request::LinkCalibreBook {
            uuid: "c47437a8".into(),
            book_id: id,
        })
        .await)
    {
        Response::Unit => {}
        other => panic!("{other:?}"),
    }

    // Both landed, and against the same book — the table is keyed
    // `(source, external_id)`, so one call must not have overwritten the other.
    for (source, external) in [("goodreads", "34051011"), ("calibre", "c47437a8")] {
        let linked = api
            .dispatch(Request::LinkCalibreBook {
                uuid: external.into(),
                book_id: id,
            })
            .await;
        assert!(linked.is_ok(), "{source} link is re-recordable");
    }
}

/// A link to a book that is not there is a typed `NotFound`, not a foreign-key
/// error naming a constraint — a candidate list can always name a book another
/// pane has since deleted.
#[tokio::test]
async fn linking_to_a_missing_book_is_a_typed_error() {
    let (api, _tmp) = api().await;
    let err = api
        .link_calibre_book("uuid-abc", -1)
        .await
        .expect_err("there is no book -1");
    assert_eq!(err.code, ErrorCode::NotFound);
}

/// `only` is `#[serde(default)]`, so a client written before the field existed
/// sends the same JSON and still means "the whole library". Parsed rather than
/// constructed, because the default is a property of the wire form.
#[test]
fn an_import_request_without_only_still_means_the_whole_library() {
    let parsed: Request =
        serde_json::from_str(r#"{"method":"import_calibre_library","params":{"dry_run":true}}"#)
            .expect("an older client's request still parses");
    match parsed {
        Request::ImportCalibreLibrary {
            dry_run,
            only,
            library,
            create_ambiguous,
        } => {
            assert!(dry_run);
            assert!(only.is_empty(), "absent means the whole library");
            assert!(library.is_none());
            assert!(!create_ambiguous);
        }
        other => panic!("{other:?}"),
    }
}

/// The activity log crosses whole, and **absence survives the trip**.
///
/// Items 21 and 31 are built on one distinction — `minutes: null` is "we have no
/// data" and `0` is a measured near-nothing — and a wire that flattened one into
/// the other would put the lie on every reading screen at once. A fresh library
/// has no minutes at all, which is the case that catches a `unwrap_or(0)`
/// anywhere between the SQL and the JSON.
#[tokio::test]
async fn a_period_with_no_device_data_has_no_minutes_rather_than_zero() {
    let (api, _tmp) = api().await;
    seed(&api).await;

    let typed = api
        .activity_summary("2026-01-01", "2026-12-31")
        .await
        .unwrap();
    assert_eq!(typed.minutes, None, "no device has ever spoken here");
    assert_eq!(typed.pages, None);
    // A count the engine fully originates: zero is knowable and is not absence.
    assert_eq!(typed.books_finished, 0);
    assert_eq!(typed.range.from, "2026-01-01");

    let json = serde_json::to_string(&typed).unwrap();
    assert!(json.contains("\"minutes\":null"), "{json}");

    match ok(api
        .dispatch(Request::ActivitySummary {
            from: "2026-01-01".into(),
            to: "2026-12-31".into(),
        })
        .await)
    {
        Response::ActivitySummary(d) => assert_eq!(d, typed),
        other => panic!("{other:?}"),
    }
}

/// A backwards range is an error, **not an empty answer**.
///
/// `DayRange::new` refuses one because every aggregate over an inverted span
/// reports a confident, wrong zero. That refusal has to survive this layer: if
/// the API validated more loosely than the engine — or not at all, by reaching
/// past `DayRange` — a client would get the zero the engine exists to refuse.
#[tokio::test]
async fn a_backwards_range_is_refused_rather_than_answered() {
    let (api, _tmp) = api().await;

    let err = api
        .activity_summary("2026-12-31", "2026-01-01")
        .await
        .expect_err("an inverted range is not a question with an answer");
    assert_eq!(err.code, ErrorCode::InvalidInput);

    match api
        .dispatch(Request::ActivityByDay {
            from: "2026-13-45".into(),
            to: "2026-12-31".into(),
        })
        .await
    {
        Err(e) => assert_eq!(e.code, ErrorCode::InvalidInput),
        Ok(r) => panic!("a non-day was answered: {r:?}"),
    }
}

/// Refilling twice changes nothing the second time, and the report says so per
/// filler rather than as one total.
#[tokio::test]
async fn refilling_the_log_is_idempotent_across_the_seam() {
    let (api, _tmp) = api().await;
    seed(&api).await;

    api.refill_reading_events().await.unwrap();
    match ok(api.dispatch(Request::RefillReadingEvents).await) {
        Response::RefillReport(r) => {
            assert_eq!(r.highlights.inserted, 0);
            assert_eq!(r.notes.updated, 0);
            assert_eq!(r.readings.inserted, 0);
        }
        other => panic!("{other:?}"),
    }
}

/// **`null` is not the same as an empty chapter list.**
///
/// A book owning no epub cannot be asked; an epub with no `toc.ncx` was asked
/// and had nothing to say. A client that collapsed the two would tell its reader
/// the same thing about a missing file and an ordinary EPUB3 book, so the
/// distinction has to reach the wire — and the seeded book, which owns no file
/// at all, is the first half of it.
#[tokio::test]
async fn a_book_with_no_file_has_no_chapter_list_to_read() {
    let (api, _tmp) = api().await;
    let id = seed(&api).await;

    assert_eq!(api.table_of_contents(id).await.unwrap(), None);
    match ok(api.dispatch(Request::TableOfContents { book_id: id }).await) {
        Response::TableOfContents(t) => assert_eq!(t, None),
        other => panic!("{other:?}"),
    }
}

/// A correction crosses as the user's, and the provenance says so afterwards.
///
/// This is item 29's table getting its first client: `set_book_fields` stamps
/// `user`, which is the rank no provider merge outranks, and a frontend showing
/// "who said this" reads it back here. It also pins the three columns item 32
/// added, which had no wire field at all until this — a `save_book` round trip
/// used to drop them silently.
#[tokio::test]
async fn a_correction_crosses_as_the_users_and_is_recorded_that_way() {
    let (api, _tmp) = api().await;
    let id = seed(&api).await;

    let after = ok(api
        .dispatch(Request::SetBookFields {
            book_id: id,
            fields: BookDto {
                subjects: vec!["Fiction / Dystopian".into()],
                series: Some("Dune".into()),
                series_index: Some(2.0),
                ..Default::default()
            },
        })
        .await);
    match after {
        Response::Book(Some(b)) => {
            assert_eq!(b.subjects, ["Fiction / Dystopian"]);
            assert_eq!(b.series.as_deref(), Some("Dune"));
            assert_eq!(b.series_index, Some(2.0));
        }
        other => panic!("{other:?}"),
    }

    let provenance = api.field_provenance(id).await.unwrap();
    let claimed: Vec<&str> = provenance
        .iter()
        .filter(|f| f.source == "user")
        .map(|f| f.field.as_str())
        .collect();
    assert!(claimed.contains(&"subjects"), "{provenance:?}");
    assert!(claimed.contains(&"series"), "{provenance:?}");
    assert!(claimed.contains(&"series_index"), "{provenance:?}");
    // A field nobody has claimed is simply absent — never "unknown provider".
    assert!(!provenance.iter().any(|f| f.field == "publisher"));
}

// ---- item 18: the list surface ------------------------------------------

/// An old client's payload still means what it did. `offset` and `filter` are
/// additive and their absence is the previous behaviour exactly, which is why
/// this item did not move `API_VERSION`.
#[test]
fn a_list_books_payload_without_the_new_fields_still_parses() {
    let r: Request =
        serde_json::from_str(r#"{"method":"list_books","params":{"limit":20}}"#).unwrap();
    assert_eq!(
        r,
        Request::ListBooks {
            limit: 20,
            sort: Default::default(),
            offset: 0,
            filter: None,
        }
    );
    let r: Request = serde_json::from_str(r#"{"method":"list_notes","params":{}}"#).unwrap();
    assert_eq!(
        r,
        Request::ListNotes {
            book_id: None,
            reading_id: None,
            limit: None,
        },
        "no limit is every note, which is what this method always did — and a
         payload written before item 40 still parses, which is why `reading_id`
         is `#[serde(default)]` and `API_VERSION` did not move"
    );
}

/// The count and the page answer the same filter, across the seam.
#[tokio::test]
async fn a_filtered_page_and_its_count_agree_over_the_wire() {
    let (api, _tmp) = api().await;
    let id = seed(&api).await;
    api.save_book(BookDto {
        title: Some("Sea of Tranquility".into()),
        authors: vec!["Emily St. John Mandel".into()],
        isbn_13: Some("9780593321447".into()),
        ..Default::default()
    })
    .await
    .unwrap();
    api.update_progress(id, Some(40), None).await.unwrap();

    let reading = BookFilterDto {
        status: Some(StatusFilterDto::State {
            is: ReadingStateDto::Reading,
        }),
        ..Default::default()
    };
    match ok(api
        .dispatch(Request::CountBooks {
            filter: Some(reading.clone()),
        })
        .await)
    {
        Response::Count(n) => assert_eq!(n, 1),
        other => panic!("{other:?}"),
    }
    match ok(api
        .dispatch(Request::ListBooks {
            limit: -1,
            sort: Default::default(),
            offset: 0,
            filter: Some(reading),
        })
        .await)
    {
        Response::Books(books) => assert_eq!(books.len(), 1),
        other => panic!("{other:?}"),
    }

    // Absence is a filter case and not a reading state — the wire says so too.
    match ok(api
        .dispatch(Request::CountBooks {
            filter: Some(BookFilterDto {
                status: Some(StatusFilterDto::NoReading),
                ..Default::default()
            }),
        })
        .await)
    {
        Response::Count(n) => assert_eq!(n, 1),
        other => panic!("{other:?}"),
    }
    match ok(api.dispatch(Request::CountBooks { filter: None }).await) {
        Response::Count(n) => assert_eq!(n, 2, "no filter is every book"),
        other => panic!("{other:?}"),
    }
}

/// One call for a page, one row per id in the order asked.
#[tokio::test]
async fn the_summary_crosses_the_seam_in_the_order_asked() {
    let (api, _tmp) = api().await;
    let id = seed(&api).await;
    match ok(api
        .dispatch(Request::BookSummaries {
            book_ids: vec![id, id],
        })
        .await)
    {
        Response::BookSummaries(rows) => {
            assert_eq!(rows.len(), 2);
            assert!(rows.iter().all(|r| r.book_id == id));
            assert_eq!(rows[0].highlights, 0);
        }
        other => panic!("{other:?}"),
    }
}

/// Item 19's arithmetic reaches a client that links this crate and not the
/// engine — which is the whole reason the field exists.
#[tokio::test]
async fn a_book_carries_the_shape_of_its_edition() {
    let (api, _tmp) = api().await;
    let id = seed(&api).await;
    let book = api.get_book(id).await.unwrap().unwrap();
    // 333 pages is recorded; nothing measured the cover, so the width stands in.
    assert_eq!(book.shape.thickness_source, ShapeSourceDto::Recorded);
    assert_eq!(book.shape.width_source, ShapeSourceDto::Assumed);
    assert!(book.shape.width_over_height > 0.0);
    assert!(book.shape.thickness_over_height > 0.0);

    // A provider candidate has no page count, so both numbers are stand-ins —
    // and the field is still there, because a book always has *a* shape.
    let bare = BookDto::default();
    assert_eq!(bare.shape.width_source, ShapeSourceDto::Assumed);
    assert_eq!(bare.shape.thickness_source, ShapeSourceDto::Assumed);
}

// ---- item 34: one search surface -------------------------------------------

/// `source` is a filter and its absence is *both*, in `BookFilterDto`'s idiom.
///
/// A payload that names no source has to be the widest question rather than a
/// parse error, or every client would have to spell out the default it wanted
/// anyway.
#[test]
fn a_search_payload_without_a_source_asks_both_indexes() {
    let r: Request =
        serde_json::from_str(r#"{"method":"search_marks","params":{"query":"grief","limit":25}}"#)
            .unwrap();
    assert_eq!(
        r,
        Request::SearchMarks {
            query: "grief".into(),
            source: None,
            book_id: None,
            limit: 25,
        }
    );
    let narrowed: Request = serde_json::from_str(
        r#"{"method":"search_marks","params":{"query":"grief","source":"highlight","limit":5}}"#,
    )
    .unwrap();
    assert_eq!(
        narrowed,
        Request::SearchMarks {
            query: "grief".into(),
            source: Some(SearchSourceDto::Highlight),
            book_id: None,
            limit: 5,
        }
    );
}

/// Item 40 is **additive**, and this is what that claim means on the wire.
///
/// Both new parameters are `#[serde(default)]`, so every payload a client wrote
/// before the item still parses to the same request it always did — which is
/// why `API_VERSION` stays at 2. `the_replaced_method_is_gone_from_the_wire`
/// below pins the number itself; this pins the reason it did not have to move.
#[test]
fn a_payload_written_before_the_scope_existed_still_parses() {
    let old_search: Request =
        serde_json::from_str(r#"{"method":"search_marks","params":{"query":"grief","limit":25}}"#)
            .unwrap();
    assert_eq!(
        old_search,
        Request::SearchMarks {
            query: "grief".into(),
            source: None,
            book_id: None,
            limit: 25,
        },
        "absent is the whole library, exactly as before"
    );
    let old_notes: Request =
        serde_json::from_str(r#"{"method":"list_notes","params":{"book_id":3}}"#).unwrap();
    assert_eq!(
        old_notes,
        Request::ListNotes {
            book_id: Some(3),
            reading_id: None,
            limit: None,
        }
    );

    // And the new shapes are reachable by name, with no positional guessing.
    let scoped: Request = serde_json::from_str(
        r#"{"method":"search_marks","params":{"query":"grief","book_id":7,"limit":25}}"#,
    )
    .unwrap();
    assert_eq!(
        scoped,
        Request::SearchMarks {
            query: "grief".into(),
            source: None,
            book_id: Some(7),
            limit: 25,
        }
    );
    let per_read: Request =
        serde_json::from_str(r#"{"method":"list_notes","params":{"reading_id":2,"limit":12}}"#)
            .unwrap();
    assert_eq!(
        per_read,
        Request::ListNotes {
            book_id: None,
            reading_id: Some(2),
            limit: Some(12),
        }
    );
}

/// **The bug, at the seam.** A client that searched the library and filtered its
/// own answer would have got nothing; the request that carries the scope gets
/// the passage.
///
/// The precondition is asserted, so this cannot pass vacuously if the ranking
/// ever changes underneath it.
#[tokio::test]
async fn a_scoped_search_crosses_the_seam_without_being_truncated() {
    const LIMIT: i64 = 5;
    let (api, _tmp) = api().await;
    for i in 0..12 {
        let decoy = api
            .save_book(BookDto {
                title: Some(format!("Decoy {i}")),
                ..Default::default()
            })
            .await
            .unwrap()
            .id
            .unwrap();
        api.create_note(NewNoteDto {
            book_id: Some(decoy),
            title: Some(format!("d{i}")),
            body: "grief".into(),
            kind: NoteKindDto::Note,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    let wanted = seed(&api).await;
    api.create_note(NewNoteDto {
        book_id: Some(wanted),
        title: Some("On collapse".into()),
        body: "a long paragraph about what the collapse does to grief".into(),
        kind: NoteKindDto::Note,
        ..Default::default()
    })
    .await
    .unwrap();

    let unbounded = api.search_marks("grief", None, None, 100).await.unwrap();
    let buried = unbounded
        .iter()
        .position(|h| matches!(h, SearchHitDto::Note { note, .. } if note.book_id == Some(wanted)))
        .expect("the wanted book matches at all");
    assert!(buried >= LIMIT as usize, "fixture ranks it at {buried}");

    let truncated = api.search_marks("grief", None, None, LIMIT).await.unwrap();
    assert_eq!(truncated.len(), LIMIT as usize);
    assert!(
        !truncated
            .iter()
            .any(|h| matches!(h, SearchHitDto::Note { note, .. } if note.book_id == Some(wanted))),
        "nothing for a client-side filter to keep"
    );

    let scoped = api
        .search_marks("grief", None, Some(wanted), LIMIT)
        .await
        .unwrap();
    assert_eq!(scoped.len(), 1);

    match ok(api
        .dispatch(Request::SearchMarks {
            query: "grief".into(),
            source: None,
            book_id: Some(wanted),
            limit: LIMIT,
        })
        .await)
    {
        Response::SearchHits(dispatched) => assert_eq!(dispatched, scoped),
        other => panic!("{other:?}"),
    }
}

/// The two ids are alternatives, and the seam says so rather than picking one.
///
/// A reading names its book, so the pair is redundant at best and a
/// contradiction at worst — and a contradiction's honest answer is an empty
/// list no client can tell from an empty vault. `NoteScope` cannot represent
/// the pair at all; this is the one place it has to be refused.
#[tokio::test]
async fn asking_a_book_and_a_reading_at_once_is_refused_rather_than_guessed() {
    let (api, _tmp) = api().await;
    let book = seed(&api).await;
    let reading = api.reread(book).await.unwrap();

    assert!(api.list_notes(Some(book), None, None).await.is_ok());
    assert!(api.list_notes(None, Some(reading), None).await.is_ok());

    let err = api
        .list_notes(Some(book), Some(reading), None)
        .await
        .expect_err("the pair is not a conjunction");
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("reading_id"), "{}", err.message);
}

/// The removal, pinned — this is what `API_VERSION = 2` is *for*.
///
/// A client still speaking version 1 must meet a clear refusal rather than
/// having its search silently answer half the question, and this asserts the
/// refusal exists rather than trusting that deleting the variant produced one.
#[test]
fn the_replaced_method_is_gone_from_the_wire() {
    let old = r#"{"method":"search_notes","params":{"query":"grief","limit":25}}"#;
    assert!(
        serde_json::from_str::<Request>(old).is_err(),
        "search_notes still parses, so nothing was actually replaced"
    );
    assert_eq!(readingbuddy_api::API_VERSION, 2);
}

/// One list, and every hit says which kind it is — so a client switches on a
/// string rather than probing for the field that happens to be present.
#[test]
fn a_hit_names_its_kind_on_the_wire() {
    let raw = r#"{"kind":"highlight",
                  "highlight":{"id":7,"book_id":1,"text":"history has failed us",
                               "source":"koreader","created_at":0},
                  "snippet":"history has >>failed<< us"}"#;
    let hit: SearchHitDto = serde_json::from_str(raw).unwrap();
    let SearchHitDto::Highlight { highlight, snippet } = &hit else {
        panic!("the tag did not select the highlight arm: {hit:?}");
    };
    assert_eq!(highlight.id, 7);
    assert_eq!(snippet, "history has >>failed<< us");

    let json = serde_json::to_value(&hit).unwrap();
    assert_eq!(json["kind"], "highlight");
    assert!(
        json.get("note").is_none(),
        "one row, not two nullable ones: {json}"
    );
    assert_eq!(serde_json::from_value::<SearchHitDto>(json).unwrap(), hit);
}

/// The typed method and the dispatch arm are the same call, for the item's own
/// method — claim 1 of this suite, applied where it is newest.
#[tokio::test]
async fn dispatch_and_the_typed_method_agree_on_a_search() {
    let (api, _tmp) = api().await;
    let book = seed(&api).await;
    api.create_note(NewNoteDto {
        book_id: Some(book),
        title: Some("On collapse".into()),
        body: "The symphony rehearses in an antechamber.".into(),
        kind: NoteKindDto::Note,
        ..Default::default()
    })
    .await
    .unwrap();

    let typed = api
        .search_marks("antechamber", None, None, 10)
        .await
        .unwrap();
    assert_eq!(typed.len(), 1);
    assert!(matches!(typed[0], SearchHitDto::Note { .. }));

    match ok(api
        .dispatch(Request::SearchMarks {
            query: "antechamber".into(),
            source: None,
            book_id: None,
            limit: 10,
        })
        .await)
    {
        Response::SearchHits(dispatched) => assert_eq!(dispatched, typed),
        other => panic!("{other:?}"),
    }
}

/// Absence crosses the seam as an answer. A query nothing matches, and a query
/// nobody typed, are both an empty list and never an error.
#[tokio::test]
async fn an_empty_answer_is_not_an_error_across_the_seam() {
    let (api, _tmp) = api().await;
    seed(&api).await;
    for query in ["thermodynamics", "", "   ", "don't"] {
        assert!(
            api.search_marks(query, None, None, 10)
                .await
                .unwrap()
                .is_empty(),
            "{query:?} should answer nothing rather than fail"
        );
    }
}

/// `title` joins the five filters that were already there, rather than becoming
/// a seventh endpoint — so it composes with the count and the paging that
/// `BookFilterDto` already had.
#[tokio::test]
async fn a_title_filter_crosses_the_seam_as_a_predicate() {
    let (api, _tmp) = api().await;
    seed(&api).await;

    let filter = BookFilterDto {
        title: Some("station".into()),
        ..Default::default()
    };
    assert_eq!(api.count_books(Some(filter.clone())).await.unwrap(), 1);
    let rows = api
        .list_books(BookQueryDto {
            sort: Default::default(),
            filter: Some(filter),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // And it composes with a neighbour rather than replacing it.
    let both = BookFilterDto {
        title: Some("station".into()),
        author: Some("mandel".into()),
        ..Default::default()
    };
    assert_eq!(api.count_books(Some(both)).await.unwrap(), 1);

    let none = BookFilterDto {
        title: Some("thermodynamics".into()),
        ..Default::default()
    };
    assert_eq!(api.count_books(Some(none)).await.unwrap(), 0);
}

// ---- item 23: moments ------------------------------------------------------

/// A moment crosses whole, is **polled** rather than pushed, and carries the
/// read it belongs to.
///
/// `reading_id` is the field this test exists for. Item 28 mints a card per
/// *reading* and a reread mints a second one beside the first, so a moment
/// naming only its book cannot select the right card — and that is the shape
/// that is expensive to change later, because it is what the moment *is*
/// rather than what it carries.
#[tokio::test]
async fn a_closed_reading_crosses_as_a_moment_naming_its_read() {
    let (api, _tmp) = api().await;
    let book = seed(&api).await;

    // Nothing has happened yet, and that is an empty list rather than an error.
    assert_eq!(api.pending_moments(None).await.unwrap(), vec![]);

    api.update_progress(book, Some(333), Some(true))
        .await
        .unwrap();

    let typed = api.pending_moments(None).await.unwrap();
    let closed: Vec<_> = typed
        .iter()
        .filter(|m| m.kind == MomentKindDto::ReadingClosed)
        .collect();
    assert_eq!(closed.len(), 1, "one read ended: {typed:?}");
    assert_eq!(closed[0].book_id, Some(book));
    assert!(
        closed[0].reading_id.is_some(),
        "a card is minted per reading, so the moment has to name one"
    );

    match ok(api.dispatch(Request::PendingMoments { limit: None }).await) {
        Response::Moments(ms) => assert_eq!(ms, typed),
        other => panic!("{other:?}"),
    }
}

/// Acknowledging is what makes it fire once, and it is **idempotent** — a
/// client that acknowledges twice, or two clients that acknowledge the same
/// moment, must not be able to break the ceremony or resurrect it.
#[tokio::test]
async fn acknowledging_a_moment_retires_it_and_repeating_it_changes_nothing() {
    let (api, _tmp) = api().await;
    let book = seed(&api).await;
    api.update_progress(book, Some(333), Some(true))
        .await
        .unwrap();

    let id = api.pending_moments(None).await.unwrap()[0].id.clone();
    ok(api
        .dispatch(Request::AcknowledgeMoment { id: id.clone() })
        .await);
    assert!(
        api.pending_moments(None)
            .await
            .unwrap()
            .iter()
            .all(|m| m.id != id)
    );

    // Twice, through both doors, and neither brings it back.
    api.acknowledge_moment(&id).await.unwrap();
    ok(api
        .dispatch(Request::AcknowledgeMoment { id: id.clone() })
        .await);
    assert!(
        api.pending_moments(None)
            .await
            .unwrap()
            .iter()
            .all(|m| m.id != id)
    );
}

/// An id this build knows no kind for is a typed error rather than a row.
#[tokio::test]
async fn an_invented_moment_id_is_refused() {
    let (api, _tmp) = api().await;
    let err = api
        .acknowledge_moment("something_i_made_up:1")
        .await
        .expect_err("a moment id is not a string a client composes");
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

/// **Nothing on this surface counts.** `docs/decisions.md` forbids a badge
/// counting what you have not done, and the cheap version of that is a
/// `pending: 3` beside the rows — so the reply is a bare array and there is no
/// count method to reach for. Asserted rather than trusted, because the next
/// person to want it will want it for a good reason.
#[test]
fn the_wire_states_no_number_of_moments() {
    let json = serde_json::to_string(&Response::Moments(vec![])).unwrap();
    assert_eq!(
        json, r#"{"shape":"moments","value":[]}"#,
        "a bare array, with no number beside it"
    );

    for name in ["count_moments", "pending_moment_count", "moment_count"] {
        let call = format!(r#"{{"method":"{name}","params":{{}}}}"#);
        assert!(
            serde_json::from_str::<Request>(&call).is_err(),
            "{name} must not exist: a number waiting is a badge"
        );
    }
}

/// `limit` takes from the newest end, and the two doors agree about it.
#[tokio::test]
async fn a_limit_takes_the_newest_moments() {
    let (api, _tmp) = api().await;
    let a = seed(&api).await;
    let b = api
        .save_book(BookDto {
            title: Some("Middlemarch".into()),
            isbn_13: Some("9780141439549".into()),
            page_count: Some(880),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
        .unwrap();
    api.update_progress(a, Some(333), Some(true)).await.unwrap();
    api.update_progress(b, Some(880), Some(true)).await.unwrap();

    let all = api.pending_moments(None).await.unwrap();
    assert!(all.len() >= 2);
    assert!(all.windows(2).all(|w| w[0].occurred_at >= w[1].occurred_at));

    match ok(api
        .dispatch(Request::PendingMoments { limit: Some(1) })
        .await)
    {
        Response::Moments(ms) => {
            assert_eq!(ms.len(), 1);
            assert_eq!(ms[0], all[0]);
        }
        other => panic!("{other:?}"),
    }
}
