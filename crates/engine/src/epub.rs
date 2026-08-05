use std::path::{Path, PathBuf};

use epub::doc::EpubDoc;

use crate::book::normalize_isbn;
use crate::error::{EngineError, Result};

#[derive(Debug, Default)]
pub struct EpubInfo {
    pub isbn: Option<String>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
}

/// Read epub metadata. Scans ALL identifier entries for the first one that
/// validates as an ISBN (identifiers are often UUIDs or urn:isbn: forms).
pub fn epub_info(path: &Path) -> Result<EpubInfo> {
    let doc =
        EpubDoc::new(path).map_err(|e| EngineError::Epub(format!("{}: {e}", path.display())))?;
    let mut info = EpubInfo::default();
    for (key, values) in doc.metadata.iter() {
        match key.as_str() {
            "identifier" => {
                if info.isbn.is_none() {
                    info.isbn = values.iter().find_map(|v| normalize_isbn(v));
                }
            }
            "title" => info.title = values.first().cloned(),
            "creator" => info.authors = values.clone(),
            "language" => info.language = values.first().cloned(),
            _ => {}
        }
    }
    Ok(info)
}

/// A book's chapter list, and **which file said so**.
///
/// The sha256 travels with the entries because nothing is stored: a caller that
/// wants to cache this — a TUI redrawing per keypress — keys on exactly that,
/// and a caller comparing two answers can tell whether the book got a better
/// file rather than whether the parser changed its mind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableOfContents {
    pub sha256: String,
    /// In reading order. Empty means the file carries no navigable TOC.
    pub entries: Vec<TocEntry>,
}

/// One line of a book's table of contents.
///
/// Flat with a `depth` rather than a tree of children: every consumer this has
/// (a chapter list, naming the chapter a position falls in) walks it in reading
/// order, and a tree makes the common case — "the entries, in order" — the
/// awkward one. The nesting is not lost; it is a column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// The chapter's name, as the book gives it.
    pub label: String,
    /// 0 for a top-level entry, 1 for a section inside one, and so on.
    pub depth: usize,
    /// The resource this entry points at, relative to the epub root, fragment
    /// included (`OEBPS/ch2.xhtml#part-two`). Kept as text because it is the
    /// book's own name for the place, not a path on this machine.
    pub target: String,
    /// Where `target` sits in the spine, when it names a spine item at all.
    ///
    /// `None` is ordinary: a TOC may point into the middle of a document, or at
    /// a resource the spine does not carry. It is deliberately **not** a page
    /// number — see [`table_of_contents`].
    pub spine_index: Option<usize>,
}

/// The book's own table of contents, read from the file.
///
/// **Derived on demand, never stored, and that is the decision this carries.**
/// The file is content-addressed in `files_dir`, so re-reading it is cheap and
/// is always current; a stored copy is a *second answer* that goes stale the
/// moment a better file is attached to the same book, and nothing would notice.
/// Three more reasons, each of which outlived the first:
///
/// - **A TOC is not a fact with an origin.** Every other column added by
///   migration `0013` is a claim by a *source* — a provider said this — and gets
///   a `field_provenance` row saying who. A chapter list is a pure function of a
///   sha256, so a stamp naming `epub` as its source would be attributing a
///   value to a party that never claimed anything, and `rb set` could not
///   meaningfully let the user own it. A column whose provenance is a lie is
///   worse than a column that does not exist.
/// - **It cannot be corrected, so it must not be persisted.** `docs/decisions.md`
///   bans a field a provider can write and a user cannot; the honest way out is
///   a field nobody writes.
/// - **The parse will get better.** `epub =2.1.4` reads `toc.ncx` only — an
///   EPUB3 book with just a `nav` document comes back empty — and it silently
///   drops any `navPoint` with no `playOrder`. Derived, every book gets the
///   better answer the day that changes. Stored, the fleet keeps the worse one
///   until something invalidates a cache nobody wrote.
///
/// If this ever becomes hot — a TUI redrawing a chapter list per keypress — the
/// cache keys on `book_files.sha256`, which is already the primary key there.
/// Adding a cache later is cheap; removing a stored copy later is a migration.
///
/// **An empty list is an answer**: this file carries no navigable TOC. It is
/// not an error, and no caller should read it as one.
///
/// **It does not resolve a `pos0`.** `docs/decisions.md` is explicit that a
/// `pos0` is a cre-engine xpointer and that resolving one means reimplementing
/// enough of that engine to agree with it. Naming a chapter is a smaller claim
/// and it still needs a locator; the locator is the excerpt view's text search,
/// not this.
pub fn table_of_contents(path: &Path) -> Result<Vec<TocEntry>> {
    let doc =
        EpubDoc::new(path).map_err(|e| EngineError::Epub(format!("{}: {e}", path.display())))?;
    let mut out = Vec::new();
    flatten(&doc, &doc.toc, 0, &mut out);
    Ok(out)
}

