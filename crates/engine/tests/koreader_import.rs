//! Data-driven KOReader import harness.
//!
//! Auto-discovers the fixture corpus under `tests/fixtures/koreader/`, imports
//! each sidecar into a fresh in-memory database, and asserts:
//!   * results match committed golden JSON snapshots (`import_matches_golden`),
//!   * re-importing the same export changes nothing — the idempotency guarantee
//!     (`reimport_is_strictly_idempotent`),
//!   * a superset re-import adds only the genuinely new rows
//!     (`appends_only_new_on_partial_reimport`),
//!   * malformed/unmatched sidecars degrade to warnings, never abort
//!     (`malformed_and_unmatched_are_non_fatal`),
//!   * the user's own drop-in exports under `real/` are idempotent, when present
//!     (`real_exports_are_idempotent`).
//!
//! Regenerate the golden snapshots with `UPDATE_GOLDEN=1` (or `make golden`).

use std::path::{Path, PathBuf};

use readingbuddy::koreader::{self, parse_sidecar};
use readingbuddy::{Book, Storage};
use serde_json::{Value, json};

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/koreader")
}

fn synthetic_dir() -> PathBuf {
    fixtures_root().join("synthetic")
}

/// The `fixtures` table from `manifest.json`.
fn manifest() -> Value {
    let raw =
        std::fs::read_to_string(fixtures_root().join("manifest.json")).expect("read manifest.json");
    let m: Value = serde_json::from_str(&raw).expect("parse manifest.json");
    m["fixtures"].clone()
}

/// Every `*.sdr` directory under `synthetic/`, sorted by name.
fn synthetic_fixtures() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(synthetic_dir())
        .expect("read synthetic dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".sdr"))
        .collect();
    names.sort();
    names
}

async fn mem_storage() -> Storage {
    Storage::connect("sqlite::memory:")
        .await
        .expect("open in-memory db")
}

