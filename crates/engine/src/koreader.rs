use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mlua::{Lua, LuaOptions, StdLib, Table, Value};
use strsim::jaro_winkler;

use crate::book::Book;
use crate::diagnostic::Diagnostic;
use crate::error::{EngineError, Result};
use crate::flashcards::single_word;
use crate::search::normalize;
use crate::storage::{NewHighlight, Storage};

/// Instructions a sidecar chunk may execute before it is killed.
///
/// A genuine sidecar is a table literal, so its cost is roughly proportional to
/// its entries: the 5000-highlight scale fixture is on the order of 10^5
/// instructions. 5M is ~50x headroom over the largest export anyone plausibly
/// has, while still tripping quickly.
///
/// It was 50M first, which is wrong for a reason worth recording: the budget
/// has to bite quickly under *instrumentation*, not just in a release build.
/// Under ASAN + sanitizer coverage the fuzzer ran for minutes on a single
/// `while true do end` input instead of the milliseconds a normal build takes —
/// so the ceiling that looked harmless made the fuzz target useless.
const LUA_INSTRUCTION_BUDGET: u32 = 5_000_000;

/// How deep a library tree may nest before the walk gives up. Guards against a
/// symlink cycle, which is otherwise an unbounded recursion.
const MAX_LIBRARY_DEPTH: usize = 32;

/// Parsed KOReader `.sdr` sidecar (`metadata.epub.lua` etc.).
#[derive(Debug, Default)]
pub struct KoSidecar {
    pub title: Option<String>,
    pub authors: Option<String>,
    pub language: Option<String>,
    pub partial_md5: Option<String>,
    pub highlights: Vec<NewHighlight>,
}

/// Evaluate a sidecar chunk (`return { ... }`) in a sandboxed Lua VM (no
/// stdlib loaded) and walk the returned table. Handles both the modern
/// (2024+) flat `annotations` array and the legacy `highlight`+`bookmarks`
/// page-keyed layout.
pub fn parse_sidecar(src: &str) -> Result<KoSidecar> {
    let lua = Lua::new_with(StdLib::NONE, LuaOptions::default())
        .map_err(|e| EngineError::Sidecar(format!("lua init: {e}")))?;

    // `StdLib::NONE` takes away the standard library but NOT the ability to
    // loop: `return (function() while true do end end)()` is a valid chunk that
    // never returns, and a sidecar is a file we did not write. Without a
    // budget, pointing an import at one such file hangs the whole run forever.
    //
    // A real sidecar is a table literal — it executes in thousands of
    // instructions, not millions — so this ceiling is far above any legitimate
    // input while still bounding a hostile one.
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(LUA_INSTRUCTION_BUDGET),
        |_lua, _debug| {
            Err(mlua::Error::RuntimeError(
                "sidecar exceeded its instruction budget (runaway loop?)".to_string(),
            ))
        },
    );

    let value: Value = lua
        .load(src)
        .eval()
        .map_err(|e| EngineError::Sidecar(format!("lua eval: {e}")))?;
    let Value::Table(root) = value else {
        return Err(EngineError::Sidecar(
            "sidecar did not return a table".into(),
        ));
    };

    let mut sidecar = KoSidecar {
        partial_md5: get_str(&root, "partial_md5_checksum"),
        ..Default::default()
    };
    if let Some(props) = get_table(&root, "doc_props") {
        sidecar.title = get_str(&props, "title");
        sidecar.authors = get_str(&props, "authors");
        sidecar.language = get_str(&props, "language");
    }

    if let Some(annotations) = get_table(&root, "annotations") {
        sidecar.highlights = parse_annotations(&annotations)?;
    } else if let Some(highlight) = get_table(&root, "highlight") {
        let notes_by_datetime = get_table(&root, "bookmarks")
            .map(|b| bookmark_notes(&b))
            .unwrap_or_default();
        sidecar.highlights = parse_legacy(&highlight, &notes_by_datetime)?;
    }
    Ok(sidecar)
}

