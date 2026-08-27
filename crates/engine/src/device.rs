//! Reading a mounted KOReader device.
//!
//! Two verbs, and the difference between them is the whole module: [`scan_device`]
//! is read-only and answers *what is the state of each book on this device*;
//! [`sync_device`] writes, and pulls a chosen selection in.
//!
//! It lives here rather than in [`crate::koreader`] because a scan is not an
//! import. `import`'s `dry_run` already walks a tree and previews it, so a scan
//! looks nearly free — and is not, twice over. It evaluates every sidecar's Lua
//! in mlua before it can say anything, which on a device with several hundred
//! books is several hundred VM evaluations *every time the screen opens*; and
//! its answer is an `ImportReport`, which says what would import, not what state
//! each book is in. This module consumes `koreader`'s parsing and matching and
//! adds the pre-filter cache (`sidecar_seen`, migration `0006`) that makes the
//! first problem go away.

use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::error::{EngineError, Result};
use crate::koreader::{
    self, KoSidecar, KoStatus, MatchCandidate, MatchMethod, PullReport, find_sidecars,
    parse_sidecar,
};
use crate::storage::DeviceDigest;
use crate::storage::{NewHighlight, SidecarFacts, Storage};

/// Where a KOReader install sits relative to a mount root, in the order worth
/// looking.
///
/// Not just `koreader/`: only Kindle puts it at the root of the USB volume.
/// Kobo installs into `.adds/koreader`, PocketBook into `applications/koreader`.
/// A device screen that only knew the Kindle layout would report the other two
/// as "not a KOReader device" with the device plainly mounted.
const KOREADER_DIRS: [&str; 3] = ["koreader", ".adds/koreader", "applications/koreader"];

/// What has to be inside that directory before we believe it.
///
/// A bare directory named `koreader` is not a KOReader install — it is just as
/// likely a folder somebody made. These three are present in every build:
/// `reader.lua` is the entry point, `frontend/` the library, `plugins/` where a
/// plugin would go.
///
/// **This is not cosmetic.** `docs/decisions.md` requires that readingbuddy
/// refuse to write to an unrecognised volume before it installs its plugin, and
/// this predicate is the gate that will enforce it. It is settled here, while
/// everything around it is still read-only.
const KOREADER_MARKERS: [(&str, Kind); 3] = [
    ("reader.lua", Kind::File),
    ("frontend", Kind::Dir),
    ("plugins", Kind::Dir),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Dir,
}

/// Mount points to look under, per platform.
const MOUNT_ROOTS: [&str; 1] = ["/Volumes"];
/// Mount points that are per-user on Linux; `$USER` is appended.
const USER_MOUNT_ROOTS: [&str; 2] = ["/run/media", "/media"];

/// The KOReader install directory inside a mount, if this looks like one.
///
/// Returned rather than discarded because the plugin installer needs exactly
/// this path, and having two functions disagree about where KOReader lives is
/// the failure mode worth designing out now.
pub fn koreader_dir(mount: &Path) -> Option<PathBuf> {
    KOREADER_DIRS
        .iter()
        .map(|rel| mount.join(rel))
        .find(|dir| has_markers(dir))
}

fn has_markers(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    KOREADER_MARKERS.iter().all(|(name, kind)| {
        let p = dir.join(name);
        match kind {
            Kind::File => p.is_file(),
            Kind::Dir => p.is_dir(),
        }
    })
}

/// Does this volume hold a KOReader install?
pub fn is_koreader_mount(p: &Path) -> bool {
    koreader_dir(p).is_some()
}

/// The directories this platform mounts removable volumes into, as they exist
/// on this machine.
///
/// Public because [`crate::watch`] watches exactly these, and a watcher looking
/// at a different set of directories from the one [`candidate_mounts`] walks is
/// the failure mode where a device is listed but never noticed.
pub fn mount_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = MOUNT_ROOTS.iter().map(PathBuf::from).collect();
    if let Ok(user) = std::env::var("USER") {
        roots.extend(USER_MOUNT_ROOTS.iter().map(|r| Path::new(r).join(&user)));
    }
    roots.retain(|r| r.is_dir());
    roots
}

