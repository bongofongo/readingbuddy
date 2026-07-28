//! Scanning a mounted device.
//!
//! Everything here runs against a **copy** of the generated synthetic corpus
//! (`cargo run -p corpus -- gen-synthetic`), because a scan is about files that
//! change: mtimes get touched, bytes get rewritten, sidecars get deleted. The
//! committed fixtures are the shapes; the tempdir is the device.
//!
//! Offline, `sqlite::memory:`, no network. No test here can skip: the fixtures
//! it needs are committed.

use std::path::{Path, PathBuf};

use readingbuddy::device::{self, DeviceBook, DeviceState};
use readingbuddy::{Book, MatchMethod, Storage};

mod common;
use common::{SYNTHETIC, place};

async fn seed(s: &Storage, title: &str) -> i64 {
    s.upsert_book(&Book {
        title: Some(title.into()),
        ..Default::default()
    })
    .await
    .unwrap()
}

fn row<'a>(books: &'a [DeviceBook], path: &Path) -> &'a DeviceBook {
    books
        .iter()
        .find(|b| b.path == path)
        .unwrap_or_else(|| panic!("no scanned row for {}", path.display()))
}

/// The four states `docs/decisions.md` names, each from a fixture that earns
/// it — not from a hand-built struct.
#[tokio::test]
async fn each_of_the_four_states_comes_from_a_fixture_that_deserves_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let pachinko = place(&root, "Pachinko.sdr", "Pachinko.sdr");
    let odyssey = place(&root, "Multi-Chapter.sdr", "Odyssey.sdr");
    let stranger = place(&root, "Unmatched.sdr", "Stranger.sdr");
    let broken = place(&root, "Malformed.sdr", "Broken.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    seed(&s, "Pachinko").await;
    let odyssey_id = seed(&s, "The Odyssey").await;

    // One book has already been brought across; the other has not.
    device::sync_device(&s, std::slice::from_ref(&odyssey))
        .await
        .unwrap();

    let scan = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(scan.books.len(), 4);

    assert_eq!(
        row(&scan.books, &pachinko).state,
        DeviceState::Updated {
            new_highlights: 2,
            refreshed: 0
        }
    );

    let done = row(&scan.books, &odyssey);
    assert_eq!(done.state, DeviceState::Unchanged);
    assert_eq!(done.book_id, Some(odyssey_id));
    assert_eq!(
        done.matched_by,
        Some(MatchMethod::Md5),
        "the sync recorded the link, so the scan must not re-guess it"
    );

    let new = row(&scan.books, &stranger);
    match &new.state {
        DeviceState::New { candidates } => assert!(
            candidates.is_empty(),
            "nothing in the library is close to it: {candidates:?}"
        ),
        other => panic!("expected New, got {other:?}"),
    }
    assert_eq!(new.book_id, None);

    let bad = row(&scan.books, &broken);
    match &bad.state {
        DeviceState::Unreadable(d) => assert!(
            matches!(
                d.kind,
                readingbuddy::DiagnosticKind::SidecarUnparsable { .. }
            ),
            "the existing vocabulary, not a parallel one: {:?}",
            d.kind
        ),
        other => panic!("expected Unreadable, got {other:?}"),
    }
}

/// The pre-filter's entire claim. Asserted as a count, never as a duration.
#[tokio::test]
async fn a_second_scan_of_an_unmodified_tree_parses_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    place(&root, "Pachinko.sdr", "Pachinko.sdr");
    place(&root, "Multi-Chapter.sdr", "Odyssey.sdr");
    place(&root, "Gen-Summary.sdr", "Rated.sdr");
    place(&root, "Unmatched.sdr", "Stranger.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    seed(&s, "Pachinko").await;
    seed(&s, "The Odyssey").await;
    seed(&s, "A Rated Book").await;

    let first = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(first.parsed, 4, "nothing is cached yet");
    assert_eq!(first.cached, 0);

    // Bring the device across, which is the state a device spends its life in.
    let paths: Vec<PathBuf> = first.syncable().map(|b| b.path.clone()).collect();
    assert_eq!(paths.len(), 4);
    device::sync_device(&s, &paths).await.unwrap();

    let second = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(
        second.parsed, 0,
        "no file changed, so no sidecar may be evaluated again"
    );
    assert_eq!(second.cached, 4);
    for b in &second.books {
        assert_eq!(b.state, DeviceState::Unchanged, "{}", b.path.display());
    }

    // And it stays that way — the cache does not decay after one hit.
    let third = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(third.parsed, 0);
    assert_eq!(third.cached, 4);
}