/// Seed the library with the books a fixture expects (so title-fuzzy matching
/// in `koreader::match_book` succeeds).
async fn seed_books(storage: &Storage, books: &Value) {
    let Some(arr) = books.as_array() else { return };
    for b in arr {
        let authors = b["authors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        storage
            .upsert_book(&Book {
                title: b["title"].as_str().map(str::to_owned),
                authors,
                // Optional, and the reason the schema was extended: without an
                // ISBN on the seeded book, the sibling-epub match path can
                // never fire and stays untested.
                isbn_10: b["isbn_10"].as_str().map(str::to_owned),
                isbn_13: b["isbn_13"].as_str().map(str::to_owned),
                language: b["language"].as_str().map(str::to_owned),
                ..Default::default()
            })
            .await
            .expect("seed book");
    }
}

/// Snapshot every highlight of a book in stored order: the fields that must
/// stay stable across a re-import.
async fn highlight_rows(storage: &Storage, book_id: i64) -> Value {
    let rows = storage
        .list_highlights(book_id)
        .await
        .expect("list highlights");
    Value::Array(
        rows.into_iter()
            .map(|h| {
                json!({
                    "text": h.text,
                    "chapter": h.chapter,
                    "page": h.page,
                    "note": h.note,
                    "ko_datetime": h.ko_datetime,
                })
            })
            .collect(),
    )
}

async fn flashcard_words(storage: &Storage, book_id: i64) -> Vec<String> {
    let mut words: Vec<String> = storage
        .list_flashcards_for_book(book_id)
        .await
        .expect("list flashcards")
        .into_iter()
        .map(|f| f.word)
        .collect();
    words.sort();
    words
}

/// Import one fixture dir into a freshly seeded in-memory DB and reduce the
/// result to the golden JSON shape. Warning text carries absolute paths, so we
/// record only whether warnings fired, never their content.
async fn import_to_golden(fixture: &str, books: &Value) -> (Storage, Value) {
    let storage = mem_storage().await;
    seed_books(&storage, books).await;
    let dir = synthetic_dir().join(fixture);
    let report = koreader::import(&storage, &dir, false)
        .await
        .expect("import");

    let mut imported = Vec::new();
    for s in &report.imported {
        imported.push(json!({
            "book_title": s.book_title,
            "inserted": s.inserted,
            "skipped": s.skipped,
            "flashcards": s.flashcards,
            // Recorded because the two match paths are not interchangeable and
            // the fallback masks the failure of the better one: break the
            // sibling-epub ISBN lookup and fuzzy title matching silently
            // rescues every fixture, leaving every other field in this golden
            // unchanged. This is the only line that guards that branch.
            "matched_by": s.matched_by.to_string(),
            // The device's own reading state. Nothing persists it yet — it
            // lands in `readings` at build item 4 — so this is the only place
            // the `summary`/`percent_finished` parse is asserted end to end.
            // Without these three lines the four device-state fixtures would be
            // green while parsing nothing.
            "percent_finished": s.percent_finished,
            "status": s.status.as_ref().map(|s| s.to_string()),
            "rating": s.rating,
            "highlights": highlight_rows(&storage, s.book_id).await,
            "flashcard_words": flashcard_words(&storage, s.book_id).await,
        }));
    }
    let unmatched: Vec<Value> = report
        .unmatched
        .iter()
        .map(|u| json!({ "title": u.title }))
        .collect();

    let golden = json!({
        "imported": imported,
        "unmatched": unmatched,
        "has_warnings": !report.warnings.is_empty(),
    });
    (storage, golden)
}

fn golden_path(fixture: &str) -> PathBuf {
    // `Pachinko.sdr` -> `expected/Pachinko.json`
    let stem = fixture.strip_suffix(".sdr").unwrap_or(fixture);
    fixtures_root()
        .join("expected")
        .join(format!("{stem}.json"))
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).expect("serialize json")
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn import_matches_golden() {
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let man = manifest();
    let mut failures = Vec::new();
    let mut produced: Vec<PathBuf> = Vec::new();

    for fixture in synthetic_fixtures() {
        let books = man
            .get(&fixture)
            .and_then(|f| f.get("books"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        let (_s, actual) = import_to_golden(&fixture, &books).await;
        let path = golden_path(&fixture);

        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, format!("{}\n", pretty(&actual))).unwrap();
            produced.push(path);
            continue;
        }

        let expected_raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => {
                failures.push(format!(
                    "{fixture}: missing golden {} — run `make golden`",
                    path.display()
                ));
                continue;
            }
        };
        let expected: Value = serde_json::from_str(&expected_raw).expect("parse golden");
        if expected != actual {
            failures.push(format!(
                "{fixture}: golden mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
                pretty(&expected),
                pretty(&actual)
            ));
        }
    }

    if update {
        // Regeneration used to be a pure overwrite that never removed anything,
        // so deleting a fixture left its golden behind to rot — still loaded,
        // still green, guarding nothing.
        let expected_dir = fixtures_root().join("expected");
        let mut stale = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&expected_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("json") && !produced.contains(&p)
                {
                    stale.push(p);
                }
            }
        }
        stale.sort();
        for p in &stale {
            std::fs::remove_file(p).expect("remove stale golden");
            eprintln!("UPDATE_GOLDEN: removed stale golden {}", p.display());
        }
        eprintln!(
            "UPDATE_GOLDEN: rewrote {} golden snapshots.",
            produced.len()
        );
        return;
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