/// Would we offer this path as a mounted reader?
///
/// [`is_koreader_mount`] plus the symlink rule, in one place because the watcher
/// and the lister must agree: a volume one of them announces and the other never
/// lists is a device the user is told about and cannot open.
pub fn offers_reader(path: &Path) -> bool {
    // `/Volumes/Macintosh HD` is a symlink to `/` on macOS. Following it would
    // offer the boot disk as a removable device.
    let is_link = std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    !is_link && is_koreader_mount(path)
}

/// Mounted volumes that hold a KOReader install.
///
/// Filtered rather than listed: an unfiltered list of `/Volumes` is something
/// the caller could produce itself, and every caller would then have to
/// re-apply [`is_koreader_mount`] — including the one that is about to write to
/// the volume.
pub fn candidate_mounts() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in mount_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if offers_reader(&p) {
                found.push(p);
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

// ---- the scan ---------------------------------------------------------------

/// What one book on the device is, relative to the library.
///
/// The four states `docs/decisions.md` specifies, and no more. `Unreadable`
/// carries the existing [`Diagnostic`] vocabulary rather than a parallel error
/// taxonomy of its own.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceState {
    /// Not in the library. `candidates` is the ambiguous band — non-empty means
    /// "and here is what it probably already is", which is the difference
    /// between a decision and a dead end.
    New { candidates: Vec<MatchCandidate> },
    /// Linked, and the device has nothing we do not already hold.
    Unchanged,
    /// Linked, with annotations to bring across. `refreshed` counts rows whose
    /// device-owned fields the sidecar disagrees with — the same distinction
    /// `BookImportStats::updated` draws, for the same reason: without it a note
    /// edited on the device is indistinguishable from nothing at all.
    Updated {
        new_highlights: usize,
        refreshed: usize,
    },
    /// The file could not be read or could not be parsed. Never fatal: one bad
    /// sidecar must not cost you the view of the other four hundred.
    Unreadable(Diagnostic),
}

impl DeviceState {
    /// Is there anything for a sync to do here?
    pub fn is_syncable(&self) -> bool {
        matches!(self, DeviceState::New { .. } | DeviceState::Updated { .. })
    }

    /// One word, for a list row.
    pub fn label(&self) -> &'static str {
        match self {
            DeviceState::New { .. } => "new",
            DeviceState::Unchanged => "unchanged",
            DeviceState::Updated { .. } => "updated",
            DeviceState::Unreadable(_) => "unreadable",
        }
    }
}

/// One book as it exists on the device.
#[derive(Debug, Clone)]
pub struct DeviceBook {
    pub path: PathBuf,
    pub title: Option<String>,
    pub authors: Option<String>,
    pub partial_md5: Option<String>,
    pub book_id: Option<i64>,
    pub matched_by: Option<MatchMethod>,
    pub state: DeviceState,
    pub ko_percent: Option<f64>,
    pub ko_status: Option<KoStatus>,
}