/// The device's own reading state has to survive a cache hit, or the screen
/// would lose the progress bar on every scan that did its job.
#[tokio::test]
async fn a_cache_hit_still_knows_what_the_device_said() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let rated = place(&root, "Gen-Summary.sdr", "Rated.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    seed(&s, "A Rated Book").await;
    device::sync_device(&s, std::slice::from_ref(&rated))
        .await
        .unwrap();

    let first = device::scan_device(&s, &root).await.unwrap();
    let second = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(second.parsed, 0);

    let (a, b) = (&first.books[0], &second.books[0]);
    assert_eq!(a.title.as_deref(), Some("A Rated Book"));
    assert_eq!(a.ko_percent, Some(0.99770326136886));
    assert_eq!(
        a.ko_status.as_ref().map(|s| s.to_string()),
        Some("complete".into())
    );
    assert_eq!(b.title, a.title);
    assert_eq!(b.authors, a.authors);
    assert_eq!(b.ko_percent, a.ko_percent);
    assert_eq!(b.ko_status, a.ko_status);
    assert_eq!(b.partial_md5, a.partial_md5);
}

/// Size and mtime are the freshness key, so moving either one has to cost a
/// parse. This is the half a same-second stamp would miss.
#[tokio::test]
async fn touching_the_mtime_forces_a_re_parse() {
    use std::time::{Duration, SystemTime};

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let odyssey = place(&root, "Multi-Chapter.sdr", "Odyssey.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    seed(&s, "The Odyssey").await;
    device::sync_device(&s, std::slice::from_ref(&odyssey))
        .await
        .unwrap();

    assert_eq!(device::scan_device(&s, &root).await.unwrap().parsed, 1);
    assert_eq!(device::scan_device(&s, &root).await.unwrap().parsed, 0);

    // Same bytes, new mtime — a KOReader flush that changed nothing we keep.
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(&odyssey)
        .unwrap();
    f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000))
        .unwrap();
    drop(f);

    let after = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(after.parsed, 1, "the stat key moved");
    assert_eq!(
        after.books[0].state,
        DeviceState::Unchanged,
        "re-reading it must not invent work"
    );
}

/// A note edited on the device is the case the `refreshed` counter exists for:
/// same identity hash, different payload, and no new highlights at all.
#[tokio::test]
async fn changing_the_bytes_moves_the_state_to_updated() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let pachinko = place(&root, "Pachinko.sdr", "Pachinko.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    seed(&s, "Pachinko").await;
    device::sync_device(&s, std::slice::from_ref(&pachinko))
        .await
        .unwrap();
    assert_eq!(
        device::scan_device(&s, &root).await.unwrap().books[0].state,
        DeviceState::Unchanged
    );

    std::fs::copy(
        Path::new(SYNTHETIC).join("variants/Pachinko-NoteEdited.sdr/metadata.epub.lua"),
        &pachinko,
    )
    .unwrap();

    let scan = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(scan.parsed, 1);
    assert_eq!(
        scan.books[0].state,
        DeviceState::Updated {
            new_highlights: 0,
            refreshed: 1
        }
    );

    // **And it stays Updated.** The scan that noticed the edit also cached what
    // the file now says, so the next scan is a cache hit — and a pre-filter
    // keyed on a row *count* would call this Unchanged, having reported the
    // edit exactly once and then buried it. Measured, not hypothetical: the
    // first cut of this cache did precisely that.
    let again = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(
        again.parsed, 1,
        "the digests disagree, and only the file has the numbers"
    );
    assert_eq!(
        again.books[0].state,
        DeviceState::Updated {
            new_highlights: 0,
            refreshed: 1
        },
        "a device edit must survive being cached"
    );

    // And syncing it settles the state rather than adding a second copy.
    device::sync_device(&s, &[pachinko]).await.unwrap();
    assert_eq!(
        device::scan_device(&s, &root).await.unwrap().books[0].state,
        DeviceState::Unchanged
    );
}

