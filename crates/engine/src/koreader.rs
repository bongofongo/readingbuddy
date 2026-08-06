use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mlua::{Lua, LuaOptions, StdLib, Table, Value};

use crate::book::Book;
use crate::diagnostic::Diagnostic;
use crate::error::{EngineError, Result};
use crate::flashcards::single_word;
use crate::matching::{Prepared, Query};
use crate::storage::{LinkedBy, NewHighlight, Source, Storage, ko_datetime_to_unix};

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

#[cfg(test)]
use crate::matching::AUTO_MATCH;
/// The floor of the candidate band. The rule itself lives in
/// [`crate::matching`], which is where the reasons are; the callers no longer
/// compare against `AUTO_MATCH` by hand, because whether a match may be made
/// silently is now the matcher's answer rather than a threshold anyone can
/// re-derive.
pub(crate) use crate::matching::CANDIDATE_MIN;

/// Parsed KOReader `.sdr` sidecar (`metadata.epub.lua` etc.).
#[derive(Debug, Default)]
pub struct KoSidecar {
    pub title: Option<String>,
    pub authors: Option<String>,
    pub language: Option<String>,
    pub partial_md5: Option<String>,
    /// Root `doc_pages`. The reader's own page count, and on a sidecar with no
    /// `stats` block the only one there is — which is the common case for the
    /// books item 3 has to create, since `stats` is residue from the pre-DB
    /// statistics plugin. Equal to `stats.pages` in current files, but they have
    /// been seen to diverge by one across a re-render, so they are kept apart.
    pub doc_pages: Option<i64>,
    /// Root `percent_finished`, 0.0..=1.0.
    pub percent_finished: Option<f64>,
    /// The device's own status/rating/review.
    pub summary: Option<KoSummary>,
    pub stats: Option<KoStats>,
    pub highlights: Vec<NewHighlight>,
    /// Entries that are highlights on the device but carry an anchor we cannot
    /// store: KOReader writes `pos0` as a **table** — a page plus coordinates —
    /// on a paging document (PDF, DjVu), where a reflowable one gets a cre
    /// xpointer string. See [`Anchor`].
    ///
    /// A **count, not a collection**, and that is the decision rather than an
    /// economy: the degradation is per *file*
    /// ([`DiagnosticKind::SidecarAnchorsUnsupported`]). A 300-highlight PDF that
    /// emitted 300 diagnostics would have replaced silence with noise, and noise
    /// is not the improvement this was after.
    ///
    /// Zero on every EPUB sidecar there has ever been, which is why it defaults
    /// and why nothing downstream had to learn about it.
    ///
    /// [`DiagnosticKind::SidecarAnchorsUnsupported`]: crate::diagnostic::DiagnosticKind::SidecarAnchorsUnsupported
    pub unsupported_anchors: usize,
}

/// KOReader's per-book reading status.
///
/// `Other` is not a nicety: a future KOReader adding a status must degrade to a
/// value we carry through, never fail somebody's import. The three known
/// variants come from the status toggle's `args` in `bookstatuswidget.lua`, and
/// they are lowercase — the doc-comment in that same file showing `"Reading"`
/// is stale. Only `reading` and `complete` have been seen in the wild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KoStatus {
    Reading,
    Abandoned,
    Complete,
    Other(String),
}

impl KoStatus {
    /// True when the value was not one KOReader is known to write. The import
    /// turns this into a `Diagnostic`, which is the only signal we would ever
    /// get that the device grew a status we do not model.
    pub fn is_unknown(&self) -> bool {
        matches!(self, KoStatus::Other(_))
    }
}

impl From<&str> for KoStatus {
    fn from(s: &str) -> Self {
        match s {
            "reading" => KoStatus::Reading,
            "abandoned" => KoStatus::Abandoned,
            "complete" => KoStatus::Complete,
            other => KoStatus::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for KoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Round-trips through `From<&str>`, so an unknown status survives a
        // parse/render cycle unchanged rather than collapsing to a placeholder.
        match self {
            KoStatus::Reading => write!(f, "reading"),
            KoStatus::Abandoned => write!(f, "abandoned"),
            KoStatus::Complete => write!(f, "complete"),
            KoStatus::Other(s) => write!(f, "{s}"),
        }
    }
}

/// The `summary` subtable: the device's status, rating and review.
///
/// None of this is persisted yet — `readings` (build item 4) is where it
/// belongs, and parking it on `books` now would only have to be undone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KoSummary {
    pub status: Option<KoStatus>,
    /// 1..=5. **Absent means unrated, not zero** — KOReader deletes the key
    /// when the user clears the rating rather than writing 0.
    pub rating: Option<i64>,
    /// The user's own review. Real in KOReader's source, but unwritten by any
    /// build we have seen — expect `None`.
    pub note: Option<String>,
    /// `"%Y-%m-%d"`. Date only, unlike annotation datetimes.
    pub modified: Option<String>,
}

