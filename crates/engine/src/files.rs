//! Owned files: content-addressed storage, and the three-level dedup ladder.
//!
//! `docs/decisions.md`: **readingbuddy owns its files.** The bytes are copied
//! into `database/files/<ab>/<sha256>.<ext>` and the row that says whose they
//! are lives in `book_files` (migration `0010`). The original filename is a
//! column there, never the path.
//!
//! # sha256 here, `partial_md5` there
//!
//! [`crate::partial_md5`] samples twelve 1024-byte windows, so two files
//! identical at those windows collide however different they are elsewhere.
//! That is fine for its three jobs — dedup *candidates*, sidecar↔book matching,
//! and the `statistics.sqlite3` join — and wrong as a content address, which is
//! the job here. The module doc over there already says so; this is the caller
//! that has to honour it. Both values are computed for an imported file and
//! neither substitutes for the other.
//!
//! # The three levels are distinct, and conflating them is the usual mistake
//!
//! 1. **same bytes** → sha256. Answered by `book_files`' primary key, before
//!    anything is copied.
//! 2. **same book, different file** → many-to-one via `book_id`. epub + azw3 is
//!    two rows and one book; nothing else is needed for it, and in particular
//!    no "is this the same edition" heuristic.
//! 3. **same book, unknown file** → ISBN, then `partial_md5` through the
//!    existing `device_books` mapping, then the title+author fingerprint.
//!
//! **No second matcher.** Level 3's last rung is [`crate::matching`], through
//! [`crate::koreader::scores_for`] — the same scan a sidecar and a Goodreads
//! row get, with the same `AUTO_MATCH`/`CANDIDATE_MIN` band. An epub supplies
//! its own authors to it; a file matched by its filename stem has none, and the
//! title then decides alone. Two fuzzy matchers would be two answers to "is this
//! the book I already have", and the one that disagreed would be the one that
//! made the duplicate.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::book::Book;
use crate::error::{EngineError, Result};
use crate::koreader::MatchCandidate;
use crate::storage::{BookFile, LinkedBy, Storage};

/// 64 KiB, the usual compromise: big enough that a 400 MB pdf is not a syscall
/// per page, small enough to stay off the stack and out of the way.
const COPY_BUF: usize = 64 * 1024;

/// Distinguishes concurrent imports' temp files. Only ever appended to a name
/// that is deleted or renamed before the function returns.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Which rung of the ladder found the book.
///
/// Its own enum rather than [`crate::koreader::MatchMethod`]: that one describes
/// how a *sidecar* was matched and has no notion of file content, and
/// [`FileMatch::Sha256`] is precisely the rung a sidecar can never take. The
/// three shared rungs deliberately keep the same names, because they are the
/// same rungs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMatch {
    /// These exact bytes are already owned. Level 1.
    Sha256,
    /// The file's own metadata carried an ISBN we have.
    Isbn,
    /// KOReader's `partialMD5` is already mapped to a book — either because the
    /// device told us, or because we imported this file once before.
    Md5,
    /// The shared title+author matcher was sure enough to link without asking.
    Title,
}

impl std::fmt::Display for FileMatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileMatch::Sha256 => write!(f, "sha256"),
            FileMatch::Isbn => write!(f, "isbn"),
            FileMatch::Md5 => write!(f, "md5"),
            FileMatch::Title => write!(f, "title"),
        }
    }
}

