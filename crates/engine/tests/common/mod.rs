//! Shared scaffolding for the engine's integration tests.
//!
//! `common/mod.rs` rather than `common.rs`: the directory form is the one cargo
//! does not also pick up as its own test target.
//!
//! Everything here is offline. `sqlite::memory:` plus a `TempDir` for the vault
//! and images, so a test leaves nothing behind and cannot reach the developer's
//! library.

// One file, compiled into every test binary that declares `mod common;`. A
// helper used by one of them is dead code in the others, and CI runs
// `clippy --all-targets -D warnings`, so "unused here" has to be allowed rather
// than chased. It is the normal case for this file, not a smell.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use readingbuddy::{Book, Engine, EngineConfig, NewHighlight};

/// The committed tier-1 fixtures. Shapes, not scale — see `CLAUDE.md` on the
/// fixture tiers.
pub const SYNTHETIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/koreader/synthetic"
);

/// An engine on an in-memory database with a temp vault.
///
/// The config is built *literally* rather than through `EngineConfig::rooted_at`,
/// which reads `GOOGLE_BOOKS_API_KEY` from the ambient environment — that would
/// make these tests pass or fail depending on the developer's shell.
///
/// Returns the `TempDir` as well, and the caller must hold it: dropping it
/// deletes the vault out from under the engine mid-test.
pub async fn engine() -> (tempfile::TempDir, Engine) {
    engine_with_calibre(None).await
}

/// The same engine, told where to look for calibre's tools.
///
/// `calibre_bin_dir` rather than a `PATH` edit: `PATH` is process-global and
/// `set_var` from a test races every other test in the same binary.
pub async fn engine_with_calibre(bin_dir: Option<PathBuf>) -> (tempfile::TempDir, Engine) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = EngineConfig {
        db_url: "sqlite::memory:".into(),
        images_dir: tmp.path().join("database/images"),
        files_dir: tmp.path().join("database/files"),
        vault_dir: tmp.path().join("vault"),
        log_dir: tmp.path().join("logs"),
        google_api_key: None,
        calibre_bin_dir: bin_dir,
    };
    let engine = Engine::open(config).await.expect("engine opens");
    (tmp, engine)
}

/// A plain `Book` with a title and one author.
pub fn book(title: &str) -> Book {
    Book {
        title: Some(title.into()),
        authors: vec!["Min Jin Lee".into()],
        ..Default::default()
    }
}

/// Save `title` through the facade and return its id.
pub async fn seed_book(engine: &Engine, title: &str) -> i64 {
    engine
        .save_book(&book(title))
        .await
        .expect("save")
        .id
        .expect("a stored book has an id")
}

/// A KOReader-shaped highlight. `when` is a `%Y-%m-%d %H:%M:%S` device
/// timestamp — the field reading-window attribution matches on, so tests that
/// care about which reading a highlight lands in set it deliberately.
pub fn highlight(text: &str, when: &str) -> NewHighlight {
    NewHighlight {
        text: text.into(),
        chapter: Some("Ch 1".into()),
        page: Some(12),
        pos0: Some(format!("/body/p[1]/text().{text}")),
        pos1: None,
        ko_datetime: Some(when.into()),
        ko_datetime_updated: None,
        color: Some("yellow".into()),
        note: None,
        source: "koreader".into(),
    }
}

/// Copy a committed fixture `.sdr` directory into a writable tree and return the
/// path of the sidecar inside it.
///
/// Always a copy, never the fixture itself: a scan touches mtimes, rewrites
/// bytes and prunes rows, so running one against the committed tree would edit
/// the repository. The fixtures are the shapes; the tempdir is the device.
pub fn place(device_root: &Path, fixture: &str, as_name: &str) -> PathBuf {
    let src = Path::new(SYNTHETIC).join(fixture);
    let dst = device_root.join(as_name);
    std::fs::create_dir_all(&dst).unwrap();
    let mut sidecar = None;
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        std::fs::copy(entry.path(), dst.join(&name)).unwrap();
        let name = name.to_string_lossy().into_owned();
        if name.starts_with("metadata.") && name.ends_with(".lua") {
            sidecar = Some(dst.join(name));
        }
    }
    sidecar.unwrap_or_else(|| panic!("{fixture} has no sidecar"))
}

/// Rewrite a sidecar's text and move its mtime forward.
///
/// The mtime bump is not cosmetic: `sidecar_seen` pre-filters on `(size, mtime)`
/// and will skip the re-parse entirely if the clock has not moved. mtime is
/// stored in *nanoseconds* precisely because a device flushes on every page turn
/// and two edits inside one second are ordinary — but a test writes both edits
/// far faster than that, so it must set the time rather than trust it.
pub fn rewrite_sidecar(path: &Path, edit: impl FnOnce(String) -> String) {
    let before = std::fs::read_to_string(path).expect("read sidecar");
    std::fs::write(path, edit(before)).expect("write sidecar");
    let f = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open sidecar");
    let bumped = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    f.set_modified(bumped).expect("bump mtime");
}

/// A loud skip. Prints `SKIPPED:` and honours `READINGBUDDY_REQUIRE_FIXTURES`,
/// which the nightly corpus job sets — so a broken fetch fails the build instead
/// of masquerading as a green one.
///
/// A test that quietly `return`s when its fixture is missing is green without
/// asserting anything, which `epub.rs` did for months. See `CLAUDE.md`, Engine
/// standards.
pub fn skipped(reason: &str, hint: &str) -> bool {
    if std::env::var("READINGBUDDY_REQUIRE_FIXTURES").is_ok() {
        panic!("REQUIRE_FIXTURES is set but {reason}");
    }
    eprintln!("SKIPPED: {reason} — {hint}");
    true
}

/// A valid, ~2 KB epub with **no ISBN identifier**.
///
/// The missing ISBN is the point, not an oversight: `import_epub` looks a found
/// ISBN up through the providers, and this suite makes no network calls. Built
/// rather than committed, for the same reason the corpus generator builds its
/// own — what is inside it stays readable in the diff.
///
/// Shared rather than copied: `book_files.rs` needs the same file for the same
/// reason (its create-a-book path goes through `import_epub`), and two epub
/// builders would be two definitions of what a valid epub is.
pub fn write_isbnless_epub(path: &std::path::Path, title: &str) {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    // `mimetype` must be first and STORED per the epub spec.
    zip.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(b"application/epub+zip").unwrap();

    let opts = SimpleFileOptions::default();
    zip.start_file("META-INF/container.xml", opts).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#,
    )
    .unwrap();

    zip.start_file("OEBPS/content.opf", opts).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">urn:uuid:00000000-0000-4000-8000-000000000000</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator>A Test Author</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
  </spine>
</package>
"#
        )
        .as_bytes(),
    )
    .unwrap();

    zip.start_file("OEBPS/ch1.xhtml", opts).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title></head>
<body><h1>{title}</h1><p>A single paragraph, so the spine is not empty.</p></body></html>
"#
        )
        .as_bytes(),
    )
    .unwrap();
    zip.finish().unwrap();
}