/// The `stats` subtable.
///
/// Residue rather than live data: current KOReader keeps per-book statistics in
/// `statistics.sqlite3` and only ever *reads* this block, once, to migrate a
/// pre-DB sidecar. It survives rewrites because DocSettings re-serialises
/// whatever it loaded.
///
/// `md5` and `total_time_in_sec` are absent on every 2024.11+ sidecar we have;
/// they are kept as `Option` so an older file still round-trips. **Nothing may
/// depend on them** — the book identifier is the root `partial_md5_checksum`,
/// which is what `device_books` keys on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KoStats {
    pub title: Option<String>,
    pub authors: Option<String>,
    pub language: Option<String>,
    pub series: Option<String>,
    pub pages: Option<i64>,
    /// Counts annotations *without* a note; `notes` counts those *with* one.
    pub highlights: Option<i64>,
    pub notes: Option<i64>,
    pub md5: Option<String>,
    pub total_time_in_sec: Option<i64>,
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
        doc_pages: get_int(&root, "doc_pages"),
        percent_finished: get_f64(&root, "percent_finished"),
        // `summary`, `stats` and `percent_finished` are DocSettings *root*
        // keys, written by subsystems that never look at the annotations
        // layout. Reading them before the layout dispatch below is what makes a
        // legacy sidecar carry them too — pinned by `Gen-Summary-Legacy`.
        summary: get_table(&root, "summary").map(|t| parse_summary(&t)),
        stats: get_table(&root, "stats").map(|t| parse_stats(&t)),
        ..Default::default()
    };
    if let Some(props) = get_table(&root, "doc_props") {
        sidecar.title = get_str(&props, "title");
        sidecar.authors = get_str(&props, "authors");
        sidecar.language = get_str(&props, "language");
    }

    if let Some(annotations) = get_table(&root, "annotations") {
        let parsed = parse_annotations(&annotations)?;
        sidecar.highlights = parsed.highlights;
        sidecar.unsupported_anchors = parsed.unsupported_anchors;
    } else if let Some(highlight) = get_table(&root, "highlight") {
        let notes_by_datetime = get_table(&root, "bookmarks")
            .map(|b| bookmark_notes(&b))
            .unwrap_or_default();
        let parsed = parse_legacy(&highlight, &notes_by_datetime)?;
        sidecar.highlights = parsed.highlights;
        sidecar.unsupported_anchors = parsed.unsupported_anchors;
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

/// A completed book is written as the bare integer `1`, not `1.0`, so this must
/// not require a Lua float. `mlua`'s `f64` conversion accepts either.
fn get_f64(t: &Table, key: &str) -> Option<f64> {
    t.get::<Option<f64>>(key).ok().flatten()
}

fn get_table(t: &Table, key: &str) -> Option<Table> {
    t.get::<Option<Table>>(key).ok().flatten()
}

fn parse_summary(t: &Table) -> KoSummary {
    KoSummary {
        status: get_str(t, "status").map(|s| KoStatus::from(s.as_str())),
        rating: get_int(t, "rating"),
        note: get_str(t, "note"),
        modified: get_str(t, "modified"),
    }
}

fn parse_stats(t: &Table) -> KoStats {
    KoStats {
        title: get_str(t, "title"),
        authors: get_str(t, "authors"),
        language: get_str(t, "language"),
        series: get_str(t, "series"),
        pages: get_int(t, "pages"),
        highlights: get_int(t, "highlights"),
        notes: get_int(t, "notes"),
        md5: get_str(t, "md5"),
        total_time_in_sec: get_int(t, "total_time_in_sec"),
    }
}

/// A `pos0`, as the file actually carries it.
///
/// The distinction that matters is not string-versus-absent, which is what the
/// parser used to see, but **three** cases: an anchor we can store, no anchor at
/// all, and an anchor we can read the shape of and cannot represent.
enum Anchor {
    /// A cre xpointer, e.g. `/body/DocFragment[18]/body/p[135]/text().158`. The
    /// only shape `highlights.pos0` can hold, and the only one `identity_hash`
    /// has ever been fed.
    Xpointer(String),
    /// No `pos0`. A plain bookmark entry — 14 of the 361 annotations in the real
    /// library — which is not a highlight and never was. Ordinary, and silent.
    Absent,
    /// Present, and a **table**. This is KOReader's paging-document anchor: a
    /// page number plus coordinates (and a sibling `pboxes` array of rectangles)
    /// rather than a position in a text stream, because a scanned page has no
    /// text stream to point into.
    ///
    /// We cannot store it, so the entry does not import — but it is a *highlight
    /// the user made*, not a bookmark, and the difference is exactly what the
    /// old code could not see.
    Unsupported,
}

/// Read `pos0` without deciding what an entry is.
///
/// **Only the table case is new, and deliberately only that.** Every other Lua
/// value still goes through `get_str`, which coerces a number to its digits and
/// filters the empty string exactly as it did before this function existed.
/// Narrowing the change to one value type is what makes "no reflowable sidecar
/// imports differently" a property of the code rather than a claim about which
/// fixtures happen to be committed.
fn anchor(item: &Table) -> Anchor {
    if matches!(item.get::<Value>("pos0"), Ok(Value::Table(_))) {
        return Anchor::Unsupported;
    }
    match get_str(item, "pos0") {
        Some(pos0) => Anchor::Xpointer(pos0),
        None => Anchor::Absent,
    }
}

/// What one `annotations` (or legacy `highlight`) entry turned out to be.
enum Entry {
    Highlight(Box<NewHighlight>),
    /// A bookmark, or an entry with nothing to store. Ordinary; goes uncounted.
    NotAHighlight,
    /// A highlight whose [`Anchor`] we cannot represent. Counted, and reported
    /// once per file.
    UnsupportedAnchor,
}

fn entry_to_highlight(item: &Table, page: Option<i64>) -> Entry {
    // `text` is tested **before** the anchor, and the order is the honest one:
    // an entry with no text would not import however good its anchor was, so
    // counting it would overstate what the anchor cost us. The diagnostic's
    // claim is "this many highlights did not arrive *because of the anchor*".
    let Some(text) = get_str(item, "text") else {
        return Entry::NotAHighlight;
    };
    // Modern `annotations` mixes highlights and plain bookmarks; a real
    // highlight always carries a pos0. On a reflowable document that is a cre
    // xpointer string — on a paging one it is a table, which is a highlight we
    // cannot anchor rather than a bookmark we should ignore.
    let pos0 = match anchor(item) {
        Anchor::Xpointer(pos0) => pos0,
        Anchor::Absent => return Entry::NotAHighlight,
        Anchor::Unsupported => return Entry::UnsupportedAnchor,
    };
    Entry::Highlight(Box::new(NewHighlight {
        text,
        chapter: get_str(item, "chapter"),
        page: get_int(item, "pageno").or(get_int(item, "page")).or(page),
        pos0: Some(pos0),
        pos1: get_str(item, "pos1"),
        ko_datetime: get_str(item, "datetime"),
        ko_datetime_updated: get_str(item, "datetime_updated"),
        color: get_str(item, "color").or_else(|| get_str(item, "drawer")),
        note: get_str(item, "note"),
        source: "koreader".to_string(),
    }))
}

/// What one layout's entries came to: the highlights, and the ones left behind.
///
/// A struct rather than a bare `Vec` because the second number has to travel out
/// of here — it used to be discarded inside the `if let Some(h)` that both
/// parsers wrote, which is precisely where a PDF library's highlights went.
struct Parsed {
    highlights: Vec<NewHighlight>,
    unsupported_anchors: usize,
}

fn parse_annotations(annotations: &Table) -> Result<Parsed> {
    // `sequence_values` stops dead at the first missing index, so a table like
    // `{[1]=.., [3]=..}` would silently yield one highlight and drop the rest.
    // KOReader produces exactly that shape after a sync conflict, and silent
    // data loss on someone's reading notes is the worst outcome here. Iterate
    // the pairs and sort, the same way `parse_legacy` already does.
    let mut indexed: Vec<(i64, NewHighlight)> = Vec::new();
    let mut unsupported_anchors = 0;
    for pair in annotations.pairs::<i64, Table>() {
        let (idx, item) =
            pair.map_err(|e| EngineError::Sidecar(format!("annotation entry: {e}")))?;
        match entry_to_highlight(&item, None) {
            Entry::Highlight(h) => indexed.push((idx, *h)),
            Entry::UnsupportedAnchor => unsupported_anchors += 1,
            Entry::NotAHighlight => {}
        }
    }
    // Lua map iteration order is arbitrary; the index is the only ordering the
    // file actually carries.
    indexed.sort_by_key(|(idx, _)| *idx);
    Ok(Parsed {
        highlights: indexed.into_iter().map(|(_, h)| h).collect(),
        unsupported_anchors,
    })
}

/// Legacy layout: `highlight[pageno][idx] = { datetime, text, pos0, ... }`.
/// User notes live separately in `bookmarks`, joined here by datetime.
fn parse_legacy(highlight: &Table, notes_by_datetime: &HashMap<String, String>) -> Result<Parsed> {
    let mut out = Vec::new();
    let mut unsupported_anchors = 0;
    for pair in highlight.pairs::<i64, Table>() {
        let (page, items) =
            pair.map_err(|e| EngineError::Sidecar(format!("highlight page: {e}")))?;
        for item in items.pairs::<i64, Table>() {
            let (_, item) =
                item.map_err(|e| EngineError::Sidecar(format!("highlight item: {e}")))?;
            match entry_to_highlight(&item, Some(page)) {
                Entry::Highlight(mut h) => {
                    if h.note.is_none()
                        && let Some(dt) = &h.ko_datetime
                    {
                        h.note = notes_by_datetime.get(dt).cloned();
                    }
                    out.push(*h);
                }
                // The rule is shared with the modern layout rather than
                // restated, so a pre-2024 PDF sidecar — doubly unobserved, and
                // therefore the one nobody would have written a branch for —
                // degrades the same way by construction.
                Entry::UnsupportedAnchor => unsupported_anchors += 1,
                Entry::NotAHighlight => {}
            }
        }
    }
    // Page-keyed map iteration order is arbitrary; make output deterministic.
    out.sort_by_key(|h| (h.page, h.ko_datetime.clone()));
    Ok(Parsed {
        highlights: out,
        unsupported_anchors,
    })
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
    /// A `device_books` row already links this sidecar's `partial_md5` to a
    /// book. Strongest of the three: it is a decision that was made once and
    /// recorded, not a guess re-made on every import.
    Md5,
    /// A sibling `.epub` next to the `.sdr` dir carried an ISBN we know.
    Isbn,
    /// The shared matcher was sure enough on `doc_props` title and authors to
    /// link without asking.
    Title,
    /// Nothing matched, so the book was created from the sidecar's own
    /// metadata. Only [`import_book_from_sidecar`] produces this — an ordinary
    /// import still reports the sidecar as unmatched rather than inventing a
    /// book behind the user's back.
    New,
}

impl std::fmt::Display for MatchMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchMethod::Md5 => write!(f, "md5"),
            MatchMethod::Isbn => write!(f, "isbn"),
            MatchMethod::Title => write!(f, "title"),
            MatchMethod::New => write!(f, "new"),
        }
    }
}

/// A library book that looks like it might be this sidecar's, but not enough to
/// link without asking.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchCandidate {
    pub book_id: i64,
    pub title: String,
    /// Jaro-winkler similarity of the normalized titles, in
    /// `CANDIDATE_MIN..AUTO_MATCH`.
    pub score: f64,
}

#[derive(Debug)]
pub struct BookImportStats {
    pub book_id: i64,
    pub book_title: String,
    pub inserted: usize,
    /// Rows already present whose **device-owned** payload (`ko_note`, `color`,
    /// `chapter`, `page`) the sidecar disagreed with, and which were refreshed
    /// toward the device. Ours — `annotation` — is never in this count.
    ///
    /// It exists because without it a note edited on the device is reported as
    /// `skipped`, indistinguishable from "already had it, identical". Silence
    /// looked exactly like success.
    pub updated: usize,
    /// Present and identical. Narrower than it used to be: what would now be
    /// counted as `updated` used to land here.
    pub skipped: usize,
    pub flashcards: usize,
    pub matched_by: MatchMethod,
    /// The device's reading state, as the sidecar reported it.
    ///
    /// Persisted since migration `0005`, onto the reading rather than the book —
    /// `readings.ko_status`/`ko_percent`/`ko_rating`, the device-owned mirror.
    /// These fields stay in the report because they are what the caller prints,
    /// and because the goldens assert them: without them the four device-state
    /// fixtures would be green while parsing nothing.
    pub percent_finished: Option<f64>,
    pub status: Option<KoStatus>,
    pub rating: Option<i64>,
}