fn flatten<R: std::io::Read + std::io::Seek>(
    doc: &EpubDoc<R>,
    points: &[epub::doc::NavPoint],
    depth: usize,
    out: &mut Vec<TocEntry>,
) {
    for p in points {
        let target = p.content.to_string_lossy().to_string();
        // The spine is keyed on resources, and a resource never carries the
        // fragment an entry may point at (`ch2.xhtml#part-two`). Look it up by
        // the document, and keep the fragment in `target` where it belongs.
        let (file, _) = target.split_once('#').unwrap_or((target.as_str(), ""));
        out.push(TocEntry {
            label: p.label.trim().to_string(),
            depth,
            spine_index: doc.resource_uri_to_chapter(&PathBuf::from(file)),
            target,
        });
        flatten(doc, &p.children, depth + 1, out);
    }
}

/// Extract the embedded cover image into `images_dir`, measured and named by
/// content like every other cover.
///
/// **This used to name the file `slugify(title).ext`**, which collides on two
/// editions of one title — the same class of bug as the Google Books one, minus
/// the URL. It also trusted the OPF's declared mime type for the extension,
/// which is an epub author's claim about their own file rather than a reading
/// of it. Both go through [`crate::images::store_cover`] now, which hashes the
/// bytes and takes the extension from what they actually are.
pub fn extract_cover(path: &Path, images_dir: &Path) -> Result<Option<crate::images::CoverFile>> {
    let mut doc =
        EpubDoc::new(path).map_err(|e| EngineError::Epub(format!("{}: {e}", path.display())))?;
    let Some((data, _mime)) = doc.get_cover() else {
        return Ok(None);
    };
    crate::images::store_cover(&data, images_dir).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real, committed epub — generated by `cargo run -p corpus --
    /// gen-synthetic`, ~2 KB, with a known ISBN in its OPF. Unconditional, so
    /// these tests actually run on every machine and in CI.
    fn generated_fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/koreader/synthetic/Gen-Isbn-Match.epub")
    }

    /// The other committed epub, whose whole reason to exist is its `toc.ncx`
    /// (`cargo run -p corpus -- gen-synthetic`). Unconditional, like its
    /// neighbour: a TOC test that only runs on the machine with `epubs/` is a
    /// TOC test that does not run.
    fn toc_fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/koreader/synthetic/Gen-Toc-Chapters.epub")
    }

    /// The user's own sample epubs, which live in a gitignored dir.
    fn optional_fixture() -> Option<PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../epubs/pachinko.epub");
        p.exists().then_some(p)
    }

    /// A skip must be loud, and refusable. Both tests below used to `return`
    /// silently when their fixture was absent — which is every machine without
    /// `epubs/` — so they were green without asserting anything.
    /// `READINGBUDDY_REQUIRE_FIXTURES=1` turns every such skip into a failure,
    /// which is what the nightly job sets so a broken fetch cannot masquerade
    /// as a passing build.
    fn skip(reason: &str) {
        if std::env::var("READINGBUDDY_REQUIRE_FIXTURES").is_ok() {
            panic!("REQUIRE_FIXTURES set but {reason}");
        }
        eprintln!("SKIPPED: {reason}");
    }

    #[test]
    fn reads_metadata_from_the_generated_epub() {
        let info = epub_info(&generated_fixture()).expect("generated fixture must parse");
        assert_eq!(info.title.as_deref(), Some("The ISBN Matched Book"));
        assert_eq!(info.language.as_deref(), Some("en"));
        // The OPF carries a uuid identifier *before* the isbn one; picking the
        // ISBN out means scanning them all rather than taking the first.
        assert_eq!(info.isbn.as_deref(), Some("9780316769488"));
    }

    #[test]
    fn a_nonexistent_epub_is_an_error_not_a_panic() {
        let err = epub_info(Path::new("/definitely/not/here.epub")).unwrap_err();
        assert!(matches!(err, EngineError::Epub(_)), "{err:?}");
    }

    /// Users import arbitrary files. A corrupt one must be rejected, never
    /// panic the app.
    #[test]
    fn corrupt_epubs_are_rejected_rather_than_panicking() {
        let tmp = tempfile::tempdir().unwrap();
        let good = std::fs::read(generated_fixture()).unwrap();

        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("empty", Vec::new()),
            ("not a zip", b"this is plainly not a zip archive".to_vec()),
            ("truncated zip", good[..good.len() / 2].to_vec()),
            ("header only", good[..4].to_vec()),
            ("zip with garbage tail", {
                let mut v = good.clone();
                v.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
                v
            }),
            ("nulled central directory", {
                let mut v = good.clone();
                let n = v.len();
                for b in v.iter_mut().skip(n.saturating_sub(64)) {
                    *b = 0;
                }
                v
            }),
        ];

        for (name, bytes) in cases {
            let path = tmp.path().join(format!("{}.epub", name.replace(' ', "-")));
            std::fs::write(&path, &bytes).unwrap();
            // The contract is "does not panic". Either outcome is acceptable —
            // some corruptions still leave a readable OPF.
            let _ = epub_info(&path);
            let _ = extract_cover(&path, &tmp.path().join("images"));
            let _ = table_of_contents(&path);
        }
    }

    /// The chapter list, flattened, in **reading order** — which is
    /// `playOrder`, not the order the `navPoint`s are written in. The fixture
    /// declares chapter three before chapter two precisely so that a parse that
    /// took document order would fail here rather than on somebody's book.
    #[test]
    fn reads_the_chapter_list_from_the_generated_epub() {
        let toc = table_of_contents(&toc_fixture()).expect("generated fixture must parse");
        let shape: Vec<(&str, usize, Option<usize>)> = toc
            .iter()
            .map(|e| (e.label.as_str(), e.depth, e.spine_index))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("One: The Beginning", 0, Some(0)),
                ("Two: The Middle", 0, Some(1)),
                // Nested inside its parent, and pointing at a fragment of the
                // *same* document — so it shares chapter two's spine position.
                ("Two, part two", 1, Some(1)),
                ("Three: The End", 0, Some(2)),
            ]
        );
        // The fragment survives in the target: it is the book's own name for
        // the place, and dropping it would make two entries indistinguishable.
        assert!(
            toc[2].target.ends_with("ch2.xhtml#part-two"),
            "{:?}",
            toc[2].target
        );
    }

    /// An epub with no `toc.ncx` is not a broken epub. It answers "no navigable
    /// table of contents", which is an empty list rather than an error — the
    /// other committed fixture is exactly that file.
    #[test]
    fn an_epub_without_a_toc_answers_with_no_chapters() {
        assert!(
            table_of_contents(&generated_fixture())
                .expect("a valid epub without an ncx still parses")
                .is_empty()
        );
    }

    /// A TOC read off a **real** book, which the synthetic one cannot stand in
    /// for: real ncx files nest three deep, point at fragments, and carry
    /// labels with entities in them.
    #[test]
    fn reads_a_chapter_list_from_a_real_epub() {
        let Some(path) = optional_fixture() else {
            skip("epubs/pachinko.epub is missing");
            return;
        };
        let toc = table_of_contents(&path).unwrap();
        assert!(!toc.is_empty(), "a real trade epub has chapters");
        for e in &toc {
            assert!(!e.label.is_empty(), "a nameless chapter is not a chapter");
            assert!(!e.target.is_empty());
        }
        assert!(
            toc.iter().any(|e| e.spine_index.is_some()),
            "a TOC none of whose entries names a spine item is a parse that missed"
        );
    }

    #[test]
    fn reads_metadata_from_fixture() {
        let Some(path) = optional_fixture() else {
            skip("epubs/pachinko.epub is missing");
            return;
        };
        let info = epub_info(&path).unwrap();
        assert!(info.title.is_some());
        // If an ISBN was found it must be a validated, normalized one.
        if let Some(isbn) = &info.isbn {
            assert!(normalize_isbn(isbn).is_some());
        }
    }

    #[test]
    fn extracts_cover_from_fixture() {
        let Some(path) = optional_fixture() else {
            skip("epubs/pachinko.epub is missing");
            return;
        };
        let dir = std::env::temp_dir().join(format!("rb-epub-test-{}", std::process::id()));
        let cover = extract_cover(&path, &dir).unwrap();
        if let Some(c) = cover {
            assert!(c.path.exists());
            assert!(std::fs::metadata(&c.path).unwrap().len() > 0);
            // Named by content, not by `slugify(title)` — so two editions of
            // one book no longer overwrite each other's jacket.
            let stem = c.path.file_stem().unwrap().to_string_lossy().to_string();
            assert_eq!(stem.len(), 64, "{stem}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