fn get_str(t: &Table, key: &str) -> Option<String> {
    t.get::<Option<String>>(key)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

fn get_int(t: &Table, key: &str) -> Option<i64> {
    t.get::<Option<i64>>(key).ok().flatten()
}

fn get_table(t: &Table, key: &str) -> Option<Table> {
    t.get::<Option<Table>>(key).ok().flatten()
}

fn entry_to_highlight(item: &Table, page: Option<i64>) -> Option<NewHighlight> {
    let text = get_str(item, "text")?;
    // Modern `annotations` mixes highlights and plain bookmarks; a real
    // highlight always carries a pos0 xpointer.
    let pos0 = get_str(item, "pos0")?;
    Some(NewHighlight {
        text,
        chapter: get_str(item, "chapter"),
        page: get_int(item, "pageno").or(get_int(item, "page")).or(page),
        pos0: Some(pos0),
        pos1: get_str(item, "pos1"),
        ko_datetime: get_str(item, "datetime"),
        color: get_str(item, "color").or_else(|| get_str(item, "drawer")),
        note: get_str(item, "note"),
        source: "koreader".to_string(),
    })
}

fn parse_annotations(annotations: &Table) -> Result<Vec<NewHighlight>> {
    // `sequence_values` stops dead at the first missing index, so a table like
    // `{[1]=.., [3]=..}` would silently yield one highlight and drop the rest.
    // KOReader produces exactly that shape after a sync conflict, and silent
    // data loss on someone's reading notes is the worst outcome here. Iterate
    // the pairs and sort, the same way `parse_legacy` already does.
    let mut indexed: Vec<(i64, NewHighlight)> = Vec::new();
    for pair in annotations.pairs::<i64, Table>() {
        let (idx, item) =
            pair.map_err(|e| EngineError::Sidecar(format!("annotation entry: {e}")))?;
        if let Some(h) = entry_to_highlight(&item, None) {
            indexed.push((idx, h));
        }
    }
    // Lua map iteration order is arbitrary; the index is the only ordering the
    // file actually carries.
    indexed.sort_by_key(|(idx, _)| *idx);
    Ok(indexed.into_iter().map(|(_, h)| h).collect())
}

/// Legacy layout: `highlight[pageno][idx] = { datetime, text, pos0, ... }`.
/// User notes live separately in `bookmarks`, joined here by datetime.
fn parse_legacy(
    highlight: &Table,
    notes_by_datetime: &HashMap<String, String>,
) -> Result<Vec<NewHighlight>> {
    let mut out = Vec::new();
    for pair in highlight.pairs::<i64, Table>() {
        let (page, items) =
            pair.map_err(|e| EngineError::Sidecar(format!("highlight page: {e}")))?;
        for item in items.pairs::<i64, Table>() {
            let (_, item) =
                item.map_err(|e| EngineError::Sidecar(format!("highlight item: {e}")))?;
            if let Some(mut h) = entry_to_highlight(&item, Some(page)) {
                if h.note.is_none()
                    && let Some(dt) = &h.ko_datetime
                {
                    h.note = notes_by_datetime.get(dt).cloned();
                }
                out.push(h);
            }
        }
    }
    // Page-keyed map iteration order is arbitrary; make output deterministic.
    out.sort_by_key(|h| (h.page, h.ko_datetime.clone()));
    Ok(out)
}

/// Legacy bookmarks: `text` holds the user's note, `notes` the highlighted
/// passage. Only keep entries with a real user note.
fn bookmark_notes(bookmarks: &Table) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in bookmarks.pairs::<i64, Table>() {
        let Ok((_, item)) = pair else { continue };
        let (Some(dt), Some(text)) = (get_str(&item, "datetime"), get_str(&item, "text")) else {
            continue;
        };
        let highlighted = get_str(&item, "notes");
        if Some(&text) != highlighted.as_ref() {
            map.insert(dt, text);
        }
    }
    map
}

// ---- import ----------------------------------------------------------------

#[derive(Debug, Default)]
pub struct ImportReport {
    pub imported: Vec<BookImportStats>,
    pub unmatched: Vec<UnmatchedSidecar>,
    pub warnings: Vec<Diagnostic>,
}

/// How a sidecar found its book.
///
/// Recorded because the two paths are not equally good and the fallback hides
/// the failure of the better one: if the sibling-epub ISBN lookup breaks, fuzzy
/// title matching quietly rescues almost every fixture and every golden stays
/// green. Without this field that branch is effectively untested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMethod {
    /// A sibling `.epub` next to the `.sdr` dir carried an ISBN we know.
    Isbn,
    /// Fuzzy jaro-winkler match on the normalized `doc_props.title`.
    Title,
}

impl std::fmt::Display for MatchMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchMethod::Isbn => write!(f, "isbn"),
            MatchMethod::Title => write!(f, "title"),
        }
    }
}