#[derive(Debug)]
pub struct UnmatchedSidecar {
    pub path: PathBuf,
    pub title: Option<String>,
    /// The sidecar's root `partial_md5_checksum`, so a caller can act on this
    /// entry — `link_sidecar` it to an existing book — without reopening and
    /// re-parsing the file it was just told about.
    pub partial_md5: Option<String>,
    /// Library books in the ambiguous band. Empty is the ordinary case; a
    /// non-empty list is the difference between "unmatched" and "unmatched, and
    /// here is what it probably is".
    pub candidates: Vec<MatchCandidate>,
}

/// The result of pulling one book in from the reader.
///
/// Not a bare [`BookImportStats`]: a pull can degrade — a sidecar with no
/// `partial_md5_checksum` imports but cannot be made idempotent — and a typed
/// diagnostic needs somewhere to travel. Not an [`ImportReport`] either, whose
/// `Vec` would make every caller index `[0]` for a function that handles
/// exactly one book.
#[derive(Debug)]
pub struct PullReport {
    pub stats: BookImportStats,
    pub warnings: Vec<Diagnostic>,
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

/// Is this one of KOReader's live sidecar files?
///
/// **`metadata.epub.lua.old` is excluded, and that is load-bearing.**
/// `docsettings.lua:340` writes a backup on every flush, so 9 of 10 real `.sdr`
/// directories have one — and it is a *previous* state of the same annotations.
/// Reading it would resurrect highlights the user deleted on the device. The
/// `.lua` suffix test already refuses it; this doc comment and the tests around
/// it are what stop the exclusion from being re-lost as an accident.
pub fn is_sidecar_file(p: &Path) -> bool {
    let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.starts_with("metadata.") && name.ends_with(".lua")
}

/// Match a sidecar to a library book: (a) a recorded `device_books` link on the
/// sidecar's `partial_md5`, (b) sibling ebook file's ISBN, (c) the shared
/// [`crate::matching`] scan over `doc_props` title and authors.
///
/// Public so [`crate::device`] can show the same verdict a scan's later import
/// would reach. Only `partial_md5`, `title` and `authors` are read off `sc`,
/// and `sidecar_seen` caches all three verbatim — which is what lets the scan
/// call this from its cached facts rather than a fresh parse.
pub async fn match_book(
    storage: &Storage,
    sidecar_path: &Path,
    sc: &KoSidecar,
) -> Result<Option<(Book, MatchMethod)>> {
    // The recorded link goes first, and it is the only branch that is not a
    // guess. It also covers what the other two cannot: a sidecar filed under
    // KOReader's `hash` or `dir` storage layout has no sibling ebook at all,
    // and a title the user edited on either side defeats the fuzzy match.
    if let Some(md5) = &sc.partial_md5
        && let Some(book) = storage.find_book_by_partial_md5(md5).await?
    {
        return Ok(Some((book, MatchMethod::Md5)));
    }

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

    let mut scored = title_scores(storage, sc).await?;
    if scored.first().is_some_and(|s| s.can_auto) {
        return Ok(Some((scored.remove(0).book, MatchMethod::Title)));
    }
    Ok(None)
}

/// Every library book that could be this sidecar's, best first.
///
/// One scan shared by [`match_book`] and [`match_candidates`], because the two
/// answers have to come from the same ordering: a book the auto-match rejected
/// showing up as a candidate is the whole point, and a book it accepted showing
/// up as one as well would be an invitation to link what is already linked.
async fn title_scores(storage: &Storage, sc: &KoSidecar) -> Result<Vec<Scored>> {
    let authors = split_authors(sc.authors.as_deref());
    scores_for(storage, &Query::new(sc.title.as_deref(), &authors)).await
}

/// A library book and what [`crate::matching`] made of it.
pub(crate) struct Scored {
    pub score: f64,
    pub can_auto: bool,
    pub book: Book,
}

/// The scan, given whatever the other system knows about the book.
///
/// `pub(crate)` because a Goodreads row, a calibre row and an owned file need
/// the *same* matcher a sidecar gets: `docs/decisions.md` names "do not invent
/// a second matcher" under **Files**, and it applies wherever books are
/// matched. Two fuzzy matchers would be two answers to "is this the book I
/// already have", and the one that disagreed would be the one that made the
/// duplicate.
///
/// The books this returns are the ones that survived the rule — a coincidence
/// of letters is **absent**, not present with a low number. That is what lets a
/// caller say *nothing here looks like it* instead of naming its best guess.
pub(crate) async fn scores_for(storage: &Storage, query: &Query<'_>) -> Result<Vec<Scored>> {
    let Some(prepared) = Prepared::new(query) else {
        return Ok(Vec::new());
    };
    // Every book, and it has to be: this decides whether a sidecar is a book we
    // already have, so a book outside the window is a book that gets a duplicate
    // rather than a match. `10_000` was a cap wearing a limit's clothes, and the
    // library that crossed it would have been told nothing.
    let mut scored: Vec<Scored> = storage
        .list_books(&crate::BookQuery::default())
        .await?
        .into_iter()
        .filter_map(|book| {
            let v = prepared.compare(&book)?;
            Some(Scored {
                score: v.score,
                can_auto: v.can_auto,
                book,
            })
        })
        .collect();
    // `total_cmp` rather than `partial_cmp().unwrap()`: a NaN here would panic
    // mid-import. Ties break on book id so the order is deterministic and a
    // caller showing "the best candidate" shows the same one twice running.
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.book.id.cmp(&b.book.id)));
    Ok(scored)
}

/// The band, out of a scan already run.
///
/// A pure filter, so a caller that needs both the auto-match and the band pays
/// for **one** pass over the library rather than two. `calibre.rs` used to run
/// the scan twice per row, which on a four-hundred-book library is eight
/// hundred loads of the whole shelf.
pub(crate) fn band(scored: Vec<Scored>) -> Vec<MatchCandidate> {
    scored
        .into_iter()
        .filter(|s| !s.can_auto && s.score >= CANDIDATE_MIN)
        .filter_map(|s| {
            Some(MatchCandidate {
                book_id: s.book.id?,
                title: s.book.display_title().to_string(),
                score: s.score,
            })
        })
        .collect()
}

/// Library books in the ambiguous band, best first.
///
/// A book the matcher would link outright is not here — the caller has already
/// matched it — and neither is one it refused, which is the half that changed:
/// a coincidence of letters is now absent rather than present with a plausible
/// number. What is left is the case worth asking about: a variant title — a
/// subtitle dropped, a translator's spelling, "The" gained or lost — that
/// quietly became a second copy of a book already on the shelf, with nothing
/// said and nothing to act on. Offering it is what turns `unmatched` from a
/// dead end into a decision.
pub async fn match_candidates(storage: &Storage, sc: &KoSidecar) -> Result<Vec<MatchCandidate>> {
    Ok(band(title_scores(storage, sc).await?))
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
            // Unmatched is a state to act on, not a dead end: carry the
            // near-misses and the device key so the caller can link this
            // sidecar to a book, or pull it in as a new one, without going back
            // to the file.
            let candidates = match_candidates(storage, &sc).await?;
            report.unmatched.push(UnmatchedSidecar {
                path: sidecar_path,
                title: sc.title,
                partial_md5: sc.partial_md5,
                candidates,
            });
            continue;
        };
        let Some(book_id) = book.id else {
            // Storage always assigns an id; treat the impossible as a skip
            // rather than a panic, since this runs over a whole library.
            tracing::error!(title = %book.display_title(), "matched book has no id; skipping");
            continue;
        };

        // Record the link so the next import matches on md5 rather than
        // re-guessing from a title the user may edit on either side. Skipped
        // under `dry_run`, which must not write. Item 3's `link_sidecar`
        // replaces this call site, not the table.
        if !dry_run && let Some(md5) = &sc.partial_md5 {
            storage
                .link_device_book(md5, book_id, LinkedBy::Auto)
                .await?;
        }

        let target = ImportTarget {
            book_id,
            book_title: book.display_title().to_string(),
            matched_by,
        };
        let stats = import_into(
            storage,
            target,
            &sc,
            &sidecar_path,
            dry_run,
            &mut report.warnings,
        )
        .await?;
        report.imported.push(stats);
    }
    Ok(report)
}

/// Which book a parsed sidecar is about to be written into, and how that was
/// decided. Bundled so [`import_into`] keeps a readable arity.
struct ImportTarget {
    book_id: i64,
    book_title: String,
    matched_by: MatchMethod,
}