impl DeviceBook {
    /// Something to put in a list row.
    ///
    /// Falls back to the `.sdr` directory's name, which is the ebook's filename
    /// and is the only thing an unreadable sidecar can be identified by at all —
    /// a row reading "unknown title" tells the user nothing about *which* file
    /// to go and look at.
    pub fn display_title(&self) -> String {
        if let Some(t) = &self.title {
            return t.clone();
        }
        self.path
            .parent()
            .and_then(|d| d.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown title".to_string())
    }
}

/// The result of walking a mounted device.
#[derive(Debug, Clone, Default)]
pub struct DeviceScan {
    pub root: PathBuf,
    pub books: Vec<DeviceBook>,
    pub warnings: Vec<Diagnostic>,
    /// Sidecars whose Lua was evaluated by this scan.
    ///
    /// Reported, not logged, because the pre-filter's whole claim is that this
    /// number is zero on a second scan of an unmodified tree — and a claim a
    /// test can only check by timing is not a claim at all.
    pub parsed: usize,
    /// Sidecars answered from `sidecar_seen` without touching the parser.
    pub cached: usize,
}

impl DeviceScan {
    pub fn syncable(&self) -> impl Iterator<Item = &DeviceBook> {
        self.books.iter().filter(|b| b.state.is_syncable())
    }
}

/// What one whole-device sync did (item 55).
///
/// The reply to *bring everything across*, and it carries the scan it was
/// decided from rather than only the imports: **"nothing to bring across" and
/// "there is nothing on this device" are different answers**, and a report that
/// held only `reports` would render them as the same empty list.
#[derive(Debug, Default)]
pub struct MountSync {
    pub mount: PathBuf,
    /// The paired reader this mount turned out to be, when it is one.
    ///
    /// `None` covers two ordinary cases and neither is an error: a KOReader
    /// volume we have never installed onto, and one carrying a `pairing.lua`
    /// minted by somebody else's copy of readingbuddy. The sync happens either
    /// way — importing sidecars has never needed a pairing — and only the
    /// `last_synced_at` stamp is skipped.
    pub device_id: Option<String>,
    /// Books the scan found, syncable or not.
    pub found: usize,
    /// Books that had something to bring across, and so were imported.
    pub synced: usize,
    pub reports: Vec<PullReport>,
    /// The **scan's** warnings. Each report carries its own import's.
    pub warnings: Vec<Diagnostic>,
}

/// Walk a mounted device (or any library tree) and report the state of every
/// book on it. Read-only.
///
/// Never fails on one bad file: an unreadable or unparsable sidecar becomes a
/// row in [`DeviceState::Unreadable`], not an error.
#[tracing::instrument(skip(storage), fields(root = %root.display()))]
pub async fn scan_device(storage: &Storage, root: &Path) -> Result<DeviceScan> {
    // One token for the whole scan, stamped onto every cache row it touches.
    // Nanoseconds, and compared for equality rather than order — see
    // `Storage::forget_sidecars_under`.
    let seen_at = nanos(std::time::SystemTime::now());
    let mut scan = DeviceScan {
        root: root.to_path_buf(),
        ..Default::default()
    };

    let sidecars = find_sidecars(root)?;
    if sidecars.is_empty() {
        tracing::warn!(root = %root.display(), "no KOReader sidecars found");
        scan.warnings
            .push(Diagnostic::no_sidecars_found(root.to_path_buf()));
        return Ok(scan);
    }

    let mut counts = (0usize, 0usize);
    for path in sidecars {
        scan.books
            .push(scan_one(storage, &path, seen_at, &mut counts).await?);
    }
    (scan.parsed, scan.cached) = counts;

    // A sidecar deleted from the device leaves a cache row that will never be
    // seen again. Scoped to this root, so unplugging one device does not forget
    // another.
    let forgotten = storage
        .forget_sidecars_under(&root.to_string_lossy(), seen_at)
        .await?;

    tracing::info!(
        books = scan.books.len(),
        parsed = scan.parsed,
        cached = scan.cached,
        forgotten,
        "scanned device"
    );
    Ok(scan)
}

/// The state of one sidecar. `counts` is `(parsed, cached)`.
async fn scan_one(
    storage: &Storage,
    path: &Path,
    seen_at: i64,
    counts: &mut (usize, usize),
) -> Result<DeviceBook> {
    let key = path.to_string_lossy().into_owned();

    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            // Cannot even `stat` it, so there is nothing to key a cache row on.
            let err = EngineError::from(e);
            return Ok(unreadable(
                path,
                Diagnostic::sidecar_unreadable(path.to_path_buf(), &err),
            ));
        }
    };
    let size = meta.len() as i64;
    let mtime = mtime_nanos(&meta);

    // `stat` first. A row whose size and mtime still agree with the file is
    // what the file says, so the parse is skipped entirely.
    let fresh = storage
        .sidecar_facts(&key)
        .await?
        .filter(|f| f.size == size && f.mtime == mtime);

    let mut highlights: Option<Vec<NewHighlight>> = None;
    let facts = match fresh {
        Some(f) => {
            counts.1 += 1;
            // Seen this scan, so the prune must not take it.
            storage.touch_sidecar_seen(&key, seen_at).await?;
            f
        }
        None => {
            counts.0 += 1;
            let sc = match read_sidecar(path) {
                Ok(sc) => sc,
                // A file that does not parse has nothing to cache — every
                // column of `sidecar_seen` is something the file *said*. It is
                // re-read on every scan, which is the right price for the one
                // or two of these a real device has.
                Err(diag) => return Ok(unreadable(path, diag)),
            };
            let f = facts_of(&sc, size, mtime);
            storage.record_sidecar_facts(&key, &f, seen_at).await?;
            highlights = Some(sc.highlights);
            f
        }
    };

    // The matcher reads only `partial_md5`, `title` and `authors`, all three of
    // which `sidecar_seen` keeps verbatim, so a cache hit reaches exactly the
    // verdict a fresh parse would. `authors` joined that list when the matcher
    // gained its veto: cached but unread it would have been fine, read but
    // uncached it would have made a cache hit and a fresh parse disagree.
    let probe = KoSidecar {
        title: facts.title.clone(),
        authors: facts.authors.clone(),
        partial_md5: facts.partial_md5.clone(),
        ..Default::default()
    };

    let ko_status = facts.ko_status.as_deref().map(KoStatus::from);
    let mut book = DeviceBook {
        path: path.to_path_buf(),
        title: facts.title.clone(),
        authors: facts.authors.clone(),
        partial_md5: facts.partial_md5.clone(),
        book_id: None,
        matched_by: None,
        state: DeviceState::Unchanged,
        ko_percent: facts.ko_percent,
        ko_status,
    };

    let Some((matched, matched_by)) = koreader::match_book(storage, path, &probe).await? else {
        book.state = DeviceState::New {
            candidates: koreader::match_candidates(storage, &probe).await?,
        };
        return Ok(book);
    };
    let Some(book_id) = matched.id else {
        // Storage always assigns an id; treat the impossible as unmatched
        // rather than panicking mid-scan.
        tracing::error!(title = %matched.display_title(), "matched book has no id");
        book.state = DeviceState::New {
            candidates: Vec::new(),
        };
        return Ok(book);
    };
    book.book_id = Some(book_id);
    book.matched_by = Some(matched_by);
    if book.title.is_none() {
        book.title = Some(matched.display_title().to_string());
    }

    // The cheap half. A file we did not re-read, whose annotations the library
    // already holds exactly, has nothing to bring across — and saying so costs
    // one query rather than an mlua evaluation.
    //
    // A row *count* was the first cut here, and it is wrong: a note rewritten
    // on the device leaves the count identical, so the edit was reported by the
    // one scan that happened to re-read the file and by no scan after it — the
    // cache then said "unchanged" forever about a change it had already seen.
    // The digest covers the device-owned payload, so it cannot miss that.
    if highlights.is_none() && storage.device_highlight_digest(book_id).await? == facts.entry_digest
    {
        book.state = DeviceState::Unchanged;
        return Ok(book);
    }

    // Otherwise the numbers have to be real, and only the file has them.
    let highlights = match highlights {
        Some(h) => h,
        None => {
            counts.0 += 1;
            match read_sidecar(path) {
                Ok(sc) => sc.highlights,
                Err(diag) => return Ok(unreadable(path, diag)),
            }
        }
    };

    let mut new_highlights = 0;
    let mut refreshed = 0;
    for h in &highlights {
        if !storage.highlight_exists(book_id, h).await? {
            new_highlights += 1;
        } else if storage.device_fields_differ(book_id, h).await? {
            refreshed += 1;
        }
    }
    book.state = if new_highlights == 0 && refreshed == 0 {
        DeviceState::Unchanged
    } else {
        DeviceState::Updated {
            new_highlights,
            refreshed,
        }
    };
    Ok(book)
}

