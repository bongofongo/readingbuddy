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
use serde_json::{json, Value};

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/koreader")
}

fn synthetic_dir() -> PathBuf {
    fixtures_root().join("synthetic")
}

/// The `fixtures` table from `manifest.json`.
fn manifest() -> Value {
    let raw = std::fs::read_to_string(fixtures_root().join("manifest.json"))
        .expect("read manifest.json");
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
    Storage::connect("sqlite::memory:").await.expect("open in-memory db")
}

/// Seed the library with the books a fixture expects (so title-fuzzy matching
/// in `koreader::match_book` succeeds).
async fn seed_books(storage: &Storage, books: &Value) {
    let Some(arr) = books.as_array() else { return };
    for b in arr {
        let authors = b["authors"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        storage
            .upsert_book(&Book {
                title: b["title"].as_str().map(str::to_owned),
                authors,
                ..Default::default()
            })
            .await
            .expect("seed book");
    }
}

/// Snapshot every highlight of a book in stored order: the fields that must
/// stay stable across a re-import.
async fn highlight_rows(storage: &Storage, book_id: i64) -> Value {
    let rows = storage.list_highlights(book_id).await.expect("list highlights");
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
    let report = koreader::import(&storage, &dir, false).await.expect("import");

    let mut imported = Vec::new();
    for s in &report.imported {
        imported.push(json!({
            "book_title": s.book_title,
            "inserted": s.inserted,
            "skipped": s.skipped,
            "flashcards": s.flashcards,
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
    fixtures_root().join("expected").join(format!("{stem}.json"))
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
        eprintln!("UPDATE_GOLDEN: rewrote golden snapshots.");
        return;
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
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

        let first = koreader::import(&storage, &dir, false).await.expect("first import");
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

        let second = koreader::import(&storage, &dir, false).await.expect("second import");
        for s in &second.imported {
            assert_eq!(s.inserted, 0, "{fixture}: re-import inserted rows");
        }
        // Second-run skipped count equals what the first run inserted.
        let first_inserted: usize = first.imported.iter().map(|s| s.inserted).sum();
        let second_skipped: usize = second.imported.iter().map(|s| s.skipped).sum();
        assert_eq!(
            first_inserted, second_skipped,
            "{fixture}: skipped count != original inserted"
        );

        // Every row must be byte-for-byte unchanged: no overwrite, reorder, dup.
        for (book_id, hl, cards) in before {
            assert_eq!(hl, highlight_rows(&storage, book_id).await, "{fixture}: highlights changed");
            assert_eq!(cards, flashcard_words(&storage, book_id).await, "{fixture}: flashcards changed");
        }
    }
}

#[tokio::test]
async fn appends_only_new_on_partial_reimport() {
    // Import Pachinko, then import a superset (Pachinko + one extra highlight).
    // Only the new highlight is inserted; the originals are untouched.
    let storage = mem_storage().await;
    seed_books(&storage, &json!([{ "title": "Pachinko", "authors": ["Min Jin Lee"] }])).await;

    let base = synthetic_dir().join("Pachinko.sdr");
    let first = koreader::import(&storage, &base, false).await.expect("base import");
    let book_id = first.imported[0].book_id;
    let base_inserted = first.imported[0].inserted;
    let before = highlight_rows(&storage, book_id).await;

    // Build a superset sidecar in a temp dir: original annotations + one more.
    let tmp = std::env::temp_dir().join(format!("rb-ko-superset-{}", std::process::id()));
    let sdr = tmp.join("Pachinko.sdr");
    std::fs::create_dir_all(&sdr).unwrap();
    let original = std::fs::read_to_string(base.join("metadata.epub.lua")).unwrap();
    let extra = r#"        [4] = {
            ["chapter"] = "Chapter Three",
            ["datetime"] = "2026-01-07 10:00:00",
            ["drawer"] = "lighten",
            ["pageno"] = 70,
            ["pos0"] = "/body/DocFragment[10]/body/p[1]/text().0",
            ["pos1"] = "/body/DocFragment[10]/body/p[1]/text().12",
            ["text"] = "A brand new highlight added later.",
        },
"#;
    // Insert the extra entry just before the annotations table closes.
    let marker = "    },\n    [\"doc_props\"]";
    let superset = original.replacen(marker, &format!("{extra}    }},\n    [\"doc_props\"]"), 1);
    assert!(superset.contains("A brand new highlight"), "superset injection failed");
    std::fs::write(sdr.join("metadata.epub.lua"), superset).unwrap();

    let second = koreader::import(&storage, &tmp, false).await.expect("superset import");
    std::fs::remove_dir_all(&tmp).ok();

    let s = &second.imported[0];
    assert_eq!(s.inserted, 1, "exactly one new highlight expected");
    assert_eq!(s.skipped, base_inserted, "all originals should be skipped");

    // Originals still present and unchanged; new one appended.
    let after = highlight_rows(&storage, book_id).await;
    let before_arr = before.as_array().unwrap();
    let after_arr = after.as_array().unwrap();
    assert_eq!(after_arr.len(), before_arr.len() + 1);
    for row in before_arr {
        assert!(after_arr.contains(row), "an original highlight went missing");
    }
    assert!(
        after_arr.iter().any(|r| r["text"] == "A brand new highlight added later."),
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
    assert_eq!(report.unmatched[0].title.as_deref(), Some("Nonexistent Tome"));

    // And a whole-directory import over synthetic/ never aborts: it imports the
    // matchable books, reports the rest.
    let storage = mem_storage().await;
    seed_all_synthetic(&storage).await;
    let report = koreader::import(&storage, &synthetic_dir(), false).await.expect("bulk import");
    assert!(!report.imported.is_empty(), "bulk import found no books");
    assert!(!report.warnings.is_empty(), "malformed fixture should still warn in bulk");
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
        eprintln!("real_exports_are_idempotent: no drop-in exports under {} — skipped", real.display());
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
                .upsert_book(&Book { title: Some(title), ..Default::default() })
                .await
                .expect("seed real book");
        }
    }

    let first = koreader::import(&storage, &real, false).await.expect("first real import");
    let mut before = Vec::new();
    for s in &first.imported {
        before.push((s.book_id, highlight_rows(&storage, s.book_id).await, flashcard_words(&storage, s.book_id).await));
    }

    let second = koreader::import(&storage, &real, false).await.expect("second real import");
    for s in &second.imported {
        assert_eq!(s.inserted, 0, "real re-import inserted new rows for {}", s.book_title);
    }
    for (book_id, hl, cards) in before {
        assert_eq!(hl, highlight_rows(&storage, book_id).await, "real highlights changed on re-import");
        assert_eq!(cards, flashcard_words(&storage, book_id).await, "real flashcards changed on re-import");
    }
    eprintln!(
        "real_exports_are_idempotent: verified {} sidecar(s), {} book(s) imported",
        sidecars.len(),
        first.imported.len()
    );
}