/// What a file is, and what it appears to be a copy of. Read-only: nothing here
/// writes to the database or copies a byte.
#[derive(Debug, Clone)]
pub struct FileIdentity {
    pub path: PathBuf,
    pub sha256: String,
    /// KOReader's own file identity for the same bytes, computed so that a
    /// sidecar arriving later matches by [`crate::MatchMethod::Md5`] instead of
    /// guessing at a title.
    pub partial_md5: String,
    pub format: String,
    pub size: i64,
    /// The best title we have for it: the file's own, else the filename stem.
    pub title: Option<String>,
    /// How long the file itself says the book is, where the format can answer.
    ///
    /// PDF only today ([`crate::pdf::pdf_info`]) — `epub =2.1.4` exposes no
    /// spine-to-page mapping, and an epub has no fixed pagination to expose,
    /// which is why a KOReader sidecar carries `doc_pages` and the file does
    /// not.
    ///
    /// **`None` is not zero and must never become one.** It is the whole reason
    /// item 22 needed a new module: a zero here is a false denominator, and
    /// `Progress` deliberately has no way to represent one. See
    /// `crates/engine/src/pdf.rs`.
    pub page_count: Option<i64>,
    /// The book this file belongs to, and the rung that decided.
    pub matched: Option<(i64, FileMatch)>,
    /// Books in the ambiguous band, when nothing matched outright. Empty is the
    /// ordinary case; a non-empty list is the difference between "unmatched"
    /// and "unmatched, and here is what it probably is".
    pub candidates: Vec<MatchCandidate>,
}

/// Whether an import may create a book.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportOptions {
    /// Create a book even when the library holds a near-miss candidate.
    ///
    /// The same escape hatch, with the same name, that `ko pull` has. Without
    /// it a variant title becomes a second copy of a book already on the shelf,
    /// silently — which is the failure `match_candidates` exists to prevent.
    pub new: bool,
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    /// The bytes were copied in and attached.
    Stored,
    /// We already had these exact bytes. Nothing was copied and no row was
    /// written; `book_id` is who owns them.
    AlreadyOwned,
    /// Nothing matched and the library holds candidates, so the caller has a
    /// decision to make. **Nothing was written** — not the row, not the bytes.
    Unmatched,
}

/// One file import, reported rather than thrown.
///
/// A refusal is an outcome here and not an [`EngineError`], for the reason
/// `PullReport` gives: "unmatched, and these are the candidates" is a decision
/// the user makes, and an error is a dead end with the candidates thrown away.
#[derive(Debug, Clone)]
pub struct FileImportReport {
    pub outcome: FileOutcome,
    /// The book it landed on. `None` only for [`FileOutcome::Unmatched`].
    pub book_id: Option<i64>,
    pub matched_by: Option<FileMatch>,
    /// True when this import created the book, rather than finding it.
    pub created_book: bool,
    pub sha256: String,
    /// Where the bytes live now. Present for `Stored` and `AlreadyOwned`.
    pub stored_path: Option<PathBuf>,
    pub candidates: Vec<MatchCandidate>,
}

/// The address of a file's content. `<files_dir>/<first two hex>/<sha>.<ext>`.
///
/// The two-character shard exists because a flat directory of ten thousand
/// files is a directory every `readdir` walks; sha256 is uniform, so the first
/// byte spreads them evenly at no cost.
pub fn content_path(files_dir: &Path, sha256: &str, format: &str) -> PathBuf {
    let shard = if sha256.len() >= 2 {
        &sha256[..2]
    } else {
        "00"
    };
    files_dir.join(shard).join(format!("{sha256}.{format}"))
}

/// The stored extension for a path: its own, lowercased, or `bin`.
///
/// **Sanitized, not trusted.** This value becomes a path component, and an
/// extension is attacker-adjacent input — it comes off a filename that may have
/// arrived from a download, a device or a zip. Anything but ASCII alphanumerics
/// is dropped, which makes `..`, `/` and a NUL all collapse to `bin` rather than
/// to somewhere else on the disk.
pub fn format_of(path: &Path) -> String {
    let ext: String = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .take(16)
        .collect();
    if ext.is_empty() {
        "bin".to_string()
    } else {
        ext
    }
}