/// A row for a file we could not read. Everything but the path is unknown, by
/// definition — that is what makes it unreadable.
fn unreadable(path: &Path, diagnostic: Diagnostic) -> DeviceBook {
    DeviceBook {
        path: path.to_path_buf(),
        title: None,
        authors: None,
        partial_md5: None,
        book_id: None,
        matched_by: None,
        state: DeviceState::Unreadable(diagnostic),
        ko_percent: None,
        ko_status: None,
    }
}

/// Read and parse, turning either failure into the existing diagnostic for it.
fn read_sidecar(path: &Path) -> std::result::Result<KoSidecar, Diagnostic> {
    let src = std::fs::read_to_string(path).map_err(|e| {
        let err = EngineError::from(e);
        tracing::warn!(path = %path.display(), error = %err, "sidecar unreadable");
        Diagnostic::sidecar_unreadable(path.to_path_buf(), &err)
    })?;
    parse_sidecar(&src).map_err(|e| {
        tracing::warn!(path = %path.display(), error = %e, "sidecar unparsable");
        Diagnostic::sidecar_unparsable(path.to_path_buf(), &e)
    })
}

/// The part of a parse a scan needs later. Highlight text is not in it — it is
/// the user's private reading, and `highlights` is where it already lives.
fn facts_of(sc: &KoSidecar, size: i64, mtime: i64) -> SidecarFacts {
    SidecarFacts {
        size,
        mtime,
        partial_md5: sc.partial_md5.clone(),
        entry_count: sc.highlights.len() as i64,
        entry_digest: digest_of(&sc.highlights),
        // `doc_props.title` exactly, not a prettier fallback: this value is fed
        // back to the matcher, which must reach the same verdict from a cache
        // hit as from a fresh parse.
        title: sc.title.clone(),
        authors: sc.authors.clone(),
        ko_percent: sc.percent_finished,
        ko_status: sc
            .summary
            .as_ref()
            .and_then(|s| s.status.as_ref())
            .map(|s| s.to_string()),
    }
}