/// KOReader writes a `.lua.old` on every flush and 9 of 10 real `.sdr` dirs
/// have one. It is a *previous* state of the same annotations: scanning it
/// would offer to resurrect highlights the user deleted.
#[tokio::test]
async fn a_lua_old_sibling_is_never_scanned_and_never_cached() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let pachinko = place(&root, "Pachinko.sdr", "Pachinko.sdr");
    let backup = pachinko.with_extension("lua.old");
    std::fs::copy(
        Path::new(SYNTHETIC).join("variants/Pachinko-Superset.sdr/metadata.epub.lua"),
        &backup,
    )
    .unwrap();
    assert!(backup.is_file());

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    seed(&s, "Pachinko").await;

    let scan = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(scan.books.len(), 1, "the backup must not be a book");
    assert_eq!(scan.books[0].path, pachinko);
    assert!(
        s.sidecar_facts(&backup.to_string_lossy())
            .await
            .unwrap()
            .is_none(),
        "and it must not be cached either"
    );
}

/// One bad file must not cost the view of the rest, and both failure modes
/// have to keep their existing diagnostic.
#[tokio::test]
async fn an_unreadable_sidecar_does_not_abort_the_scan() {
    use readingbuddy::DiagnosticKind;

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let broken = place(&root, "Malformed.sdr", "Broken.sdr");
    let binary = place(&root, "Gen-Not-Utf8.sdr", "Binary.sdr");
    let odyssey = place(&root, "Multi-Chapter.sdr", "Odyssey.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    seed(&s, "The Odyssey").await;

    let scan = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(scan.books.len(), 3);

    match &row(&scan.books, &broken).state {
        DeviceState::Unreadable(d) => {
            assert!(matches!(d.kind, DiagnosticKind::SidecarUnparsable { .. }))
        }
        other => panic!("expected Unreadable, got {other:?}"),
    }
    match &row(&scan.books, &binary).state {
        DeviceState::Unreadable(d) => {
            assert!(matches!(d.kind, DiagnosticKind::SidecarUnreadable { .. }))
        }
        other => panic!("expected Unreadable, got {other:?}"),
    }
    assert!(matches!(
        row(&scan.books, &odyssey).state,
        DeviceState::Updated { .. }
    ));

    // Nothing was cached for either failure — every column of `sidecar_seen` is
    // something the file *said*, and these two said nothing.
    for p in [&broken, &binary] {
        assert!(
            s.sidecar_facts(&p.to_string_lossy())
                .await
                .unwrap()
                .is_none(),
            "{} should not be cached",
            p.display()
        );
    }
}

/// A book deleted from the device stops being on the device. The cache row for
/// it goes, and a neighbouring tree's rows do not.
#[tokio::test]
async fn a_sidecar_deleted_from_the_device_is_forgotten() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let other = tmp.path().join("OtherReader");
    let pachinko = place(&root, "Pachinko.sdr", "Pachinko.sdr");
    let odyssey = place(&root, "Multi-Chapter.sdr", "Odyssey.sdr");
    let elsewhere = place(&other, "Gen-Summary.sdr", "Rated.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    device::scan_device(&s, &root).await.unwrap();
    device::scan_device(&s, &other).await.unwrap();
    assert!(
        s.sidecar_facts(&pachinko.to_string_lossy())
            .await
            .unwrap()
            .is_some()
    );

    std::fs::remove_dir_all(pachinko.parent().unwrap()).unwrap();
    let scan = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(scan.books.len(), 1);
    assert_eq!(scan.books[0].path, odyssey);

    assert!(
        s.sidecar_facts(&pachinko.to_string_lossy())
            .await
            .unwrap()
            .is_none(),
        "the deleted sidecar's cache row must go"
    );
    assert!(
        s.sidecar_facts(&elsewhere.to_string_lossy())
            .await
            .unwrap()
            .is_some(),
        "a scan of one mount must not forget another device"
    );
}

/// The state is recomputed from the library on every scan, never remembered.
/// This is what stops the cache lying after an edit that never touched a file.
#[tokio::test]
async fn deleting_the_book_here_turns_an_unchanged_row_back_into_a_new_one() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let odyssey = place(&root, "Multi-Chapter.sdr", "Odyssey.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    let id = seed(&s, "The Odyssey").await;
    device::sync_device(&s, std::slice::from_ref(&odyssey))
        .await
        .unwrap();
    assert_eq!(
        device::scan_device(&s, &root).await.unwrap().books[0].state,
        DeviceState::Unchanged
    );

    s.delete_book(id).await.unwrap();

    let scan = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(scan.parsed, 0, "the file did not change");
    assert!(
        matches!(scan.books[0].state, DeviceState::New { .. }),
        "the verdict is never cached, only the parse: {:?}",
        scan.books[0].state
    );
}

/// A variant title in the ambiguous band is what turns "unmatched" from a dead
/// end into a decision — it has to reach the scan row, not only the import.
#[tokio::test]
async fn a_new_row_carries_the_books_it_might_already_be() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    place(&root, "Pachinko.sdr", "Pachinko.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    // A subtitle the device's copy does not carry: too weak to link silently,
    // too strong to throw away.
    let close = seed(&s, "Pachinko: A Novel of Korea and Japan").await;

    let scan = device::scan_device(&s, &root).await.unwrap();
    match &scan.books[0].state {
        DeviceState::New { candidates } => {
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].book_id, close);
        }
        other => panic!("expected New with a candidate, got {other:?}"),
    }
}