/// sha256 of a file's contents, streamed.
pub fn sha256_of(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; COPY_BUF];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// What [`store`] did with the bytes.
#[derive(Debug, Clone)]
pub struct Stored {
    pub sha256: String,
    pub size: i64,
    pub format: String,
    pub path: PathBuf,
    /// The content address was already occupied, so nothing was written. Not an
    /// error: it is what level-1 dedup looks like from the filesystem's side.
    pub already_present: bool,
}

/// Copy a file into the content store, hashing it on the way through.
///
/// **Crash-safe, and that is the whole reason this is not two lines.** The copy
/// goes to a temp name inside `files_dir` and is `rename`d into place only once
/// it is complete and flushed — an atomic operation within one filesystem. A
/// straight `copy` to the final path would leave, after a kill or a power cut, a
/// half a file at a name that claims to be the sha256 of its whole contents:
/// the store would be *silently* wrong, and every later import of that book
/// would see the address occupied and skip it.
///
/// One pass, not two: the hash is computed from the bytes being written, so the
/// name can only be a name for what was actually stored.
pub fn store(files_dir: &Path, src: &Path) -> Result<Stored> {
    let format = format_of(src);
    std::fs::create_dir_all(files_dir)?;
    // The temp file is in `files_dir` rather than `$TMPDIR` so the rename stays
    // within one filesystem — across one it is a copy, and a copy is not atomic.
    let tmp = files_dir.join(format!(
        ".incoming-{}-{}",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));

    let result = copy_and_hash(src, &tmp);
    let (sha256, size) = match result {
        Ok(v) => v,
        Err(e) => {
            // A failed import must not leave litter that looks like a file.
            std::fs::remove_file(&tmp).ok();
            return Err(e);
        }
    };

    let dest = content_path(files_dir, &sha256, &format);
    if let Some(parent) = dest.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        std::fs::remove_file(&tmp).ok();
        return Err(e.into());
    }

    // `exists()` then `rename` is a race in principle — but the loser writes
    // byte-identical content to a byte-identical name, so the race has no wrong
    // outcome. Discarding our copy is the cheaper half of it.
    let already_present = dest.exists();
    if already_present {
        std::fs::remove_file(&tmp).ok();
    } else if let Err(e) = std::fs::rename(&tmp, &dest) {
        std::fs::remove_file(&tmp).ok();
        return Err(e.into());
    }

    Ok(Stored {
        sha256,
        size,
        format,
        path: dest,
        already_present,
    })
}

fn copy_and_hash(src: &Path, tmp: &Path) -> Result<(String, i64)> {
    let mut reader = std::fs::File::open(src)?;
    let mut writer = std::fs::File::create(tmp)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; COPY_BUF];
    let mut size: i64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
        size += n as i64;
    }
    // Flush before the rename, or the rename can land while the contents have
    // not — which is the exact failure the temp file was for.
    writer.sync_all()?;
    Ok((hex(&hasher.finalize()), size))
}