/// Write one parsed sidecar's highlights into an already-chosen book.
///
/// The single place the insert/refresh/skip decision is made. `import` and
/// [`import_book_from_sidecar`] differ only in how they pick the book, and
/// letting them differ in how they *count* would put the two paths' goldens
/// quietly out of step.
async fn import_into(
    storage: &Storage,
    target: ImportTarget,
    sc: &KoSidecar,
    sidecar_path: &Path,
    dry_run: bool,
    warnings: &mut Vec<Diagnostic>,
) -> Result<BookImportStats> {
    let ImportTarget {
        book_id,
        book_title,
        matched_by,
    } = target;

    let summary = sc.summary.as_ref();
    let status = summary.and_then(|s| s.status.clone());
    if let Some(KoStatus::Other(value)) = &status {
        tracing::warn!(
            path = %sidecar_path.display(),
            status = %value,
            "unknown KOReader status; imported as-is"
        );
        warnings.push(Diagnostic::unknown_device_status(
            sidecar_path.to_path_buf(),
            value,
        ));
    }

    // Highlights the device made that we could not store. Emitted **here**, and
    // not where the file was parsed, on `UnknownDeviceStatus`'s precedent: both
    // are facts derived from the parse alone, and this is the one place `import`
    // and `import_book_from_sidecar` both reach, so neither path can grow a
    // second opinion about how a degradation is reported.
    //
    // One diagnostic, carrying the count — see the variant's doc for why not one
    // per entry.
    if sc.unsupported_anchors > 0 {
        // A count and a path. Never the text: a highlight is the user's private
        // reading and nothing here may rise above `trace!`.
        tracing::warn!(
            path = %sidecar_path.display(),
            entries = sc.unsupported_anchors,
            "sidecar highlights are anchored to a page and coordinates; cannot store them"
        );
        warnings.push(Diagnostic::sidecar_anchors_unsupported(
            sidecar_path.to_path_buf(),
            sc.unsupported_anchors,
        ));
    }

    let mut stats = BookImportStats {
        book_id,
        book_title,
        inserted: 0,
        updated: 0,
        skipped: 0,
        flashcards: 0,
        matched_by,
        percent_finished: sc.percent_finished,
        status,
        rating: summary.and_then(|s| s.rating),
    };
    for h in &sc.highlights {
        if dry_run {
            if storage.highlight_exists(book_id, h).await? {
                // A preview that reported a device edit as "already known"
                // would disagree with the import it is previewing, which is
                // the one thing a dry run must not do.
                if storage.device_fields_differ(book_id, h).await? {
                    stats.updated += 1;
                } else {
                    stats.skipped += 1;
                }
            } else {
                stats.inserted += 1;
                if single_word(&h.text).is_some() {
                    stats.flashcards += 1;
                }
            }
            continue;
        }
        match storage.insert_highlight(book_id, h).await? {
            // Already present. KOReader owns `ko_note`/`color`/`chapter`/
            // `page`, so the sidecar wins on those — and only a row that
            // genuinely changed counts as `updated`.
            None => {
                if storage.refresh_device_fields(book_id, h).await? {
                    stats.updated += 1;
                } else {
                    stats.skipped += 1;
                }
            }
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
    if !dry_run {
        persist_device_state(storage, book_id, sc, &stats).await?;
    }

    // `summary.note` is the user's review — private reading, the same class
    // as highlight text and note bodies. It is deliberately absent from
    // every field here and must never rise above `trace!`. Status, rating
    // and progress are device state, not prose, and are fine to log.
    tracing::info!(
        book_id,
        inserted = stats.inserted,
        updated = stats.updated,
        skipped = stats.skipped,
        flashcards = stats.flashcards,
        matched_by = %stats.matched_by,
        status = stats.status.as_ref().map(|s| s.to_string()),
        rating = stats.rating,
        percent_finished = stats.percent_finished,
        dry_run,
        "imported sidecar"
    );
    Ok(stats)
}

/// Mirror the sidecar's reading state onto a reading, and attribute the
/// highlights we just imported.
///
/// **An import opens a reading only when the sidecar carries device state** — a
/// status, a rating or a `percent_finished`. A sidecar with none of those says
/// nothing about whether the book was ever read, so it opens nothing and its
/// highlights stay unattributed, which is a state that can be acted on rather
/// than a guess that cannot be undone. The reading starts at the **earliest
/// `datetime` seen**: KOReader does not record when a book was opened, and the
/// first annotation is the earliest moment we can prove the user was in it.
///
/// It opens one only when the book has *no* reading — see
/// [`Storage::ensure_reading`]. "None open" would mean a `complete` sidecar
/// added a reading on every re-import, and import is the one path whose contract
/// is idempotency.
///
/// Never called under `dry_run` — the rule `link_device_book` already follows.
async fn persist_device_state(
    storage: &Storage,
    book_id: i64,
    sc: &KoSidecar,
    stats: &BookImportStats,
) -> Result<()> {
    if stats.status.is_none() && stats.percent_finished.is_none() && stats.rating.is_none() {
        return Ok(());
    }

    let started = sc
        .highlights
        .iter()
        .filter_map(|h| h.ko_datetime.as_deref())
        .filter_map(ko_datetime_to_unix)
        .min();
    storage.ensure_reading(book_id, started, "koreader").await?;

    storage
        .set_device_state(
            book_id,
            stats.status.as_ref(),
            stats.percent_finished,
            stats.rating,
        )
        .await?;

    // Ours tracks theirs, but is not the same field: `complete` on the device
    // closes the reading, `abandoned` marks it without closing it (an abandoned
    // book is one you might still pick up), `reading` says nothing new, and an
    // unrecognised status leaves ours entirely alone — the `UnknownDeviceStatus`
    // diagnostic has already fired for it.
    match &stats.status {
        Some(KoStatus::Complete) => {
            storage.finish_reading(book_id).await?;
        }
        Some(KoStatus::Abandoned) => {
            storage.abandon_reading(book_id).await?;
        }
        Some(KoStatus::Reading) | Some(KoStatus::Other(_)) | None => {}
    }

    storage.attribute_highlights(book_id).await?;
    Ok(())
}

// ---- pull, link, merge -------------------------------------------------------

/// Create a book from a sidecar's own metadata and import its highlights.
///
/// The verb the device screen is built around, and until now the engine had no
/// support for it at all: an unmatched sidecar was reported and dropped, so
/// getting a book off the reader meant adding it by title or ISBN first and
/// then importing.
///
/// **Fully offline.** No provider enrichment — deferred by decision. Title,
/// authors, page count and language come from the sidecar's `stats` block,
/// falling back to `doc_props` and `doc_pages`; the book has no ISBN, cover or
/// description, and the user enriches it later via search and then merges.
///
/// **Idempotency cannot come from `upsert_book`.** That branches
/// isbn_10 → isbn_13 → plain unconditional insert, and a sidecar-seeded book has
/// neither ISBN — so it takes the third branch every time and a second pull
/// would create a second book. The guard is `device_books` keyed on
/// `partial_md5`: known → reuse that book, unknown → create and record the
/// mapping.
pub async fn import_book_from_sidecar(storage: &Storage, sidecar: &Path) -> Result<PullReport> {
    // Unlike a library scan, this is one file the user pointed at deliberately.
    // Degrading to a warning would leave them with a success message and no
    // book, so an unreadable or unparsable sidecar is an error here.
    let src = std::fs::read_to_string(sidecar)?;
    let sc = parse_sidecar(&src)?;

    let mut warnings = Vec::new();
    let (book_id, matched_by) = match sc.partial_md5.as_deref() {
        Some(md5) => match storage.find_book_by_partial_md5(md5).await? {
            Some(book) => {
                let id = book
                    .id
                    .ok_or_else(|| EngineError::Other("linked book has no id".into()))?;
                (id, MatchMethod::Md5)
            }
            None => {
                let id = storage
                    .upsert_book(&book_from_sidecar(&sc), Some(Source::KOReader))
                    .await?;
                storage.link_device_book(md5, id, LinkedBy::Auto).await?;
                (id, MatchMethod::New)
            }
        },
        None => {
            // Nothing to key a mapping on, so this pull cannot be made
            // idempotent. Import anyway — refusing a file the user chose is a
            // dead end — but say so: the duplicate would otherwise appear
            // silently, on some later pull, with no way to connect it back.
            tracing::warn!(
                path = %sidecar.display(),
                "sidecar has no partial_md5_checksum; the pulled book cannot be de-duplicated"
            );
            warnings.push(Diagnostic::sidecar_not_identified(sidecar.to_path_buf()));
            (
                storage
                    .upsert_book(&book_from_sidecar(&sc), Some(Source::KOReader))
                    .await?,
                MatchMethod::New,
            )
        }
    };

    let book_title = storage
        .get_book(book_id)
        .await?
        .map(|b| b.display_title().to_string())
        .unwrap_or_default();
    let target = ImportTarget {
        book_id,
        book_title,
        matched_by,
    };
    let stats = import_into(storage, target, &sc, sidecar, false, &mut warnings).await?;
    Ok(PullReport { stats, warnings })
}

/// The book a sidecar describes, as far as the sidecar knows.
///
/// `stats` first, then `doc_props`: the two agree on current devices, but
/// `stats` is the block the statistics plugin wrote and carries `pages`, which
/// `doc_props` has no equivalent for.
fn book_from_sidecar(sc: &KoSidecar) -> Book {
    let stats = sc.stats.as_ref();
    let pick = |from_stats: Option<&String>, from_props: Option<&String>| {
        from_stats
            .or(from_props)
            .map(|s| s.as_str())
            .and_then(meaningful)
            .map(str::to_string)
    };
    Book {
        title: pick(stats.and_then(|s| s.title.as_ref()), sc.title.as_ref()),
        authors: split_authors(
            stats
                .and_then(|s| s.authors.as_deref())
                .or(sc.authors.as_deref()),
        ),
        language: pick(
            stats.and_then(|s| s.language.as_ref()),
            sc.language.as_ref(),
        ),
        page_count: stats.and_then(|s| s.pages).or(sc.doc_pages),
        ..Default::default()
    }
}

/// KOReader writes the literal string `N/A` where it has no value — seen on
/// `stats.series`, and nothing stops it appearing elsewhere. A book titled
/// "N/A" is worse than a book with no title.
fn meaningful(s: &str) -> Option<&str> {
    let s = s.trim();
    (!s.is_empty() && !s.eq_ignore_ascii_case("n/a")).then_some(s)
}

/// KOReader joins multiple authors with a newline, the same way it joins
/// `doc_props.identifiers`.
fn split_authors(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| {
        s.lines()
            .filter_map(meaningful)
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// Record a sidecar↔book decision so it is never re-guessed.
///
/// Linked as `Manual`, and it repoints: this is the user resolving an ambiguity
/// the matcher could not, which includes correcting a link the matcher got
/// wrong. A library scan must never do either — see
/// [`Storage::set_device_link`].
pub async fn link_sidecar(storage: &Storage, partial_md5: &str, book_id: i64) -> Result<()> {
    // Checked rather than left to the foreign key so the caller gets "no such
    // book" instead of a raw constraint violation.
    if storage.get_book(book_id).await?.is_none() {
        return Err(EngineError::NotFound(format!("book id {book_id}")));
    }
    storage
        .set_device_link(partial_md5, book_id, LinkedBy::Manual)
        .await
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
        // A reflowable sidecar has no unstorable anchors, and saying so here is
        // the cheapest possible guard on item 36 being a pure addition.
        assert_eq!(sc.unsupported_anchors, 0);
    }

    /// On a paging document KOReader writes `pos0` as a table — a page plus
    /// coordinates — because a scanned page has no text stream to point into.
    /// `get_str` returned `None` on it, `?` returned `None`, and the entry
    /// vanished with no count, no diagnostic and nothing in the report.
    ///
    /// The keys inside the tables here are reconstructed rather than observed
    /// (docs/koreader-format.md §6). Nothing reads them, and this test is
    /// written so that it would still pass if a real device used different ones:
    /// *table-ness* is the whole rule.
    #[test]
    fn a_table_shaped_pos0_is_counted_rather_than_dropped_in_silence() {
        const PDF: &str = r#"
return {
    ["annotations"] = {
        [1] = {
            ["datetime"] = "2026-05-02 11:15:00",
            ["pageno"] = 3,
            ["pos0"] = { ["page"] = 3, ["x"] = 96.5, ["y"] = 220.0 },
            ["pos1"] = { ["page"] = 3, ["x"] = 402.0, ["y"] = 236.5 },
            ["text"] = "a passage on a scanned page",
        },
        [2] = {
            ["datetime"] = "2026-05-02 11:22:41",
            ["note"] = "a note goes with it",
            ["pageno"] = 4,
            ["pos0"] = { ["page"] = 4, ["x"] = 72.0, ["y"] = 118.25 },
            ["text"] = "a second passage",
        },
        [3] = {
            -- a plain bookmark: no pos0 at all, and not a lost highlight
            ["datetime"] = "2026-05-03 09:40:00",
            ["pageno"] = 10,
            ["text"] = "in III",
        },
        [4] = {
            -- no text: it would not have imported whatever its anchor was, so
            -- counting it would overstate what the anchor cost
            ["datetime"] = "2026-05-03 09:41:00",
            ["pos0"] = { ["page"] = 11, ["x"] = 1.0, ["y"] = 2.0 },
        },
    },
    ["doc_props"] = { ["title"] = "A Scanned Monograph" },
}
"#;
        let sc = parse_sidecar(PDF).unwrap();
        assert!(sc.highlights.is_empty(), "nothing here can be anchored");
        assert_eq!(
            sc.unsupported_anchors, 2,
            "the bookmark and the text-less entry are not lost highlights"
        );
    }

    /// The rule lives in `entry_to_highlight`, which both layouts share, so a
    /// pre-2024 PDF sidecar degrades identically without a branch of its own.
    /// Doubly unobserved, and therefore exactly the case a hand-written branch
    /// would have missed.
    #[test]
    fn a_legacy_paging_sidecar_counts_the_same_way() {
        const LEGACY_PDF: &str = r#"
return {
    ["highlight"] = {
        [7] = {
            [1] = {
                ["datetime"] = "2021-05-02 22:30:00",
                ["text"] = "an older passage on a scanned page",
                ["pos0"] = { ["page"] = 7, ["x"] = 10.0, ["y"] = 20.0 },
            },
        },
    },
    ["doc_props"] = { ["title"] = "An Old Scan" },
}
"#;
        let sc = parse_sidecar(LEGACY_PDF).unwrap();
        assert!(sc.highlights.is_empty());
        assert_eq!(sc.unsupported_anchors, 1);
    }

    /// The change is scoped to **one Lua value type**, and this is what pins it.
    ///
    /// `get_str` coerces a Lua number to its digits, so a numeric `pos0` has
    /// always imported as the string `"7"`. That shape is unobserved on any
    /// device and there is no evidence for what it would mean — so it keeps the
    /// behaviour it had rather than acquiring a new one on a guess, and the
    /// empty string keeps being filtered exactly as before. Only a table moved.
    #[test]
    fn only_a_table_anchor_changed_behaviour() {
        const ODD: &str = r#"
return {
    ["annotations"] = {
        [1] = { ["text"] = "numeric pos0", ["pos0"] = 7, ["pageno"] = 7 },
        [2] = { ["text"] = "empty pos0",   ["pos0"] = "", ["pageno"] = 8 },
    },
    ["doc_props"] = { ["title"] = "An Odd Book" },
}
"#;
        let sc = parse_sidecar(ODD).unwrap();
        assert_eq!(sc.unsupported_anchors, 0, "neither of these is a table");
        assert_eq!(sc.highlights.len(), 1, "the empty pos0 is still no anchor");
        assert_eq!(sc.highlights[0].pos0.as_deref(), Some("7"));
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

    // ---- device state -----------------------------------------------------

    /// The shape a 2024.11+ device actually writes, taken from a real export:
    /// per-entry `datetime_updated`, a `summary` with no `note`, and a `stats`
    /// with neither `md5` nor `total_time_in_sec`.
    const DEVICE_STATE: &str = r#"
return {
    ["annotations"] = {
        [1] = {
            ["chapter"] = "Chapter 2",
            ["color"] = "gray",
            ["datetime"] = "2026-07-04 15:34:12",
            ["datetime_updated"] = "2026-07-04 15:34:23",
            ["drawer"] = "lighten",
            ["note"] = "typed 11 seconds after the highlight was made",
            ["page"] = "/body/DocFragment[15]/body/p[66]/text().897",
            ["pageno"] = 68,
            ["pos0"] = "/body/DocFragment[15]/body/p[66]/text().897",
            ["pos1"] = "/body/DocFragment[15]/body/p[66]/text().1149",
            ["text"] = "a passage",
        },
    },
    ["doc_pages"] = 2177,
    ["doc_props"] = { ["title"] = "1Q84", ["authors"] = "Haruki Murakami" },
    ["partial_md5_checksum"] = "a5b01da92a68bbbb6d88c12483cf3b56",
    ["percent_finished"] = 0.99770326136886,
    ["stats"] = {
        ["authors"] = "Haruki Murakami",
        ["highlights"] = 45,
        ["language"] = "en",
        ["notes"] = 38,
        ["pages"] = 2177,
        ["performance_in_pages"] = {},
        ["series"] = "N/A",
        ["title"] = "1Q84",
    },
    ["summary"] = {
        ["modified"] = "2026-07-22",
        ["rating"] = 5,
        ["status"] = "complete",
    },
}
"#;

    #[test]
    fn parses_the_devices_own_reading_state() {
        let sc = parse_sidecar(DEVICE_STATE).unwrap();

        assert_eq!(sc.percent_finished, Some(0.99770326136886));

        let summary = sc.summary.expect("summary");
        assert_eq!(summary.status, Some(KoStatus::Complete));
        assert_eq!(summary.rating, Some(5));
        assert_eq!(summary.modified.as_deref(), Some("2026-07-22"));
        // The user's review. Real in KOReader's source, written by no build we
        // have seen — see docs/koreader-format.md §2.2.
        assert_eq!(summary.note, None);

        let stats = sc.stats.expect("stats");
        assert_eq!(stats.title.as_deref(), Some("1Q84"));
        assert_eq!(stats.pages, Some(2177));
        assert_eq!(stats.highlights, Some(45));
        assert_eq!(stats.notes, Some(38));
        // The two fields the spec assumed were here and are not. The root
        // `partial_md5_checksum` is the book identifier, not `stats.md5`.
        assert_eq!(stats.md5, None);
        assert_eq!(stats.total_time_in_sec, None);
        assert_eq!(
            sc.partial_md5.as_deref(),
            Some("a5b01da92a68bbbb6d88c12483cf3b56")
        );
    }

    /// The field item 2 needs to tell "the device changed this" from "nothing
    /// happened", and the field that must never reach the identity hash.
    #[test]
    fn parses_the_edit_timestamp_separately_from_the_creation_one() {
        let sc = parse_sidecar(DEVICE_STATE).unwrap();
        let h = &sc.highlights[0];
        assert_eq!(h.ko_datetime.as_deref(), Some("2026-07-04 15:34:12"));
        assert_eq!(
            h.ko_datetime_updated.as_deref(),
            Some("2026-07-04 15:34:23")
        );
    }

    /// A completed book is serialised as the bare integer `1`. A parser that
    /// demands a Lua float reads `None` and silently loses the progress.
    #[test]
    fn a_completed_book_writes_percent_finished_as_an_integer() {
        let sc = parse_sidecar(r#"return { ["percent_finished"] = 1 }"#).unwrap();
        assert_eq!(sc.percent_finished, Some(1.0));
    }

    /// A future KOReader adding a status must not cost anybody their
    /// highlights. It degrades to `Other` and round-trips unchanged.
    #[test]
    fn an_unknown_status_degrades_rather_than_failing() {
        let sc = parse_sidecar(r#"return { ["summary"] = { ["status"] = "tbr" } }"#)
            .expect("an unknown status must not fail the parse");
        let status = sc.summary.unwrap().status.unwrap();
        assert_eq!(status, KoStatus::Other("tbr".into()));
        assert!(status.is_unknown());
        assert_eq!(status.to_string(), "tbr");
        assert_eq!(KoStatus::from(status.to_string().as_str()), status);
    }

    #[test]
    fn every_known_status_round_trips() {
        for s in ["reading", "abandoned", "complete"] {
            let parsed = KoStatus::from(s);
            assert!(!parsed.is_unknown(), "{s} should be a known status");
            assert_eq!(parsed.to_string(), s);
        }
    }

    /// `summary`, `stats` and `percent_finished` are DocSettings *root* keys,
    /// written by subsystems that never look at the annotations layout — so the
    /// legacy branch must still pick them up. Source-derived: no legacy sidecar
    /// exists in the real corpus.
    #[test]
    fn legacy_sidecars_still_carry_device_state() {
        let src = r#"
return {
    ["highlight"] = {
        [7] = { [1] = { ["datetime"] = "2021-05-02 22:30:00", ["text"] = "old",
                        ["pos0"] = "/body/p[7]/text().0" } },
    },
    ["doc_props"] = { ["title"] = "An Old Book" },
    ["percent_finished"] = 0.5,
    ["summary"] = { ["status"] = "abandoned", ["rating"] = 3 },
    ["stats"] = { ["md5"] = "deadbeef", ["total_time_in_sec"] = 7200 },
}
"#;
        let sc = parse_sidecar(src).unwrap();
        assert_eq!(sc.highlights.len(), 1, "legacy branch must still have run");
        assert_eq!(sc.percent_finished, Some(0.5));
        let summary = sc.summary.expect("summary");
        assert_eq!(summary.status, Some(KoStatus::Abandoned));
        assert_eq!(summary.rating, Some(3));
        // A legacy file is the likeliest place these two still appear.
        let stats = sc.stats.expect("stats");
        assert_eq!(stats.md5.as_deref(), Some("deadbeef"));
        assert_eq!(stats.total_time_in_sec, Some(7200));
    }

    /// The recorded link must beat the fuzzy title guess, and it must be
    /// *recorded* by an ordinary import — otherwise the branch is unreachable
    /// until item 3 lands and nothing here is real.
    #[tokio::test]
    async fn a_recorded_md5_link_wins_over_a_fuzzy_title_match() {
        use crate::book::Book;

        let tmp = tempfile::tempdir().unwrap();
        let sdr = tmp.path().join("1Q84.sdr");
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(sdr.join("metadata.epub.lua"), DEVICE_STATE).unwrap();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book_id = s
            .upsert_book(
                &Book {
                    title: Some("1Q84".into()),
                    authors: vec!["Haruki Murakami".into()],
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        // First pass: nothing recorded yet, so the title guess is all we have.
        let first = import(&s, tmp.path(), false).await.unwrap();
        assert_eq!(first.imported[0].matched_by, MatchMethod::Title);

        // Rename the book out from under the sidecar. The fuzzy match can no
        // longer find it; only the recorded link can.
        sqlx::query("UPDATE books SET title = ? WHERE id = ?")
            .bind("Something Else Entirely")
            .bind(book_id)
            .execute(s.pool())
            .await
            .unwrap();

        let second = import(&s, tmp.path(), false).await.unwrap();
        assert_eq!(second.imported.len(), 1, "the link must survive a rename");
        assert_eq!(second.imported[0].matched_by, MatchMethod::Md5);
        assert_eq!(second.imported[0].book_id, book_id);
        assert_eq!(second.imported[0].inserted, 0, "still idempotent");
    }

    /// A dry run reports; it must not write. Recording the link is a write.
    #[tokio::test]
    async fn a_dry_run_records_no_device_link() {
        use crate::book::Book;

        let tmp = tempfile::tempdir().unwrap();
        let sdr = tmp.path().join("1Q84.sdr");
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(sdr.join("metadata.epub.lua"), DEVICE_STATE).unwrap();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        s.upsert_book(
            &Book {
                title: Some("1Q84".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();

        import(&s, tmp.path(), true).await.unwrap();

        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM device_books")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    // ---- the device's reading state, persisted ---------------------------

    /// Import `src` into a fresh library holding one book called `title`.
    async fn import_one(src: &str, title: &str, dry_run: bool) -> (Storage, i64) {
        use crate::book::Book;

        let tmp = tempfile::tempdir().unwrap();
        let sdr = tmp.path().join(format!("{title}.sdr"));
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(sdr.join("metadata.epub.lua"), src).unwrap();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some(title.into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        import(&s, tmp.path(), dry_run).await.unwrap();
        (s, id)
    }

    /// A sidecar carrying `summary` and `percent_finished`, with one annotation
    /// at `when` so there is something to attribute and something to start the
    /// reading from.
    fn sidecar_with_status(status: &str, when: &str) -> String {
        format!(
            r#"
return {{
    ["annotations"] = {{
        [1] = {{
            ["datetime"] = "{when}",
            ["pos0"] = "/body/DocFragment[1]/body/p[1]/text().0",
            ["pos1"] = "/body/DocFragment[1]/body/p[1]/text().9",
            ["text"] = "a passage",
        }},
    }},
    ["doc_props"] = {{ ["title"] = "1Q84" }},
    ["percent_finished"] = 0.5,
    ["summary"] = {{ ["status"] = "{status}", ["rating"] = 4 }},
}}
"#
        )
    }

    /// The whole of what item 4 added to the import, in one pass: a reading is
    /// opened, the device's own state is mirrored onto it, and the highlights
    /// land inside its window.
    #[tokio::test]
    async fn an_import_opens_a_reading_and_mirrors_the_device() {
        let (s, id) = import_one(DEVICE_STATE, "1Q84", false).await;

        let readings = s.list_readings(id).await.unwrap();
        assert_eq!(readings.len(), 1);
        let r = &readings[0];
        assert_eq!(r.source, "koreader");
        assert_eq!(r.ko_status.as_deref(), Some("complete"));
        assert_eq!(r.ko_percent, Some(0.99770326136886));
        assert_eq!(r.ko_rating, Some(5));
        // KOReader never records when a book was *opened*, so the earliest
        // annotation is the earliest moment we can prove the user was in it.
        assert_eq!(
            r.started_at,
            crate::storage::ko_datetime_to_unix("2026-07-04 15:34:12")
        );
        // `complete` closes it.
        assert!(r.finished_at.is_some());
        assert_eq!(r.status, crate::storage::STATUS_FINISHED);

        let attributed: Option<i64> =
            sqlx::query_scalar("SELECT reading_id FROM highlights WHERE book_id = ?")
                .bind(id)
                .fetch_one(s.pool())
                .await
                .unwrap();
        assert_eq!(attributed, Some(r.id));

        // And the projection the render layer reads.
        let b = s.get_book(id).await.unwrap().unwrap();
        assert!(b.finished);
    }

    /// Each status maps as the spec says, and — the part worth pinning — an
    /// unrecognised one leaves *ours* alone while still mirroring theirs. The
    /// `UnknownDeviceStatus` diagnostic is what tells the user about it; a
    /// status we invented from `tbr` would be a guess with no way back.
    #[tokio::test]
    async fn every_device_status_maps_as_specified() {
        for (device, ours, closed) in [
            ("reading", crate::storage::STATUS_READING, false),
            ("abandoned", crate::storage::STATUS_ABANDONED, false),
            ("complete", crate::storage::STATUS_FINISHED, true),
            ("tbr", crate::storage::STATUS_READING, false),
        ] {
            let src = sidecar_with_status(device, "2026-01-05 21:14:08");
            let (s, id) = import_one(&src, "1Q84", false).await;
            let r = &s.list_readings(id).await.unwrap()[0];
            assert_eq!(r.status, ours, "device status {device}");
            assert_eq!(
                r.finished_at.is_some(),
                closed,
                "device status {device} closed the reading wrongly"
            );
            // Theirs is mirrored verbatim either way — that is the whole point
            // of a device-owned column.
            assert_eq!(r.ko_status.as_deref(), Some(device));
            assert_eq!(r.ko_rating, Some(4));
        }
    }

    /// An abandoned reading stays open. Closing it would make picking the book
    /// up again a *reread* rather than the continuation it is.
    #[tokio::test]
    async fn abandoned_marks_the_reading_without_closing_it() {
        let src = sidecar_with_status("abandoned", "2026-01-05 21:14:08");
        let (s, id) = import_one(&src, "1Q84", false).await;
        assert!(s.active_reading(id).await.unwrap().is_some());
    }

    /// A sidecar that says nothing about reading state opens nothing. Opening a
    /// reading on the strength of a highlight alone would claim the user read a
    /// book their device never said they had.
    #[tokio::test]
    async fn a_sidecar_with_no_device_state_opens_no_reading() {
        let (s, id) = import_one(MODERN, "Pachinko", false).await;
        assert!(s.list_readings(id).await.unwrap().is_empty());
        let unattributed: Option<i64> =
            sqlx::query_scalar("SELECT reading_id FROM highlights WHERE book_id = ? LIMIT 1")
                .bind(id)
                .fetch_one(s.pool())
                .await
                .unwrap();
        assert_eq!(unattributed, None, "unattributed is correct, not a gap");
    }

    /// A dry run reports; it must not write. Reading state is a write.
    #[tokio::test]
    async fn a_dry_run_persists_no_reading_state() {
        let (s, id) = import_one(DEVICE_STATE, "1Q84", true).await;
        assert!(s.list_readings(id).await.unwrap().is_empty());
    }

    /// Re-importing must not add a reading. `complete` is the case that gets
    /// this wrong: it *closes* the reading it just opened, so a rule of "open
    /// one when none is open" grows the history by one on every single import —
    /// silently, and worst for the books the user actually finished.
    #[tokio::test]
    async fn re_importing_never_adds_a_reading() {
        use crate::book::Book;

        for status in ["reading", "abandoned", "complete"] {
            let tmp = tempfile::tempdir().unwrap();
            let sdr = tmp.path().join("1Q84.sdr");
            std::fs::create_dir_all(&sdr).unwrap();
            let src = sidecar_with_status(status, "2026-01-05 21:14:08");
            std::fs::write(sdr.join("metadata.epub.lua"), &src).unwrap();

            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let id = s
                .upsert_book(
                    &Book {
                        title: Some("1Q84".into()),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .unwrap();

            import(&s, tmp.path(), false).await.unwrap();
            let first = s.list_readings(id).await.unwrap();
            assert_eq!(first.len(), 1, "device status {status}");
            import(&s, tmp.path(), false).await.unwrap();
            import(&s, tmp.path(), false).await.unwrap();
            assert_eq!(
                s.list_readings(id).await.unwrap(),
                first,
                "device status {status} grew the reading history"
            );
        }
    }

    /// The user reread it and said so. The device cannot know that — its sidecar
    /// is per-file and a reread appends to the same one — so the import must
    /// write to the reading that is open, not resurrect the closed one.
    #[tokio::test]
    async fn an_import_after_a_reread_writes_to_the_open_reading() {
        use crate::book::Book;

        let tmp = tempfile::tempdir().unwrap();
        let sdr = tmp.path().join("1Q84.sdr");
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(
            sdr.join("metadata.epub.lua"),
            sidecar_with_status("complete", "2026-01-05 21:14:08"),
        )
        .unwrap();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some("1Q84".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        import(&s, tmp.path(), false).await.unwrap();

        let second = s.reread(id).await.unwrap();
        import(&s, tmp.path(), false).await.unwrap();

        let readings = s.list_readings(id).await.unwrap();
        assert_eq!(readings.len(), 2, "the reread is the user's, not ours");
        let touched = readings.iter().find(|r| r.id == second).expect("reread");
        assert_eq!(touched.ko_status.as_deref(), Some("complete"));
        assert!(touched.finished_at.is_some(), "complete closes it again");
    }

    /// Every flush writes a `metadata.*.lua.old` beside the live file — 9 of
    /// the 10 real `.sdr` dirs have one. Importing both would resurrect
    /// highlights the user deleted, so the walker must not see them. It does
    /// not, because `.lua.old` is not `.lua` — incidental, and now guarded.
    #[test]
    fn the_walker_ignores_the_old_backup_koreader_leaves_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let sdr = tmp.path().join("1Q84.sdr");
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(sdr.join("metadata.epub.lua"), DEVICE_STATE).unwrap();
        std::fs::write(sdr.join("metadata.epub.lua.old"), DEVICE_STATE).unwrap();

        let found = find_sidecars(tmp.path()).unwrap();
        assert_eq!(found.len(), 1, "the .old backup must not be imported");
        assert!(found[0].ends_with("metadata.epub.lua"));
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
        s.upsert_book(
            &Book {
                title: Some("Pachinko".into()),
                authors: vec!["Min Jin Lee".into()],
                ..Default::default()
            },
            None,
        )
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

    // ---- pull from device --------------------------------------------------

    /// `<dir>/<name>.sdr/metadata.epub.lua`, returning the sidecar's path.
    fn sdr(dir: &Path, name: &str, src: &str) -> PathBuf {
        let d = dir.join(format!("{name}.sdr"));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("metadata.epub.lua");
        std::fs::write(&f, src).unwrap();
        f
    }

    /// The dead end this item exists to remove: before, a sidecar whose book was
    /// not already in the library was reported and dropped.
    #[tokio::test]
    async fn pulling_an_unmatched_sidecar_creates_the_book_from_its_own_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let path = sdr(tmp.path(), "1Q84", DEVICE_STATE);
        let s = Storage::connect("sqlite::memory:").await.unwrap();

        let report = import_book_from_sidecar(&s, &path).await.unwrap();
        assert!(report.warnings.is_empty());
        assert_eq!(report.stats.matched_by, MatchMethod::New);
        assert_eq!(report.stats.inserted, 1);

        let book = s.get_book(report.stats.book_id).await.unwrap().unwrap();
        assert_eq!(book.title.as_deref(), Some("1Q84"));
        assert_eq!(book.authors, ["Haruki Murakami"]);
        assert_eq!(book.language.as_deref(), Some("en"));
        assert_eq!(book.page_count, Some(2177));
        // Offline by decision: no provider is consulted, so there is nothing an
        // ISBN or a cover could have come from.
        assert_eq!(book.isbn_13, None);
        assert_eq!(book.cover_path, None);

        assert_eq!(s.list_highlights(book.id.unwrap()).await.unwrap().len(), 1);
        assert_eq!(
            s.device_links_for_book(book.id.unwrap()).await.unwrap(),
            ["a5b01da92a68bbbb6d88c12483cf3b56"],
            "the mapping is what makes a second pull a no-op"
        );
    }

    /// `upsert_book` cannot supply this: it branches isbn_10 → isbn_13 → plain
    /// unconditional insert, and a sidecar-seeded book has neither ISBN, so it
    /// takes the third branch every single time. The guard has to be
    /// `device_books`.
    #[tokio::test]
    async fn pulling_the_same_sidecar_twice_makes_one_book_not_two() {
        let tmp = tempfile::tempdir().unwrap();
        let path = sdr(tmp.path(), "1Q84", DEVICE_STATE);
        let s = Storage::connect("sqlite::memory:").await.unwrap();

        let first = import_book_from_sidecar(&s, &path).await.unwrap();
        let second = import_book_from_sidecar(&s, &path).await.unwrap();

        assert_eq!(second.stats.book_id, first.stats.book_id);
        assert_eq!(second.stats.matched_by, MatchMethod::Md5);
        assert_eq!(second.stats.inserted, 0);
        let books: i64 = sqlx::query_scalar("SELECT count(*) FROM books")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(books, 1);
    }

    /// No `partial_md5_checksum` means no mapping key, so this one pull cannot
    /// be made idempotent. Import anyway — refusing a file the user pointed at
    /// is a dead end — but say so in a typed diagnostic, because otherwise the
    /// duplicate turns up much later with nothing connecting it to this moment.
    #[tokio::test]
    async fn a_sidecar_with_no_md5_still_imports_and_says_it_cannot_be_deduped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = sdr(tmp.path(), "Pachinko", MODERN_WITHOUT_MD5);
        let s = Storage::connect("sqlite::memory:").await.unwrap();

        let report = import_book_from_sidecar(&s, &path).await.unwrap();
        assert_eq!(report.stats.inserted, 2, "the highlights still land");
        assert!(
            report.warnings.iter().any(|d| matches!(
                d.kind,
                crate::diagnostic::DiagnosticKind::SidecarNotIdentified { .. }
            )),
            "expected the not-identified diagnostic, got {:?}",
            report.warnings
        );

        // And the consequence the diagnostic warns about is real, not
        // hypothetical: pull again and you get a second book.
        import_book_from_sidecar(&s, &path).await.unwrap();
        let books: i64 = sqlx::query_scalar("SELECT count(*) FROM books")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(books, 2);
    }

    /// The band, from both edges. Above `AUTO_MATCH` the caller already
    /// matched, below `CANDIDATE_MIN` it is noise; what is left is the variant
    /// title that used to become a duplicate in silence.
    #[tokio::test]
    async fn match_candidates_returns_the_near_miss_and_nothing_else() {
        use crate::book::Book;

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        for title in [
            // Above AUTO_MATCH: the caller has matched on this already, so
            // offering it would be offering to link what is linked.
            "Pachinko",
            // The near miss — a subtitle the library carries and the device
            // does not. This is the shape that used to become a duplicate.
            "Pachinko: A Novel of Korea and Japan",
            // Noise, far below CANDIDATE_MIN.
            "The Brothers Karamazov",
        ] {
            s.upsert_book(
                &Book {
                    title: Some(title.into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        }

        let sc = parse_sidecar(MODERN).unwrap();
        let got = match_candidates(&s, &sc).await.unwrap();
        let titles: Vec<&str> = got.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Pachinko: A Novel of Korea and Japan"]);
        assert!(got[0].score >= CANDIDATE_MIN && got[0].score < AUTO_MATCH);
    }

    /// A recorded link is a decision, so `match_book` must honour it and the
    /// pull must not create a second book behind it.
    #[tokio::test]
    async fn linking_a_sidecar_makes_the_next_pull_find_the_book_we_chose() {
        use crate::book::Book;

        let tmp = tempfile::tempdir().unwrap();
        let path = sdr(tmp.path(), "1Q84", DEVICE_STATE);
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book_id = s
            .upsert_book(
                &Book {
                    // Nothing like the sidecar's title: only the recorded link can
                    // connect the two.
                    title: Some("Nineteen Eighty-Four Times Two".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        link_sidecar(&s, "a5b01da92a68bbbb6d88c12483cf3b56", book_id)
            .await
            .unwrap();

        let report = import_book_from_sidecar(&s, &path).await.unwrap();
        assert_eq!(report.stats.book_id, book_id);
        assert_eq!(report.stats.matched_by, MatchMethod::Md5);
        let books: i64 = sqlx::query_scalar("SELECT count(*) FROM books")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(books, 1, "the link must prevent the duplicate");

        // And an ordinary library import finds it the same way.
        let bulk = import(&s, tmp.path(), false).await.unwrap();
        assert_eq!(bulk.imported[0].matched_by, MatchMethod::Md5);
        assert!(bulk.unmatched.is_empty());
    }

    #[tokio::test]
    async fn linking_to_a_book_that_does_not_exist_is_not_found_not_a_constraint_error() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        assert!(matches!(
            link_sidecar(&s, "abc", 9999).await,
            Err(EngineError::NotFound(_))
        ));
    }

    /// Unmatched has to carry enough to act on, or the caller is back to
    /// re-parsing the file it was just handed.
    #[tokio::test]
    async fn an_unmatched_sidecar_reports_its_candidates_and_its_device_key() {
        use crate::book::Book;

        let tmp = tempfile::tempdir().unwrap();
        sdr(tmp.path(), "Pachinko", MODERN);
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        s.upsert_book(
            &Book {
                title: Some("Pachinko: A Novel of Korea and Japan".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();

        let report = import(&s, tmp.path(), false).await.unwrap();
        assert_eq!(report.unmatched.len(), 1);
        let u = &report.unmatched[0];
        assert_eq!(
            u.partial_md5.as_deref(),
            Some("0d6ba6c47caf63b8b3d1a2b3c4d5e6f7")
        );
        assert_eq!(u.candidates.len(), 1);
        assert_eq!(
            u.candidates[0].title,
            "Pachinko: A Novel of Korea and Japan"
        );
    }

    /// A pull is one file the user chose. Degrading to a warning would hand
    /// them a success message and no book.
    #[tokio::test]
    async fn a_pull_of_an_unparsable_sidecar_errors_rather_than_degrading() {
        let tmp = tempfile::tempdir().unwrap();
        let path = sdr(tmp.path(), "Bad", "return 42");
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        assert!(matches!(
            import_book_from_sidecar(&s, &path).await,
            Err(EngineError::Sidecar(_))
        ));
    }

    /// KOReader writes the literal `N/A` where it has no value, and joins
    /// multiple authors with a newline. A book titled "N/A" would be worse than
    /// a book with no title at all.
    #[test]
    fn sidecar_metadata_is_cleaned_before_it_becomes_a_book() {
        let sc = parse_sidecar(
            "return { [\"doc_props\"] = { [\"title\"] = \"Two Authors\",
                       [\"authors\"] = \"Ann Writer\\nBo Writer\", [\"language\"] = \"N/A\" },
                       [\"doc_pages\"] = 311 }",
        )
        .unwrap();
        let book = book_from_sidecar(&sc);
        assert_eq!(book.authors, ["Ann Writer", "Bo Writer"]);
        assert_eq!(book.language, None, "\"N/A\" is not a language");
        assert_eq!(
            book.page_count,
            Some(311),
            "doc_pages is the only page count a stats-less sidecar has"
        );
    }

    /// Byte-for-byte `MODERN` minus its `partial_md5_checksum` line.
    const MODERN_WITHOUT_MD5: &str = r#"
return {
    ["annotations"] = {
        [1] = {
            ["datetime"] = "2026-01-05 21:14:08",
            ["pageno"] = 42,
            ["pos0"] = "/body/DocFragment[8]/body/p[12]/text().0",
            ["text"] = "History has failed us, but no matter.",
        },
        [2] = {
            ["datetime"] = "2026-01-06 08:02:11",
            ["pageno"] = 55,
            ["pos0"] = "/body/DocFragment[9]/body/p[4]/text().10",
            ["text"] = "pachinko",
        },
    },
    ["doc_props"] = { ["title"] = "Pachinko", ["authors"] = "Min Jin Lee" },
}
"#;

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
        s.upsert_book(
            &Book {
                title: Some("The Trial".into()),
                authors: vec!["Franz Kafka".into()],
                ..Default::default()
            },
            None,
        )
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