/// The file's side of the scan's comparison. The library's side is
/// `Storage::device_highlight_digest`, and both go through `DeviceDigest` —
/// one formula, or the two would drift and the scan would quietly stop
/// noticing device edits.
fn digest_of(highlights: &[NewHighlight]) -> String {
    let mut digest = DeviceDigest::new();
    for h in highlights {
        digest.add(h.device_entry());
    }
    digest.finish()
}

fn mtime_nanos(meta: &std::fs::Metadata) -> i64 {
    meta.modified().ok().map(nanos).unwrap_or(0)
}

/// Nanoseconds since the epoch. A pre-1970 stamp — which a device with a dead
/// clock does produce — collapses to 0 rather than failing the scan; the worst
/// it costs is a re-parse.
fn nanos(t: std::time::SystemTime) -> i64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

// ---- the sync ---------------------------------------------------------------

/// Pull a chosen selection of the device in: one [`PullReport`] per book.
///
/// A directory expands to every sidecar under it, so a report is always about
/// exactly one book — a caller showing per-book progress needs that, and it is
/// what lets the TUI drain the queue one book per redraw.
///
/// A sidecar that fails to read or parse is an **error** here, unlike in a
/// scan. A scan is a view and degrades; a sync is something the user asked for
/// by name, and a silent success with no book is worse than a refusal.
#[tracing::instrument(skip_all, fields(paths = paths.len()))]
pub async fn sync_device(storage: &Storage, paths: &[PathBuf]) -> Result<Vec<PullReport>> {
    let mut out = Vec::new();
    for path in paths {
        for sidecar in find_sidecars(path)? {
            out.push(sync_one(storage, &sidecar).await?);
        }
    }
    Ok(out)
}

async fn sync_one(storage: &Storage, sidecar: &Path) -> Result<PullReport> {
    // Import first: it is the path that matches an existing book and records
    // the link. Creating from the sidecar unconditionally would take the
    // ISBN-less insert branch and put a second copy of an already-linked book
    // on the shelf.
    let mut report = koreader::import(storage, sidecar, false).await?;
    if let Some(stats) = report.imported.pop() {
        return Ok(PullReport {
            stats,
            warnings: report.warnings,
        });
    }

    // Unmatched. Syncing a row the scan showed as `New` *is* the decision to
    // create it — the candidate band was already offered, on the screen this
    // selection came from.
    let mut pulled = koreader::import_book_from_sidecar(storage, sidecar).await?;
    let mut warnings = report.warnings;
    warnings.append(&mut pulled.warnings);
    pulled.warnings = warnings;
    Ok(pulled)
}