/// Work out what a file is and what, if anything, it is a copy of.
///
/// Read-only: no bytes are copied and no row is written, so a frontend can show
/// the answer before asking the user to commit to it. The rungs are tried in
/// descending order of certainty, and the first one that answers wins.
pub async fn identify(storage: &Storage, path: &Path) -> Result<FileIdentity> {
    if !path.is_file() {
        return Err(EngineError::InvalidInput(format!(
            "{} is not a file",
            path.display()
        )));
    }
    let format = format_of(path);
    let sha256 = sha256_of(path)?;
    let size = std::fs::metadata(path)?.len() as i64;
    let partial_md5 = crate::partial_md5::partial_md5(path)?;

    // The file's own metadata, where the engine has a reader for the format.
    // Both readers are consulted opportunistically: a failure here is a fact
    // about this file — a corrupt epub, an encrypted pdf — and not an error to
    // propagate, because the bytes are still the user's book and still worth
    // owning.
    let info = (format == "epub")
        .then(|| crate::epub::epub_info(path).ok())
        .flatten();
    // Item 22's half. A pdf answers a *length* and sometimes a title, and never
    // an ISBN or an author list, so it fills two of the four things `info`
    // does rather than being a second `EpubInfo`.
    let pdf = (format == "pdf")
        .then(|| crate::pdf::pdf_info(path).ok())
        .flatten()
        .unwrap_or_default();
    let title = info
        .as_ref()
        .and_then(|i| i.title.clone())
        .or_else(|| pdf.title.clone())
        .or_else(|| stem_title(path));

    // ---- level 1: the same bytes ------------------------------------------
    if let Some(existing) = storage.book_file(&sha256).await? {
        return Ok(FileIdentity {
            path: path.to_path_buf(),
            sha256,
            partial_md5,
            format,
            size,
            title,
            page_count: pdf.page_count,
            matched: Some((existing.book_id, FileMatch::Sha256)),
            candidates: Vec::new(),
        });
    }

    // ---- level 3, in order: ISBN, partial_md5, then the title band ---------
    let mut matched = None;
    if let Some(isbn) = info.as_ref().and_then(|i| i.isbn.clone())
        && let Some(book) = storage.find_book_by_isbn(&isbn).await?
        && let Some(id) = book.id
    {
        matched = Some((id, FileMatch::Isbn));
    }
    if matched.is_none()
        && let Some(book) = storage.find_book_by_partial_md5(&partial_md5).await?
        && let Some(id) = book.id
    {
        matched = Some((id, FileMatch::Md5));
    }
    let mut candidates = Vec::new();
    if matched.is_none() {
        // The epub's own authors when it had any; a file matched by its
        // *filename* has none, and then the title decides alone.
        let authors = info.as_ref().map(|i| i.authors.clone()).unwrap_or_default();
        let scored = crate::koreader::scores_for(
            storage,
            &crate::matching::Query::new(title.as_deref(), &authors),
        )
        .await?;
        match scored.first() {
            Some(s) if s.can_auto => {
                if let Some(id) = s.book.id {
                    matched = Some((id, FileMatch::Title));
                }
            }
            _ => candidates = crate::koreader::band(scored),
        }
    }

    Ok(FileIdentity {
        path: path.to_path_buf(),
        sha256,
        partial_md5,
        format,
        size,
        title,
        page_count: pdf.page_count,
        matched,
        candidates,
    })
}