/// The sibling-epub ISBN branch is the one match path that a failure hides:
/// break it and fuzzy title matching silently rescues almost everything, with
/// every other golden field unchanged. `expect.match_via` in the manifest is
/// what makes that failure visible.
#[tokio::test]
async fn fixtures_match_by_the_method_the_manifest_expects() {
    let man = manifest();
    let mut checked = 0;

    for fixture in synthetic_fixtures() {
        let Some(want) = man
            .get(&fixture)
            .and_then(|f| f.get("expect"))
            .and_then(|e| e.get("match_via"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        let books = man
            .get(&fixture)
            .and_then(|f| f.get("books"))
            .cloned()
            .unwrap_or_else(|| json!([]));

        let storage = mem_storage().await;
        seed_books(&storage, &books).await;
        let report = koreader::import(&storage, &synthetic_dir().join(&fixture), false)
            .await
            .expect("import");

        assert_eq!(
            report.imported.len(),
            1,
            "{fixture}: expected exactly one matched book, got {:?}",
            report.imported
        );
        assert_eq!(
            report.imported[0].matched_by.to_string(),
            want,
            "{fixture}: matched by the wrong method"
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "no fixture declares expect.match_via — the ISBN branch is unguarded"
    );
}

/// No fixture can declare `expect.match_via: "md5"`, because on a first import
/// into a fresh library there is no mapping yet — the link is a consequence of
/// matching, not an input to it. So the md5 branch needs its own test: import
/// twice and assert the second pass used the recorded link rather than
/// re-guessing.
///
/// `Gen-Stats` is the synthetic fixture carrying a root `partial_md5_checksum`.
/// If that ever changes this test fails loudly rather than quietly checking
/// nothing, which is the same reason `fixtures_match_by_the_method_the_manifest_expects`
/// refuses to pass at zero.
#[tokio::test]
async fn a_second_import_matches_on_the_link_it_recorded() {
    let fixture = "Gen-Stats.sdr";
    let man = manifest();
    let books = man
        .get(fixture)
        .and_then(|f| f.get("books"))
        .cloned()
        .expect("Gen-Stats must be in the manifest");

    let storage = mem_storage().await;
    seed_books(&storage, &books).await;
    let dir = synthetic_dir().join(fixture);

    let first = koreader::import(&storage, &dir, false)
        .await
        .expect("first");
    assert_eq!(
        first.imported[0].matched_by.to_string(),
        "title",
        "nothing is recorded yet, so the first pass can only guess"
    );

    // Mirrors the fixture's root `partial_md5_checksum`. If `Gen-Stats` loses
    // that key, this fails rather than the test quietly degrading into a second
    // title match that still says "md5" for no reason.
    let linked = storage
        .find_book_by_partial_md5("9f2c4e6a8b0d1357911d3f5b7d9f1a3c")
        .await
        .expect("lookup");
    assert_eq!(
        linked.and_then(|b| b.id),
        Some(first.imported[0].book_id),
        "the import must record the link it just made"
    );

    let second = koreader::import(&storage, &dir, false)
        .await
        .expect("second");
    assert_eq!(second.imported[0].matched_by.to_string(), "md5");
    assert_eq!(second.imported[0].inserted, 0);
}

#[tokio::test]
async fn reimport_is_strictly_idempotent() {
    let man = manifest();
    for fixture in synthetic_fixtures() {
        let books = man
            .get(&fixture)
            .and_then(|f| f.get("books"))
            .cloned()
            .unwrap_or_else(|| json!([]));

        let storage = mem_storage().await;
        seed_books(&storage, &books).await;
        let dir = synthetic_dir().join(&fixture);

        let first = koreader::import(&storage, &dir, false)
            .await
            .expect("first import");
        // Nothing produced (malformed / unmatched) has no rows to guard.
        if first.imported.iter().all(|s| s.inserted == 0) {
            continue;
        }

        // Capture rows for every imported book before re-importing.
        let mut before = Vec::new();
        for s in &first.imported {
            before.push((
                s.book_id,
                highlight_rows(&storage, s.book_id).await,
                flashcard_words(&storage, s.book_id).await,
            ));
        }

        let second = koreader::import(&storage, &dir, false)
            .await
            .expect("second import");
        for s in &second.imported {
            assert_eq!(s.inserted, 0, "{fixture}: re-import inserted rows");
        }
        // On a re-import every entry the sidecar contains must be skipped —
        // which is `inserted + skipped` from the first run, not `inserted`
        // alone. A sidecar can carry duplicates of its *own* entries (KOReader
        // emits them after a sync conflict), so the first run already skips
        // some; assuming otherwise made this assertion accidentally specific to
        // duplicate-free fixtures.
        let first_seen: usize = first.imported.iter().map(|s| s.inserted + s.skipped).sum();
        let second_skipped: usize = second.imported.iter().map(|s| s.skipped).sum();
        assert_eq!(
            first_seen, second_skipped,
            "{fixture}: re-import did not skip every entry it saw"
        );

        // Every row must be byte-for-byte unchanged: no overwrite, reorder, dup.
        for (book_id, hl, cards) in before {
            assert_eq!(
                hl,
                highlight_rows(&storage, book_id).await,
                "{fixture}: highlights changed"
            );
            assert_eq!(
                cards,
                flashcard_words(&storage, book_id).await,
                "{fixture}: flashcards changed"
            );
        }
    }
}

#[tokio::test]
async fn appends_only_new_on_partial_reimport() {
    // Import Pachinko, then import a superset (Pachinko + one extra highlight).
    // Only the new highlight is inserted; the originals are untouched.
    let storage = mem_storage().await;
    seed_books(
        &storage,
        &json!([{ "title": "Pachinko", "authors": ["Min Jin Lee"] }]),
    )
    .await;

    let base = synthetic_dir().join("Pachinko.sdr");
    let first = koreader::import(&storage, &base, false)
        .await
        .expect("base import");
    let book_id = first.imported[0].book_id;
    let base_inserted = first.imported[0].inserted;
    let before = highlight_rows(&storage, book_id).await;

    // The superset is a committed fixture, not a string splice. This used to
    // do `original.replacen("    },\n    [\"doc_props\"]", ..)` against
    // Pachinko's literal indentation, so reformatting that file silently broke
    // this test. It lives under `variants/` because fixture discovery is a
    // NON-recursive read_dir — invisible to the golden loop, so it needs no
    // golden of its own.
    let superset_dir = synthetic_dir().join("variants/Pachinko-Superset.sdr");
    let second = koreader::import(&storage, &superset_dir, false)
        .await
        .expect("superset import");

    let s = &second.imported[0];
    assert_eq!(s.inserted, 1, "exactly one new highlight expected");
    assert_eq!(s.skipped, base_inserted, "all originals should be skipped");

    // Originals still present and unchanged; new one appended.
    let after = highlight_rows(&storage, book_id).await;
    let before_arr = before.as_array().unwrap();
    let after_arr = after.as_array().unwrap();
    assert_eq!(after_arr.len(), before_arr.len() + 1);
    for row in before_arr {
        assert!(
            after_arr.contains(row),
            "an original highlight went missing"
        );
    }
    assert!(
        after_arr
            .iter()
            .any(|r| r["text"] == "A brand new highlight added later."),
        "new highlight not stored"
    );
}

#[tokio::test]
async fn malformed_and_unmatched_are_non_fatal() {
    // Malformed: bad Lua -> warning, no imported/unmatched.
    let storage = mem_storage().await;
    let report = koreader::import(&storage, &synthetic_dir().join("Malformed.sdr"), false)
        .await
        .expect("import must not error on bad lua");
    assert!(report.imported.is_empty());
    assert!(report.unmatched.is_empty());
    assert!(!report.warnings.is_empty(), "malformed sidecar should warn");

    // Unmatched: valid highlights, no matching book -> reported, not fatal.
    let storage = mem_storage().await;
    let report = koreader::import(&storage, &synthetic_dir().join("Unmatched.sdr"), false)
        .await
        .expect("import");
    assert!(report.imported.is_empty());
    assert_eq!(report.unmatched.len(), 1);
    assert_eq!(
        report.unmatched[0].title.as_deref(),
        Some("Nonexistent Tome")
    );

    // And a whole-directory import over synthetic/ never aborts: it imports the
    // matchable books, reports the rest.
    let storage = mem_storage().await;
    seed_all_synthetic(&storage).await;
    let report = koreader::import(&storage, &synthetic_dir(), false)
        .await
        .expect("bulk import");
    assert!(!report.imported.is_empty(), "bulk import found no books");
    assert!(
        !report.warnings.is_empty(),
        "malformed fixture should still warn in bulk"
    );
}

/// Seed every book named in the manifest — used for whole-directory imports.
async fn seed_all_synthetic(storage: &Storage) {
    let man = manifest();
    if let Some(obj) = man.as_object() {
        for f in obj.values() {
            if let Some(books) = f.get("books") {
                seed_books(storage, books).await;
            }
        }
    }
}

#[tokio::test]
async fn real_exports_are_idempotent() {
    let real = fixtures_root().join("real");
    let sidecars = koreader::find_sidecars(&real).expect("scan real dir");
    if sidecars.is_empty() {
        if std::env::var("READINGBUDDY_REQUIRE_FIXTURES").is_ok() {
            panic!(
                "REQUIRE_FIXTURES set but no drop-in exports under {}",
                real.display()
            );
        }
        eprintln!(
            "SKIPPED real_exports_are_idempotent: no drop-in exports under {}",
            real.display()
        );
        return;
    }

    let storage = mem_storage().await;
    // Seed a book per sidecar title so fuzzy matching succeeds.
    for path in &sidecars {
        if let Ok(src) = std::fs::read_to_string(path)
            && let Ok(sc) = parse_sidecar(&src)
            && let Some(title) = sc.title
        {
            storage
                .upsert_book(&Book {
                    title: Some(title),
                    ..Default::default()
                })
                .await
                .expect("seed real book");
        }
    }

    let first = koreader::import(&storage, &real, false)
        .await
        .expect("first real import");
    let mut before = Vec::new();
    for s in &first.imported {
        before.push((
            s.book_id,
            highlight_rows(&storage, s.book_id).await,
            flashcard_words(&storage, s.book_id).await,
        ));
    }

    let second = koreader::import(&storage, &real, false)
        .await
        .expect("second real import");
    for s in &second.imported {
        assert_eq!(
            s.inserted, 0,
            "real re-import inserted new rows for {}",
            s.book_title
        );
    }
    for (book_id, hl, cards) in before {
        assert_eq!(
            hl,
            highlight_rows(&storage, book_id).await,
            "real highlights changed on re-import"
        );
        assert_eq!(
            cards,
            flashcard_words(&storage, book_id).await,
            "real flashcards changed on re-import"
        );
    }
    eprintln!(
        "real_exports_are_idempotent: verified {} sidecar(s), {} book(s) imported",
        sidecars.len(),
        first.imported.len()
    );
}

/// A real library is a directory tree, not a flat list of `.sdr` dirs. Nothing
/// covered the recursive walk, the depth cap, or the rule that a non-sidecar
/// file inside a `.sdr` is ignored.
#[tokio::test]
async fn a_nested_library_tree_is_walked_to_its_leaves() {
    let tmp = tempfile::tempdir().unwrap();
    let deep = tmp
        .path()
        .join("Fiction/Translated/Japanese/Modern/The Trial.sdr");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(
        deep.join("metadata.epub.lua"),
        std::fs::read(
            synthetic_dir()
                .join("The-Trial.sdr")
                .join("metadata.epub.lua"),
        )
        .unwrap(),
    )
    .unwrap();
    // Files that are not sidecars must be ignored, even inside a `.sdr`.
    std::fs::write(deep.join("cover.jpg"), b"not a sidecar").unwrap();
    std::fs::write(deep.join("notes.txt"), b"also not a sidecar").unwrap();

    let found = koreader::find_sidecars(tmp.path()).expect("walk");
    assert_eq!(found.len(), 1, "expected one sidecar, got {found:?}");

    let storage = mem_storage().await;
    seed_books(
        &storage,
        &json!([{ "title": "The Trial", "authors": ["Franz Kafka"] }]),
    )
    .await;
    let report = koreader::import(&storage, tmp.path(), false)
        .await
        .expect("import from a nested tree");
    assert_eq!(report.imported.len(), 1);
    assert!(report.imported[0].inserted > 0);
}

/// Not a CI case — it exists so the cost of a genuinely large export is known
/// rather than guessed. `cargo test -p readingbuddy -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "scale check; run explicitly"]
async fn a_very_large_export_imports_in_reasonable_time() {
    const N: usize = 5_000;

    let mut lua = String::from("return {\n    [\"annotations\"] = {\n");
    for i in 1..=N {
        lua.push_str(&format!(
            "        [{i}] = {{ [\"text\"] = \"highlight number {i}\", \
             [\"pos0\"] = \"/body/p[{i}]/text().0\", [\"pageno\"] = {i}, \
             [\"datetime\"] = \"2026-01-01 00:00:00\" }},\n"
        ));
    }
    lua.push_str("    },\n    [\"doc_props\"] = { [\"title\"] = \"Enormous Book\" },\n}\n");

    let tmp = tempfile::tempdir().unwrap();
    let sdr = tmp.path().join("Enormous Book.sdr");
    std::fs::create_dir_all(&sdr).unwrap();
    std::fs::write(sdr.join("metadata.epub.lua"), &lua).unwrap();

    let storage = mem_storage().await;
    seed_books(
        &storage,
        &json!([{ "title": "Enormous Book", "authors": [] }]),
    )
    .await;

    let started = std::time::Instant::now();
    let report = koreader::import(&storage, tmp.path(), false)
        .await
        .expect("large import");
    let elapsed = started.elapsed();

    assert_eq!(report.imported[0].inserted, N);
    eprintln!(
        "imported {N} highlights in {elapsed:?} ({:.0}/s)",
        N as f64 / elapsed.as_secs_f64()
    );
}