/// Build a plausible KOReader install under `root`.
///
/// One definition of "this is really a reader", because that is the whole point
/// of [`is_koreader_mount`]: it gates the read path and the write path alike,
/// and two fixture builders would be two opinions about what it gates on.
/// `plugin.rs`'s unit tests and `crates/engine/tests/` both build their mounts
/// with it.
///
/// **Test-only**, behind the same `internals` feature that carries
/// `Engine::storage` and `pdf::synthetic_pdf`, for that function's own reason —
/// a fixture builder is what the feature is for, and a plain
/// `cargo check --workspace` is what keeps it out of a shipped build.
#[cfg(any(test, feature = "internals"))]
pub fn install_fake_reader(root: &Path) {
    let dir = root.join("koreader");
    std::fs::create_dir_all(dir.join("frontend")).unwrap();
    std::fs::create_dir_all(dir.join("plugins")).unwrap();
    std::fs::write(dir.join("reader.lua"), "-- entry point\n").unwrap();
}

#[cfg(test)]
pub(crate) mod tests {
    pub(crate) use super::install_fake_reader as install;
    use super::*;

    #[test]
    fn a_real_install_is_recognised_wherever_the_platform_puts_it() {
        for rel in KOREADER_DIRS {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join(rel);
            std::fs::create_dir_all(dir.join("frontend")).unwrap();
            std::fs::create_dir_all(dir.join("plugins")).unwrap();
            std::fs::write(dir.join("reader.lua"), "-- entry point\n").unwrap();

            assert!(is_koreader_mount(tmp.path()), "{rel} should be recognised");
            assert_eq!(koreader_dir(tmp.path()), Some(dir));
        }
    }

    /// The predicate that will gate writing to a volume. An ordinary directory
    /// is not a device, and neither is a folder that merely shares the name.
    #[test]
    fn an_ordinary_directory_and_a_look_alike_are_both_refused() {
        let plain = tempfile::tempdir().unwrap();
        assert!(!is_koreader_mount(plain.path()));
        assert_eq!(koreader_dir(plain.path()), None);

        // Someone's own folder called "koreader" — the look-alike.
        let fake = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(fake.path().join("koreader")).unwrap();
        std::fs::write(fake.path().join("koreader/notes.txt"), "mine").unwrap();
        assert!(!is_koreader_mount(fake.path()));

        // Two of the three markers is still not an install.
        let partial = tempfile::tempdir().unwrap();
        let dir = partial.path().join("koreader");
        std::fs::create_dir_all(dir.join("frontend")).unwrap();
        std::fs::write(dir.join("reader.lua"), "-- entry point\n").unwrap();
        assert!(
            !is_koreader_mount(partial.path()),
            "plugins/ is missing, and that is where we would write"
        );

        // `reader.lua` as a *directory* must not satisfy the file marker.
        let typed = tempfile::tempdir().unwrap();
        let dir = typed.path().join("koreader");
        std::fs::create_dir_all(dir.join("frontend")).unwrap();
        std::fs::create_dir_all(dir.join("plugins")).unwrap();
        std::fs::create_dir_all(dir.join("reader.lua")).unwrap();
        assert!(!is_koreader_mount(typed.path()));
    }

    /// `candidate_mounts` reads real system paths, so what it returns here
    /// depends on the machine. What must hold on any machine is that every
    /// entry it *does* return passes the predicate.
    #[test]
    fn every_offered_mount_is_one_we_would_accept() {
        for m in candidate_mounts() {
            assert!(is_koreader_mount(&m), "{} was offered", m.display());
        }
    }

    #[test]
    fn a_mount_is_recognised_by_its_contents_not_its_name() {
        let tmp = tempfile::tempdir().unwrap();
        install(tmp.path());
        assert!(is_koreader_mount(tmp.path()));
    }
}