/// The filename without its extension, as a title of last resort.
///
/// Only ever fed to the fuzzy matcher, never stored as a book's title by this
/// module: `Long_Walk_to_Freedom_(1994)_v2.azw3` is a filename, not a name, and
/// the matcher scoring it below `CANDIDATE_MIN` is the correct outcome.
fn stem_title(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let cleaned = stem.replace(['_', '.'], " ");
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

/// Attach a file to a book that is already known: copy the bytes in, record the
/// row, and teach `device_books` this file's KOReader identity.
///
/// This is dedup level 2 as an operation — a second format of a book we have.
/// It does not match and it does not create; the caller has already decided
/// whose file this is.
pub async fn attach(
    storage: &Storage,
    files_dir: &Path,
    book_id: i64,
    path: &Path,
) -> Result<FileImportReport> {
    let id = identify(storage, path).await?;
    attach_identified(storage, files_dir, book_id, &id, false).await
}

async fn attach_identified(
    storage: &Storage,
    files_dir: &Path,
    book_id: i64,
    id: &FileIdentity,
    created_book: bool,
) -> Result<FileImportReport> {
    // Level 1 again, and deliberately *not* skipped just because a caller named
    // a book: repointing owned content at a different book on the strength of a
    // re-import is exactly what the global primary key refuses.
    if let Some(existing) = storage.book_file(&id.sha256).await? {
        return Ok(FileImportReport {
            outcome: FileOutcome::AlreadyOwned,
            book_id: Some(existing.book_id),
            matched_by: Some(FileMatch::Sha256),
            created_book: false,
            sha256: id.sha256.clone(),
            stored_path: Some(content_path(files_dir, &existing.sha256, &existing.format)),
            candidates: Vec::new(),
        });
    }

    let stored = store(files_dir, &id.path)?;
    storage
        .add_book_file(&BookFile {
            sha256: stored.sha256.clone(),
            book_id,
            format: stored.format.clone(),
            original_name: id
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned()),
            size: stored.size,
            added_at: 0,
        })
        .await?;

    // The same write `import_epub` makes, for the same reason and with the same
    // method: `link_device_book`, never `set_device_link`. Importing a file is a
    // scan-shaped act, and a scan must not relabel a link the user made by hand.
    storage
        .link_device_book(&id.partial_md5, book_id, LinkedBy::Auto)
        .await?;

    // Item 22, the fourth ownership row made concrete: we hold these bytes, so
    // what they say about their own length is ours to record and to attribute.
    //
    // Through `fill_book` — the **stored row wins** merge — rather than
    // `enrich_book`. The choice is `calibre.rs`'s rule, that the pattern follows
    // whether the record is *complete*: a page count already on the book is a
    // provider's or calibre's claim about a specific edition, and a pdf is one
    // more partial record beside it, not an authority over it. So this fills a
    // gap and never overwrites an answer — and it cannot overwrite the user's,
    // because `fill_book`'s guard holds a `user` field against every source.
    //
    // `id.page_count` is `None` far more often than not, and `None` writes
    // nothing at all. That is the point: absence is not zero.
    if let Some(pages) = id.page_count {
        storage
            .fill_book(
                book_id,
                &Book {
                    page_count: Some(pages),
                    ..Default::default()
                },
                Some(crate::storage::Source::Pdf),
            )
            .await?;
    }

    Ok(FileImportReport {
        outcome: FileOutcome::Stored,
        book_id: Some(book_id),
        matched_by: id.matched.map(|(_, m)| m),
        created_book,
        sha256: stored.sha256,
        stored_path: Some(stored.path),
        candidates: Vec::new(),
    })
}

/// Import a file into the library: identify it, attach it to the book it
/// belongs to, and create that book only when nothing plausible exists.
///
/// **Refuses to create over a candidate.** A near-miss title with `new` unset
/// comes back as [`FileOutcome::Unmatched`] carrying the candidates, having
/// written nothing — creating first and warning afterwards would reproduce the
/// silent duplicate the candidate band exists to prevent. With no candidates at
/// all there is nothing to refuse over, so the book is created; that is the same
/// shape `ko pull` has.
pub async fn import(
    engine: &crate::Engine,
    path: &Path,
    opts: ImportOptions,
) -> Result<FileImportReport> {
    let storage = &engine.storage;
    let files_dir = &engine.config.files_dir;
    let id = identify(storage, path).await?;

    if let Some((book_id, _)) = id.matched {
        return attach_identified(storage, files_dir, book_id, &id, false).await;
    }
    if !id.candidates.is_empty() && !opts.new {
        return Ok(FileImportReport {
            outcome: FileOutcome::Unmatched,
            book_id: None,
            matched_by: None,
            created_book: false,
            sha256: id.sha256,
            stored_path: None,
            candidates: id.candidates,
        });
    }

    let book_id = create_book_for(engine, &id).await?;
    attach_identified(storage, files_dir, book_id, &id, true).await
}