/// Syncing an unmatched row is the decision to create it — and doing it twice
/// must not put a second copy on the shelf.
#[tokio::test]
async fn syncing_a_new_row_creates_the_book_once() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    let stranger = place(&root, "Unmatched.sdr", "Stranger.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    let reports = device::sync_device(&s, std::slice::from_ref(&stranger))
        .await
        .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].stats.matched_by, MatchMethod::New);

    let after = device::scan_device(&s, &root).await.unwrap();
    assert_eq!(after.books[0].state, DeviceState::Unchanged);
    assert_eq!(after.books[0].matched_by, Some(MatchMethod::Md5));

    device::sync_device(&s, &[stranger]).await.unwrap();
    assert_eq!(
        s.list_books(100, readingbuddy::BookSort::LastModified)
            .await
            .unwrap()
            .len(),
        1,
        "a second sync must reuse the recorded link"
    );
}

/// A sync reports per book, whatever shape the caller asked in — that is what
/// lets a frontend show progress instead of one lump at the end.
#[tokio::test]
async fn a_sync_of_a_whole_tree_reports_one_book_at_a_time() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("KOBOeReader");
    place(&root, "Pachinko.sdr", "Pachinko.sdr");
    place(&root, "Multi-Chapter.sdr", "Odyssey.sdr");

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    seed(&s, "Pachinko").await;
    seed(&s, "The Odyssey").await;

    let reports = device::sync_device(&s, std::slice::from_ref(&root))
        .await
        .unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports.iter().map(|r| r.stats.inserted).sum::<usize>(), 6);
}

/// An empty tree is a state, not a failure — and it says so in the existing
/// vocabulary.
#[tokio::test]
async fn a_tree_with_no_sidecars_warns_rather_than_failing() {
    use readingbuddy::DiagnosticKind;

    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("nothing here")).unwrap();

    let s = Storage::connect("sqlite::memory:").await.unwrap();
    let scan = device::scan_device(&s, tmp.path()).await.unwrap();
    assert!(scan.books.is_empty());
    assert_eq!(scan.warnings.len(), 1);
    assert!(matches!(
        scan.warnings[0].kind,
        DiagnosticKind::NoSidecarsFound { .. }
    ));
}