#[derive(Debug)]
pub struct BookImportStats {
    pub book_id: i64,
    pub book_title: String,
    pub inserted: usize,
    pub skipped: usize,
    pub flashcards: usize,
    pub matched_by: MatchMethod,
}

#[derive(Debug)]
pub struct UnmatchedSidecar {
    pub path: PathBuf,
    pub title: Option<String>,
}

/// Find sidecar lua files under `path`: accepts a `metadata.*.lua` file, a
/// single `.sdr` dir, or a library root to walk recursively.
pub fn find_sidecars(path: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect_sidecars(path, &mut found)?;
    found.sort();
    Ok(found)
}

fn collect_sidecars(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    collect_sidecars_at(path, out, 0)
}

fn collect_sidecars_at(path: &Path, out: &mut Vec<PathBuf>, depth: usize) -> Result<()> {
    if path.is_file() {
        if is_sidecar_file(path) {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }
    // `is_dir()` follows symlinks, so a link pointing at one of its own
    // ancestors recurses forever. A depth cap is the cheap, allocation-free
    // way to bound it — a real library is a handful of levels deep.
    if depth >= MAX_LIBRARY_DEPTH {
        tracing::warn!(
            path = %path.display(),
            depth,
            "library walk hit its depth cap; not descending further"
        );
        return Ok(());
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let p = entry?.path();
            // Don't follow directory symlinks at all. Descending through one
            // cannot reach a sidecar that the real tree does not also contain,
            // and refusing is what makes a cycle impossible rather than merely
            // bounded.
            let is_link = std::fs::symlink_metadata(&p)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
            if p.is_dir() {
                if is_link {
                    tracing::debug!(path = %p.display(), "skipping symlinked directory");
                    continue;
                }
                collect_sidecars_at(&p, out, depth + 1)?;
            } else if is_sidecar_file(&p) {
                out.push(p);
            }
        }
    }
    Ok(())
}

fn is_sidecar_file(p: &Path) -> bool {
    let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.starts_with("metadata.") && name.ends_with(".lua")
}

/// Match a sidecar to a library book: (a) sibling ebook file's ISBN,
/// (b) fuzzy doc_props title (jaro-winkler >= 0.85 on normalized titles).
async fn match_book(
    storage: &Storage,
    sidecar_path: &Path,
    sc: &KoSidecar,
) -> Result<Option<(Book, MatchMethod)>> {
    // Sidecar lives at <Book Name>.sdr/metadata.epub.lua; sibling ebook is
    // <Book Name>.epub next to the .sdr dir.
    if let Some(sdr_dir) = sidecar_path.parent()
        && sdr_dir.extension().and_then(|e| e.to_str()) == Some("sdr")
        && let (Some(parent), Some(stem)) = (sdr_dir.parent(), sdr_dir.file_stem())
    {
        for ext in ["epub", "EPUB"] {
            let candidate = parent.join(format!("{}.{ext}", stem.to_string_lossy()));
            if candidate.is_file()
                && let Ok(Some(isbn)) = crate::epub::epub_info(&candidate).map(|i| i.isbn)
                && let Some(book) = storage.find_book_by_isbn(&isbn).await?
            {
                return Ok(Some((book, MatchMethod::Isbn)));
            }
        }
    }

    let Some(title) = &sc.title else {
        return Ok(None);
    };
    let want = normalize(title);
    if want.is_empty() {
        return Ok(None);
    }
    let mut best: Option<(f64, Book)> = None;
    for book in storage
        .list_books(10_000, crate::storage::BookSort::LastModified)
        .await?
    {
        let have = normalize(book.title.as_deref().unwrap_or(""));
        if have.is_empty() {
            continue;
        }
        let sim = jaro_winkler(&want, &have);
        if sim >= 0.85 && best.as_ref().is_none_or(|(s, _)| sim > *s) {
            best = Some((sim, book));
        }
    }
    Ok(best.map(|(_, b)| (b, MatchMethod::Title)))
}