/// Make a book for a file nothing in the library claims.
///
/// An epub goes through [`crate::Engine::import_epub`], so it gets the metadata
/// path that already exists — its own OPF, provider enrichment when it carries
/// an ISBN, and the embedded cover — rather than a second, worse version of it
/// written here. Every other format has no metadata reader in this engine, so
/// the book is its filename and nothing more; that is honest, and an ebook whose
/// only name is `book2.azw3` is exactly the case the candidate band above is
/// meant to catch before it gets here.
async fn create_book_for(engine: &crate::Engine, id: &FileIdentity) -> Result<i64> {
    if id.format == "epub" {
        match engine.import_epub(&id.path).await {
            Ok(book) => {
                if let Some(book_id) = book.id {
                    return Ok(book_id);
                }
            }
            // A corrupt epub is not a reason to refuse the file: the bytes are
            // still the user's book and are still worth owning. Fall through to
            // the filename, and say so — silence here would look like a
            // successful metadata import that produced an empty book.
            Err(e) => tracing::warn!(
                path = %id.path.display(),
                error = %e,
                "epub metadata import failed; creating the book from the filename"
            ),
        }
    }
    let saved = engine
        .save_book(&Book {
            title: id.title.clone(),
            ..Default::default()
        })
        .await?;
    saved
        .id
        .ok_or_else(|| EngineError::Other("a stored book has an id".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// The one value in this module that another implementation can check us
    /// against: `sha256("")` and `sha256("abc")` are published constants.
    #[test]
    fn the_hash_is_the_sha256_everyone_elses_tool_prints() {
        let tmp = tmpdir();
        assert_eq!(
            sha256_of(&write(tmp.path(), "empty", b"")).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_of(&write(tmp.path(), "abc", b"abc")).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// A file larger than one buffer, to prove the streaming loop is not a
    /// single-read function that happens to work on small inputs.
    #[test]
    fn hashing_streams_rather_than_reading_once() {
        let tmp = tmpdir();
        let big: Vec<u8> = (0..COPY_BUF * 3 + 17).map(|i| (i % 251) as u8).collect();
        let path = write(tmp.path(), "big.epub", &big);

        let mut expect = Sha256::new();
        expect.update(&big);
        assert_eq!(sha256_of(&path).unwrap(), hex(&expect.finalize()));

        let files = tmp.path().join("files");
        let stored = store(&files, &path).unwrap();
        assert_eq!(stored.size, big.len() as i64);
        assert_eq!(std::fs::read(&stored.path).unwrap(), big);
    }

    #[test]
    fn the_address_shards_on_the_first_byte_and_keeps_the_format() {
        let p = content_path(Path::new("/data/files"), "ab12cd", "epub");
        assert_eq!(p, Path::new("/data/files/ab/ab12cd.epub"));
    }

    /// An extension becomes a path component, so it is sanitized rather than
    /// trusted. Each of these is a filename a real import can be handed.
    #[test]
    fn a_hostile_extension_cannot_leave_the_store() {
        let cases = [
            ("book.epub", "epub"),
            ("book.EPUB", "epub"),
            ("book.AZW3", "azw3"),
            ("book", "bin"),
            ("book.", "bin"),
            (".hidden", "bin"),
            // `Path` gets there first: this is a *path* whose last component is
            // `passwd`, which has no extension at all. The filter is the second
            // line of defence, not the first.
            ("book.../etc/passwd", "bin"),
            ("archive.tar.gz", "gz"),
            ("book.e p u b", "epub"),
            // Non-ASCII is dropped rather than transliterated: this is a
            // filesystem name we generate, not the user's word for anything.
            ("book.épub", "pub"),
        ];
        for (name, want) in cases {
            let got = format_of(Path::new(name));
            assert_eq!(got, want, "{name}");
            let p = content_path(Path::new("/data/files"), "abcd", &got);
            assert!(
                p.starts_with("/data/files/ab"),
                "{name} escaped the store: {}",
                p.display()
            );
            assert_eq!(p.components().count(), 5, "{name} added path components");
        }
    }

    /// The store's own invariant: the name of a stored file is the sha256 of
    /// what is in it. Everything else — dedup, the skipped re-copy, the merge —
    /// is downstream of this being true.
    #[test]
    fn a_stored_file_is_named_for_what_is_actually_in_it() {
        let tmp = tmpdir();
        let files = tmp.path().join("files");
        let src = write(tmp.path(), "Pachinko.epub", b"not really an epub");

        let stored = store(&files, &src).unwrap();
        assert!(!stored.already_present);
        assert_eq!(stored.path, content_path(&files, &stored.sha256, "epub"));
        assert_eq!(sha256_of(&stored.path).unwrap(), stored.sha256);
    }

    /// Level 1 at the filesystem: the same bytes under a different name are the
    /// same address, written once.
    #[test]
    fn the_same_bytes_are_stored_once_however_they_are_named() {
        let tmp = tmpdir();
        let files = tmp.path().join("files");
        let a = write(tmp.path(), "Pachinko.epub", b"the same bytes");
        let b = write(tmp.path(), "pachinko (1).epub", b"the same bytes");

        let first = store(&files, &a).unwrap();
        let second = store(&files, &b).unwrap();
        assert_eq!(first.sha256, second.sha256);
        assert_eq!(first.path, second.path);
        assert!(!first.already_present);
        assert!(
            second.already_present,
            "the second copy must be recognised, not rewritten"
        );
    }

    /// Nothing but content-addressed files may survive an import — a temp file
    /// left behind is a file the store cannot name and will never collect.
    #[test]
    fn an_import_leaves_no_temp_files_behind() {
        let tmp = tmpdir();
        let files = tmp.path().join("files");
        let src = write(tmp.path(), "a.epub", b"aaa");
        store(&files, &src).unwrap();
        store(&files, &src).unwrap(); // the already-present path, which also has a temp file

        let strays: Vec<PathBuf> = std::fs::read_dir(&files)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_file())
            .collect();
        assert!(strays.is_empty(), "left behind: {strays:?}");
    }

    /// The failure the temp-and-rename dance is for, in the only form a test
    /// can stage it: a file that cannot be read at all must leave the store
    /// exactly as it found it, rather than a truncated file at a name claiming
    /// to be its hash.
    #[test]
    fn a_failed_copy_stores_nothing() {
        let tmp = tmpdir();
        let files = tmp.path().join("files");
        let err = store(&files, &tmp.path().join("does-not-exist.epub")).unwrap_err();
        assert!(matches!(err, EngineError::Io(_)), "{err:?}");

        let left: Vec<PathBuf> = std::fs::read_dir(&files)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert!(left.is_empty(), "left behind: {left:?}");
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Store-then-hash is the identity: for any bytes, the file that
            /// comes out is the file that went in, at the address its own
            /// contents name. Written as a property because the failure mode
            /// this rules out — a buffer-boundary bug in the copy loop — is
            /// about lengths, and lengths are what a generator explores.
            #[test]
            fn any_bytes_round_trip_to_their_own_address(bytes in prop::collection::vec(any::<u8>(), 0..200_000)) {
                let tmp = tempfile::tempdir().unwrap();
                let files = tmp.path().join("files");
                let src = tmp.path().join("in.epub");
                std::fs::write(&src, &bytes).unwrap();

                let stored = store(&files, &src).unwrap();
                prop_assert_eq!(stored.size, bytes.len() as i64);
                prop_assert_eq!(std::fs::read(&stored.path).unwrap(), bytes);
                prop_assert_eq!(sha256_of(&stored.path).unwrap(), stored.sha256.clone());
                prop_assert_eq!(stored.path, content_path(&files, &stored.sha256, "epub"));
            }

            /// A format is always one safe path component, whatever the
            /// filename was. The generator is deliberately allowed slashes,
            /// dots and control characters.
            #[test]
            fn a_format_is_always_one_safe_component(name in ".{0,40}") {
                let format = format_of(Path::new(&name));
                prop_assert!(!format.is_empty());
                prop_assert!(format.chars().all(|c| c.is_ascii_alphanumeric()));
                let p = content_path(Path::new("/store"), "abcdef", &format);
                prop_assert!(p.starts_with("/store/ab"));
                prop_assert_eq!(p.components().count(), 4);
            }
        }
    }
}
