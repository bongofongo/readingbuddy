//! Tier-2 corpus: sidecars derived from real Project Gutenberg epubs.
//!
//! A separate file from `koreader_import.rs` on purpose. That harness is tuned
//! to a flat, committed, auto-discovered fixture set with goldens; this corpus
//! is optional, absent by default, differently laid out, and wants different
//! assertions (scale, no-loss, real prose). Grafting it on would give all seven
//! of those tests a skip branch and stop `make test-import` meaning "the fast
//! committed harness".
//!
//! Everything here **skips loudly** when the corpus is absent, and
//! `READINGBUDDY_REQUIRE_FIXTURES=1` turns every such skip into a failure — so
//! a broken fetch cannot masquerade as a green build.
//!
//!   scripts/fetch-corpus.sh && cargo run -p corpus -- gen-corpus --seed 42
//!   (or just `make corpus`)

use std::path::{Path, PathBuf};

use readingbuddy::koreader::{self, parse_sidecar};
use readingbuddy::{Book, Storage};

mod common;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/generated")
}

/// The generator emits one subtree per sidecar layout. Each is a plausible
/// device library on its own; together they are not (see
/// `the_whole_corpus_imports_idempotently`).
const LAYOUTS: [&str; 2] = ["modern", "legacy"];

/// `None` means "not generated"; every test then skips.
fn sidecars() -> Option<Vec<PathBuf>> {
    let dir = corpus_dir();
    if !dir.is_dir() {
        return None;
    }
    let found = koreader::find_sidecars(&dir).ok()?;
    (!found.is_empty()).then_some(found)
}

/// A skip that cannot hide. Returns true if the caller should bail out.
fn skipped(reason: &str) -> bool {
    common::skipped(reason, "run `make corpus`")
}

macro_rules! corpus_or_skip {
    () => {
        match sidecars() {
            Some(s) => s,
            None => {
                skipped("the tier-2 corpus has not been generated");
                return;
            }
        }
    };
}