/// Import all sidecars under `path`. Idempotent: re-running inserts nothing
/// new (identity-hash conflict). `dry_run` reports what would happen.
pub async fn import(storage: &Storage, path: &Path, dry_run: bool) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let sidecars = find_sidecars(path)?;
    if sidecars.is_empty() {
        tracing::warn!(path = %path.display(), "no KOReader sidecars found");
        report
            .warnings
            .push(Diagnostic::no_sidecars_found(path.to_path_buf()));
        return Ok(report);
    }

    for sidecar_path in sidecars {
        // Reading the file used to be `?`, which aborted the *entire* library
        // import over one bad file — while a parse failure three lines down
        // correctly degraded to a warning. One sidecar with a stray non-UTF-8
        // byte should not cost you the other four hundred.
        let src = match std::fs::read_to_string(&sidecar_path) {
            Ok(src) => src,
            Err(e) => {
                let err = EngineError::from(e);
                tracing::warn!(path = %sidecar_path.display(), error = %err, "sidecar unreadable");
                report
                    .warnings
                    .push(Diagnostic::sidecar_unreadable(sidecar_path, &err));
                continue;
            }
        };
        let sc = match parse_sidecar(&src) {
            Ok(sc) => sc,
            Err(e) => {
                tracing::warn!(path = %sidecar_path.display(), error = %e, "sidecar unparsable");
                report
                    .warnings
                    .push(Diagnostic::sidecar_unparsable(sidecar_path, &e));
                continue;
            }
        };
        let Some((book, matched_by)) = match_book(storage, &sidecar_path, &sc).await? else {
            report.unmatched.push(UnmatchedSidecar {
                path: sidecar_path,
                title: sc.title,
            });
            continue;
        };
        let Some(book_id) = book.id else {
            // Storage always assigns an id; treat the impossible as a skip
            // rather than a panic, since this runs over a whole library.
            tracing::error!(title = %book.display_title(), "matched book has no id; skipping");
            continue;
        };

        let mut stats = BookImportStats {
            book_id,
            book_title: book.display_title().to_string(),
            inserted: 0,
            skipped: 0,
            flashcards: 0,
            matched_by,
        };
        for h in &sc.highlights {
            if dry_run {
                if storage.highlight_exists(book_id, h).await? {
                    stats.skipped += 1;
                } else {
                    stats.inserted += 1;
                    if single_word(&h.text).is_some() {
                        stats.flashcards += 1;
                    }
                }
                continue;
            }
            match storage.insert_highlight(book_id, h).await? {
                None => stats.skipped += 1,
                Some(highlight_id) => {
                    stats.inserted += 1;
                    if let Some(word) = single_word(&h.text) {
                        let context = h.note.as_deref().or(h.chapter.as_deref());
                        if storage
                            .insert_flashcard(book_id, Some(highlight_id), &word, context)
                            .await?
                        {
                            stats.flashcards += 1;
                        }
                    }
                }
            }
        }
        tracing::info!(
            book_id,
            inserted = stats.inserted,
            skipped = stats.skipped,
            flashcards = stats.flashcards,
            matched_by = %stats.matched_by,
            dry_run,
            "imported sidecar"
        );
        report.imported.push(stats);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODERN: &str = r#"
return {
    ["annotations"] = {
        [1] = {
            ["chapter"] = "Chapter One",
            ["datetime"] = "2026-01-05 21:14:08",
            ["drawer"] = "lighten",
            ["color"] = "yellow",
            ["page"] = "/body/DocFragment[8]/body/p[12]/text().0",
            ["pageno"] = 42,
            ["pos0"] = "/body/DocFragment[8]/body/p[12]/text().0",
            ["pos1"] = "/body/DocFragment[8]/body/p[12]/text().57",
            ["text"] = "History has failed us, but no matter.",
            ["note"] = "Opening line - sets the whole register.",
        },
        [2] = {
            ["chapter"] = "Chapter Two",
            ["datetime"] = "2026-01-06 08:02:11",
            ["drawer"] = "lighten",
            ["pageno"] = 55,
            ["pos0"] = "/body/DocFragment[9]/body/p[4]/text().10",
            ["pos1"] = "/body/DocFragment[9]/body/p[4]/text().19",
            ["text"] = "pachinko",
        },
        [3] = {
            -- plain bookmark, no pos0: must be skipped
            ["datetime"] = "2026-01-06 09:00:00",
            ["pageno"] = 60,
            ["text"] = "dogear",
        },
    },
    ["doc_props"] = {
        ["authors"] = "Min Jin Lee",
        ["title"] = "Pachinko",
        ["language"] = "en",
    },
    ["partial_md5_checksum"] = "0d6ba6c47caf63b8b3d1a2b3c4d5e6f7",
}
"#;

    const LEGACY: &str = r#"
return {
    ["highlight"] = {
        [42] = {
            [1] = {
                ["datetime"] = "2024-03-01 10:00:00",
                ["chapter"] = "I",
                ["text"] = "Someone must have been telling lies about Josef K.",
                ["pos0"] = "/body/DocFragment[3]/body/p[1]/text().0",
                ["pos1"] = "/body/DocFragment[3]/body/p[1]/text().49",
                ["drawer"] = "lighten",
            },
        },
        [77] = {
            [1] = {
                ["datetime"] = "2024-03-02 22:30:00",
                ["chapter"] = "III",
                ["text"] = "verdict",
                ["pos0"] = "/body/DocFragment[5]/body/p[9]/text().4",
                ["pos1"] = "/body/DocFragment[5]/body/p[9]/text().11",
            },
        },
    },
    ["bookmarks"] = {
        [1] = {
            ["datetime"] = "2024-03-01 10:00:00",
            ["notes"] = "Someone must have been telling lies about Josef K.",
            ["text"] = "Famous opening, guilt before act.",
        },
    },
    ["doc_props"] = {
        ["authors"] = "Franz Kafka",
        ["title"] = "The Trial",
    },
}
"#;

    #[test]
    fn parses_modern_annotations() {
        let sc = parse_sidecar(MODERN).unwrap();
        assert_eq!(sc.title.as_deref(), Some("Pachinko"));
        assert_eq!(sc.authors.as_deref(), Some("Min Jin Lee"));
        assert_eq!(
            sc.partial_md5.as_deref(),
            Some("0d6ba6c47caf63b8b3d1a2b3c4d5e6f7")
        );
        // Bookmark entry (no pos0) skipped.
        assert_eq!(sc.highlights.len(), 2);
        let h = &sc.highlights[0];
        assert_eq!(h.text, "History has failed us, but no matter.");
        assert_eq!(h.page, Some(42));
        assert_eq!(
            h.note.as_deref(),
            Some("Opening line - sets the whole register.")
        );
        assert_eq!(sc.highlights[1].text, "pachinko");
    }

    #[test]
    fn parses_legacy_highlight_map_with_bookmark_notes() {
        let sc = parse_sidecar(LEGACY).unwrap();
        assert_eq!(sc.title.as_deref(), Some("The Trial"));
        assert_eq!(sc.highlights.len(), 2);
        let first = &sc.highlights[0];
        assert_eq!(first.page, Some(42));
        // Note joined from bookmarks by datetime.
        assert_eq!(
            first.note.as_deref(),
            Some("Famous opening, guilt before act.")
        );
        assert_eq!(sc.highlights[1].text, "verdict");
        assert_eq!(sc.highlights[1].page, Some(77));
        assert_eq!(sc.highlights[1].note, None);
    }

    #[test]
    fn rejects_non_table_sidecars() {
        assert!(parse_sidecar("return 42").is_err());
        assert!(parse_sidecar("this is not lua").is_err());
    }

    #[test]
    fn sandbox_has_no_stdlib() {
        // os/io must not exist in the sidecar VM.
        assert!(parse_sidecar("return { x = os.time() }").is_err());
    }

    #[tokio::test]
    async fn import_is_idempotent_and_extracts_flashcards() {
        use crate::book::Book;

        let dir = std::env::temp_dir().join(format!("rb-ko-test-{}", std::process::id()));
        let sdr = dir.join("Pachinko.sdr");
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(sdr.join("metadata.epub.lua"), MODERN).unwrap();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        s.upsert_book(&Book {
            title: Some("Pachinko".into()),
            authors: vec!["Min Jin Lee".into()],
            ..Default::default()
        })
        .await
        .unwrap();

        let report = import(&s, &dir, false).await.unwrap();
        assert_eq!(report.imported.len(), 1);
        let stats = &report.imported[0];
        assert_eq!(stats.inserted, 2);
        assert_eq!(stats.skipped, 0);
        // "pachinko" is a single-word highlight -> flashcard candidate.
        assert_eq!(stats.flashcards, 1);

        // Second run: everything skipped.
        let report2 = import(&s, &dir, false).await.unwrap();
        assert_eq!(report2.imported[0].inserted, 0);
        assert_eq!(report2.imported[0].skipped, 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn unmatched_sidecar_reported_not_fatal() {
        let dir = std::env::temp_dir().join(format!("rb-ko-unmatched-{}", std::process::id()));
        let sdr = dir.join("Nothing.sdr");
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(sdr.join("metadata.epub.lua"), LEGACY).unwrap();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let report = import(&s, &dir, false).await.unwrap();
        assert!(report.imported.is_empty());
        assert_eq!(report.unmatched.len(), 1);
        assert_eq!(report.unmatched[0].title.as_deref(), Some("The Trial"));

        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- hostile input ----------------------------------------------------
    //
    // These four cover read-path defects found by inspection. Each one is a
    // realistic file: KOReader itself produces holey tables after a sync
    // conflict, and a library root is a directory the user chose, not one we
    // control.

    /// A sidecar is a file we did not write, and `StdLib::NONE` removes the
    /// standard library but not the ability to loop. Without an instruction
    /// budget this hangs the import — and therefore the app — forever.
    #[test]
    fn a_runaway_loop_is_killed_rather_than_hanging_the_import() {
        let started = std::time::Instant::now();
        let err = parse_sidecar("return (function() while true do end end)()")
            .expect_err("a non-terminating chunk must not be allowed to return");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "budget did not bite: took {:?}",
            started.elapsed()
        );
        assert!(
            err.to_string().contains("instruction budget"),
            "unexpected error: {err}"
        );
    }

    /// `sequence_values` stops at the first gap, so `{[1]=..,[3]=..}` used to
    /// yield one highlight and silently drop the rest. That is data loss on
    /// someone's reading notes, with nothing on screen looking wrong.
    #[test]
    fn a_hole_in_the_annotations_table_does_not_truncate_the_import() {
        let holey = r#"
return {
    ["annotations"] = {
        [1] = { ["text"] = "first",  ["pos0"] = "/a.0", ["datetime"] = "2026-01-01 00:00:00" },
        [3] = { ["text"] = "third",  ["pos0"] = "/c.0", ["datetime"] = "2026-01-03 00:00:00" },
        [4] = { ["text"] = "fourth", ["pos0"] = "/d.0", ["datetime"] = "2026-01-04 00:00:00" },
    },
    ["doc_props"] = { ["title"] = "Holey" },
}
"#;
        let sc = parse_sidecar(holey).unwrap();
        let texts: Vec<&str> = sc.highlights.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["first", "third", "fourth"],
            "entries after the gap were dropped"
        );
    }

    /// One unreadable file used to abort the whole run via `?`, while a *parse*
    /// failure three lines away correctly degraded to a warning. A single
    /// stray byte should not cost you the rest of the library.
    #[tokio::test]
    async fn one_unreadable_sidecar_does_not_abort_the_whole_import() {
        let tmp = tempfile::tempdir().unwrap();

        // Invalid UTF-8, so `read_to_string` fails rather than `parse_sidecar`.
        let bad = tmp.path().join("Bad.sdr");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("metadata.epub.lua"), [0xff, 0xfe, 0x00, 0x01]).unwrap();

        let good = tmp.path().join("The Trial.sdr");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(good.join("metadata.epub.lua"), LEGACY).unwrap();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        s.upsert_book(&Book {
            title: Some("The Trial".into()),
            authors: vec!["Franz Kafka".into()],
            ..Default::default()
        })
        .await
        .unwrap();

        let report = import(&s, tmp.path(), false)
            .await
            .expect("an unreadable file must degrade, not abort");

        assert_eq!(
            report.imported.len(),
            1,
            "the readable sidecar should still have imported"
        );
        assert!(report.imported[0].inserted > 0);
        assert!(
            report.warnings.iter().any(|d| matches!(
                d.kind,
                crate::diagnostic::DiagnosticKind::SidecarUnreadable { .. }
            )),
            "expected an unreadable-sidecar diagnostic, got {:?}",
            report.warnings
        );
    }

    /// `is_dir()` follows symlinks, so a link to an ancestor is unbounded
    /// recursion. The walk must terminate and still find the real sidecar.
    #[cfg(unix)]
    #[test]
    fn a_symlink_cycle_does_not_hang_the_library_walk() {
        let tmp = tempfile::tempdir().unwrap();
        let sdr = tmp.path().join("Book.sdr");
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(sdr.join("metadata.epub.lua"), LEGACY).unwrap();

        // nested/loop -> the library root
        let nested = tmp.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::os::unix::fs::symlink(tmp.path(), nested.join("loop")).unwrap();

        let found = find_sidecars(tmp.path()).expect("walk must terminate");
        assert_eq!(
            found.len(),
            1,
            "expected exactly the one real sidecar, got {found:?}"
        );
    }
}