/// The lock records what the generator produced. Comparing against it separates
/// "not generated yet" from "generated, then hand-edited" — the second is a
/// silent corruption of the corpus that would otherwise never be noticed.
#[test]
fn generated_sidecars_match_the_lock_file() {
    let _ = corpus_or_skip!();
    let lock_path = corpus_dir().join("corpus.lock.json");
    let Ok(raw) = std::fs::read_to_string(&lock_path) else {
        skipped("corpus.lock.json is missing");
        return;
    };
    let lock: serde_json::Value = serde_json::from_str(&raw).expect("parse corpus.lock.json");

    let mut checked = 0;
    for (rel, want) in lock["files"].as_object().expect("files table") {
        if !rel.ends_with(".lua") {
            continue;
        }
        let path = corpus_dir().join(rel);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        let got = {
            use sha2::{Digest, Sha256};
            Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(
            got,
            want.as_str().unwrap(),
            "{rel} does not match the lock — regenerate rather than hand-editing"
        );
        checked += 1;
    }
    assert!(checked > 0, "the lock recorded no sidecars");
}

/// Real prose is where a parser meets what it was never tested against:
/// nineteenth-century punctuation, em-dashes, quotation marks, Cyrillic, and
/// space-less Japanese and Chinese.
#[test]
fn every_generated_sidecar_parses() {
    let found = corpus_or_skip!();
    let mut total_highlights = 0;

    for path in &found {
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let sc = parse_sidecar(&src)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", path.display()));

        assert!(
            sc.title.is_some(),
            "{}: doc_props.title went missing",
            path.display()
        );
        for h in &sc.highlights {
            assert!(
                !h.text.is_empty(),
                "{}: empty highlight text",
                path.display()
            );
            assert!(
                h.pos0.is_some(),
                "{}: highlight without pos0",
                path.display()
            );
        }
        total_highlights += sc.highlights.len();
    }

    eprintln!(
        "corpus: {} sidecars, {total_highlights} highlights",
        found.len()
    );
    assert!(total_highlights > 0, "the corpus produced no highlights");
}

/// The idempotency guarantee, over a whole layout tree at once rather than one
/// fixture at a time — which also exercises importing a many-book library in a
/// single call.
///
/// **One layout per pass, because a device carries one.** The two trees hold
/// the same annotations of the same books in two encodings with different
/// payloads (page `3i` against `7i`, notes only in the legacy one). Imported
/// together they match the same library book and collide on `identity_hash`, so
/// every pass refreshes every row toward whichever sidecar it read last and
/// nothing is ever `skipped`. That is import behaving correctly on input no
/// device can produce; the corpus is laid out so it cannot arise.
#[tokio::test]
async fn the_whole_corpus_imports_idempotently() {
    let _ = corpus_or_skip!();

    for layout in LAYOUTS {
        let dir = corpus_dir().join(layout);
        // Loud, not `continue`: a corpus generated before the layout split has
        // no subtrees at all, and silently covering nothing is exactly the
        // failure mode the skip rule exists to prevent.
        let found = koreader::find_sidecars(&dir).unwrap_or_default();
        if found.is_empty() {
            skipped(&format!(
                "corpus/generated/{layout} holds no sidecars (regenerate: `make corpus`)"
            ));
            return;
        }

        let storage = Storage::connect("sqlite::memory:").await.expect("db");
        // Seed one book per sidecar so every one of them matches.
        for path in &found {
            let src = std::fs::read_to_string(path).unwrap();
            let Ok(sc) = parse_sidecar(&src) else {
                continue;
            };
            let Some(title) = sc.title.clone() else {
                continue;
            };
            storage
                .upsert_book(&Book {
                    title: Some(title),
                    authors: sc.authors.clone().into_iter().collect(),
                    ..Default::default()
                })
                .await
                .expect("seed");
        }

        let first = koreader::import(&storage, &dir, false)
            .await
            .expect("first corpus import");
        assert!(
            !first.imported.is_empty(),
            "{layout}: nothing matched; warnings: {:?}",
            first.warnings
        );

        let inserted: usize = first.imported.iter().map(|s| s.inserted).sum();
        let seen: usize = first.imported.iter().map(|s| s.inserted + s.skipped).sum();
        assert!(inserted > 0, "{layout}: corpus import inserted nothing");

        // Snapshot every book's rows before re-importing.
        let mut before = Vec::new();
        for s in &first.imported {
            before.push((s.book_id, storage.list_highlights(s.book_id).await.unwrap()));
        }

        let second = koreader::import(&storage, &dir, false)
            .await
            .expect("second corpus import");
        for s in &second.imported {
            assert_eq!(
                s.inserted, 0,
                "{layout}: re-import inserted rows for {}",
                s.book_title
            );
            // The counter item 2 added, asserted here at scale: identical bytes
            // must not look like a device edit. Without this the sum below can
            // be satisfied by rows drifting into `updated` instead.
            assert_eq!(
                s.updated, 0,
                "{layout}: re-import refreshed device fields for {} — the sidecar did not change",
                s.book_title
            );
        }
        let second_skipped: usize = second.imported.iter().map(|s| s.skipped).sum();
        assert_eq!(
            seen, second_skipped,
            "{layout}: re-import did not skip every entry it saw"
        );

        for (book_id, rows) in before {
            let now = storage.list_highlights(book_id).await.unwrap();
            assert_eq!(
                rows.len(),
                now.len(),
                "{layout}: row count changed for book {book_id}"
            );
            for (a, b) in rows.iter().zip(now.iter()) {
                assert_eq!(a.text, b.text, "{layout}: highlight text mutated");
                assert_eq!(a.page, b.page, "{layout}: page mutated");
                assert_eq!(a.ko_datetime, b.ko_datetime, "{layout}: datetime mutated");
            }
        }

        eprintln!(
            "corpus/{layout}: {inserted} highlights across {} books",
            first.imported.len()
        );
    }
}

/// Both layouts are generated for every book, so the corpus is also a
/// differential test: the same source text, expressed two ways, must yield the
/// same highlight *text*. They live in sibling trees (see the idempotency test
/// for why), so a pair is the same `<slug>.sdr` under each.
#[test]
fn the_modern_and_legacy_encodings_agree_on_the_text() {
    let _ = corpus_or_skip!();
    let mut compared = 0;

    let Ok(modern_side) = koreader::find_sidecars(&corpus_dir().join("modern")) else {
        skipped("the modern corpus tree is missing");
        return;
    };

    for modern in &modern_side {
        let dir = modern.parent().unwrap();
        let name = dir.file_name().unwrap().to_str().unwrap();
        let legacy = corpus_dir().join("legacy").join(name).join(
            modern
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("metadata.epub.lua"),
        );
        if !legacy.is_file() {
            continue;
        }

        let a = parse_sidecar(&std::fs::read_to_string(modern).unwrap()).unwrap();
        let b = parse_sidecar(&std::fs::read_to_string(&legacy).unwrap()).unwrap();

        let mut ta: Vec<&str> = a.highlights.iter().map(|h| h.text.as_str()).collect();
        let mut tb: Vec<&str> = b.highlights.iter().map(|h| h.text.as_str()).collect();
        ta.sort_unstable();
        tb.sort_unstable();
        assert_eq!(
            ta, tb,
            "{name}: the two layouts disagree about the highlight text"
        );
        assert_eq!(a.title, b.title, "{name}: titles disagree");
        compared += 1;
    }

    assert!(compared > 0, "no modern/legacy pairs found");
    eprintln!("corpus: compared {compared} modern/legacy pairs");
}
