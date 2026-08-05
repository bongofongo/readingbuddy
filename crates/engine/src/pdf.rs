//! What a PDF will say about itself — a length, and sometimes a name.
//!
//! [`crate::epub::epub_info`]'s twin, and item 22's one piece of genuinely new
//! engine work. Everything else the "reading here" row needed already existed:
//! `book_files` stores any format, `import_file` copies bytes in
//! content-addressed, `update_progress` writes to the active reading. What no
//! module could answer was *how long is this file*, and without that every
//! display of a locally-read PDF degrades to a page with no denominator.
//!
//! ## Zero is not a page count
//!
//! This module has exactly one rule and it is worth stating before the API:
//! **a length we could not read is [`None`], never `Some(0)`.**
//!
//! That is not defensive phrasing. `lopdf`'s `extract_page_count` returns `0`
//! from *seven* different failure branches — no `/Root`, an unresolvable
//! catalog, a catalog that is not a dictionary, no `/Pages`, an unresolvable
//! page tree, a page tree that is not a dictionary, and a reference cycle — as
//! well as from a document that genuinely has no pages. A password-protected
//! PDF also returns `0`, because the page tree is behind the encryption.
//! Passing that number through would put a false denominator into
//! [`crate::Progress`], which is precisely what item 17 spent an item removing:
//! `a_reported_length_is_never_zero` is the property, and `Progress::Started {
//! page: Some(n), of: None, fraction: None }` — "there is a page and no
//! percentage" — is the answer this module exists to make reachable.
//!
//! So [`pdf_info`] normalises `0` to `None` at the boundary, once, and the
//! callers downstream never see the sentinel.
//!
//! ## The title most PDFs do not have
//!
//! `/Info /Title` is optional and usually absent; where it is present it is
//! very often the *producer's* echo of a source filename — `Microsoft Word -
//! chapter3final.doc` is the canonical shape, and Acrobat Distiller, LaTeX
//! front-ends and half the office suites all do some version of it. A book
//! created with that as its title is wrong in a way no type checker reaches, so
//! [`title_or_none`] refuses a title that still carries an authoring-tool file
//! extension and lets the caller fall back to the filename stem, which
//! `files.rs` already does for every format with no metadata reader.
//!
//! **The empty title is observed, not guessed.** Run against three PDFs off
//! this machine (`README-hintview.pdf` 1.5, `READ ME FIRST.pdf` 1.7, one of
//! Automator's icons 1.6), two of the three came back `Some("")` — the `/Info`
//! dictionary is present and the `/Title` entry is an empty string. `Some("")`
//! is not absence to anything downstream: it survives `Option` handling, it
//! survives `unwrap_or_default`, and it lands as a book with a blank name. That
//! is why this function returns `Option<String>` with emptiness folded into
//! `None` at the boundary rather than a `String` the caller has to remember to
//! check, and why `a_title_is_absent_or_non_empty` is a property.
//!
//! Nothing else is read. No text extraction, no outline, no embedded cover, and
//! **no viewer** — `docs/gui/gui-vision.md` puts one explicitly out of scope.

use std::path::Path;

use crate::error::{EngineError, Result};

/// What a PDF said about itself.
///
/// Both fields are absent far more often than an epub's are, and that is the
/// normal case rather than a degraded one.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PdfInfo {
    /// How many pages the file has. **Never `Some(0)`** — see the module doc.
    pub page_count: Option<i64>,
    /// The document's own title, where it has one worth believing. Never
    /// `Some("")`.
    pub title: Option<String>,
}

/// Read a PDF's metadata.
///
/// `Err` means the bytes are not a PDF this engine can parse at all — no
/// header, no cross-reference table, a truncated file. `Ok` with everything
/// absent means the file parsed and would not say, which includes the
/// password-protected case: an encrypted PDF is still a file worth owning, and
/// its page tree simply is not readable without the password.
///
/// Only the cross-reference table, the catalog and the `/Info` dictionary are
/// read; the page *contents* are never touched, so this is cheap on a large
/// file and does not depend on how the pages were compressed.
#[tracing::instrument(fields(path = %path.display()))]
pub fn pdf_info(path: &Path) -> Result<PdfInfo> {
    let meta = lopdf::Document::load_metadata(path)
        .map_err(|e| EngineError::Pdf(format!("{}: {e}", path.display())))?;

    // The one rule. `0` is lopdf's sentinel for every way of not knowing as
    // well as for a genuinely empty document, and the two are the same answer
    // here: we do not have a length.
    let page_count = (meta.page_count > 0).then_some(i64::from(meta.page_count));
    let title = meta.title.as_deref().and_then(title_or_none);

    tracing::debug!(
        page_count,
        has_title = title.is_some(),
        "read pdf metadata"
    );
    Ok(PdfInfo { page_count, title })
}

/// Authoring-tool extensions that appear in `/Info /Title` when a producer has
/// echoed a source filename rather than a title.
///
/// Deliberately **not** a general "does this look like a filename" heuristic:
/// real books are called *Sync* and *Java*, and a rule broad enough to catch
/// those is a rule that throws away good titles. This list is the observable
/// artefact — a word-processor or typesetter document name that survived into
/// the PDF — and nothing wider.
const AUTHORING_EXTENSIONS: [&str; 10] = [
    ".doc", ".docx", ".rtf", ".odt", ".pages", ".tex", ".indd", ".qxd", ".pdf", ".ps",
];

/// A `/Info /Title` worth believing, or [`None`].
///
/// Trimmed, never empty, and never a producer's echo of the file it was made
/// from. The caller's fallback is the filename stem, which is no worse and
/// often much better — `Kant - Critique of Pure Reason.pdf` beats
/// `Microsoft Word - kant_final_v2.doc` by a distance.
fn title_or_none(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if AUTHORING_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        tracing::debug!("pdf title looks like a source filename; falling back");
        return None;
    }
    Some(t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a classic (PDF 1.4, uncompressed cross-reference table) document.
    ///
    /// Generated rather than committed, for `gen-kostats`' reason: a `.pdf` in
    /// the tree would be the one fixture nobody can read a diff of, and its
    /// bytes would depend on whichever writer produced it. Assembling it here
    /// from text also means the fixture owes nothing to `lopdf` — the same
    /// argument `crates/corpus` makes about not building its fixtures with the
    /// engine's own parser.
    ///
    /// The byte offsets in the cross-reference table are computed while the
    /// objects are written, because they have to be exact: `startxref` pointing
    /// at the wrong byte is how a "valid" fixture silently tests the error path.
    fn synthetic_pdf(pages: usize, title: Option<&[u8]>) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut offsets: Vec<usize> = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");

        let push = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, n: usize, body: &[u8]| {
            offsets.push(out.len());
            out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        };

        push(&mut out, &mut offsets, 1, b"<< /Type /Catalog /Pages 2 0 R >>");

        let kids: String = (0..pages)
            .map(|i| format!("{} 0 R ", i + 3))
            .collect::<String>();
        push(
            &mut out,
            &mut offsets,
            2,
            format!("<< /Type /Pages /Kids [{kids}] /Count {pages} >>").as_bytes(),
        );
        for i in 0..pages {
            push(
                &mut out,
                &mut offsets,
                i + 3,
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            );
        }

        let info_num = pages + 3;
        if let Some(t) = title {
            let mut body = b"<< /Title (".to_vec();
            body.extend_from_slice(t);
            body.extend_from_slice(b") >>");
            push(&mut out, &mut offsets, info_num, &body);
        }

        let count = offsets.len() + 1; // +1 for the free object 0
        let xref_at = out.len();
        out.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets {
            // Exactly twenty bytes per entry, which the format requires.
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        let info = if title.is_some() {
            format!(" /Info {info_num} 0 R")
        } else {
            String::new()
        };
        out.extend_from_slice(
            format!("trailer\n<< /Size {count} /Root 1 0 R{info} >>\nstartxref\n{xref_at}\n%%EOF\n")
                .as_bytes(),
        );
        out
    }

    fn write(bytes: &[u8], name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).expect("write");
        (dir, path)
    }

    #[test]
    fn reads_the_page_count_and_the_title() {
        let (_d, p) = write(&synthetic_pdf(7, Some(b"Long Walk to Freedom")), "a.pdf");
        let info = pdf_info(&p).unwrap();
        assert_eq!(info.page_count, Some(7));
        assert_eq!(info.title.as_deref(), Some("Long Walk to Freedom"));
    }

    #[test]
    fn a_pdf_with_no_info_dictionary_has_no_title_and_still_has_a_length() {
        let (_d, p) = write(&synthetic_pdf(3, None), "b.pdf");
        let info = pdf_info(&p).unwrap();
        assert_eq!(info.page_count, Some(3));
        assert_eq!(info.title, None);
        // The ordinary case, not a degraded one: most PDFs carry no title, and
        // the caller's filename stem is the honest fallback.
    }

    /// The whole point of the module, as an assertion.
    ///
    /// A document whose page tree says `/Count 0` is indistinguishable from
    /// every one of lopdf's seven "could not tell" branches, and both must come
    /// back as absence. `Some(0)` here would be a false denominator that
    /// `Progress` cannot tell from a real one.
    #[test]
    fn a_length_that_could_not_be_read_is_absent_and_never_zero() {
        let (_d, p) = write(&synthetic_pdf(0, None), "empty.pdf");
        let info = pdf_info(&p).unwrap();
        assert_eq!(info.page_count, None, "zero pages must read as absence");
    }

    #[test]
    fn a_pdf_with_no_catalog_is_absence_rather_than_zero() {
        // A structurally valid file whose trailer names no `/Root`: lopdf's
        // first `return Ok(0)` branch, reached without corrupting anything.
        let mut bytes = synthetic_pdf(4, None);
        let s = String::from_utf8(bytes.clone()).unwrap();
        bytes = s.replace("/Root 1 0 R", "").into_bytes();
        let (_d, p) = write(&bytes, "rootless.pdf");
        match pdf_info(&p) {
            Ok(info) => assert_eq!(info.page_count, None),
            // Refusing the file outright is also an acceptable answer; what is
            // not acceptable is a length of zero.
            Err(EngineError::Pdf(_)) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn bytes_that_are_not_a_pdf_are_a_typed_error() {
        let (_d, p) = write(b"this is a text file, not a pdf", "c.pdf");
        assert!(matches!(pdf_info(&p), Err(EngineError::Pdf(_))));
    }

    #[test]
    fn a_truncated_pdf_is_a_typed_error_rather_than_a_panic() {
        let full = synthetic_pdf(5, Some(b"Cut Short"));
        let (_d, p) = write(&full[..full.len() / 2], "d.pdf");
        assert!(matches!(pdf_info(&p), Err(EngineError::Pdf(_))));
    }

    #[test]
    fn a_missing_file_is_an_error_and_not_a_page_count() {
        assert!(pdf_info(Path::new("/nonexistent/nope.pdf")).is_err());
    }

    #[test]
    fn a_producers_echo_of_a_source_filename_is_not_a_title() {
        for junk in [
            "Microsoft Word - chapter3final.doc",
            "thesis.tex",
            "  report.DOCX  ",
            "scan_0001.pdf",
        ] {
            assert_eq!(title_or_none(junk), None, "{junk:?} should be refused");
        }
    }

    #[test]
    fn a_real_title_survives_the_filter() {
        for good in [
            "Long Walk to Freedom",
            "The C Programming Language",
            // Ends in a word that is *also* an extension, but not preceded by a
            // dot. The narrow rule is what keeps these.
            "Learning Pages",
            "Ghosts of the Tsunami",
        ] {
            assert_eq!(title_or_none(good).as_deref(), Some(good.trim()));
        }
    }

    #[test]
    fn an_empty_title_is_absence() {
        assert_eq!(title_or_none(""), None);
        assert_eq!(title_or_none("   \n "), None);
    }

    /// A PDF from the user's own disk, when one has been dropped in.
    ///
    /// `crates/engine/tests/fixtures/pdf/real/` is gitignored, exactly like the
    /// KOReader `real/` drop-in and `partial_md5.rs`'s `personal_data/` checks:
    /// the synthetic documents above cover *shape*, and only a file some real
    /// producer wrote covers a cross-reference **stream** with the page tree
    /// inside a compressed object stream, which is what most PDFs made this
    /// decade actually are.
    ///
    /// It **prints `SKIPPED:` and honours `READINGBUDDY_REQUIRE_FIXTURES=1`**
    /// rather than returning silently — `epub.rs` had two tests that returned
    /// silently for months, and a test that is green without asserting anything
    /// is worse than no test.
    #[test]
    fn a_real_pdf_reports_a_plausible_length() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf/real");
        let files: Vec<_> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
            })
            .collect();

        if files.is_empty() {
            let msg = format!(
                "SKIPPED: a_real_pdf_reports_a_plausible_length — no .pdf in {}",
                dir.display()
            );
            assert!(
                std::env::var("READINGBUDDY_REQUIRE_FIXTURES").is_err(),
                "{msg} (READINGBUDDY_REQUIRE_FIXTURES=1)"
            );
            println!("{msg}");
            return;
        }

        for path in files {
            let info = pdf_info(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            match info.page_count {
                Some(n) => assert!(n > 0, "{}: a page count of {n}", path.display()),
                None => println!(
                    "note: {} parsed but would not give a length",
                    path.display()
                ),
            }
            assert_ne!(
                info.title.as_deref(),
                Some(""),
                "{}: an empty title must be absence",
                path.display()
            );
        }
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// The invariant the rest of the engine relies on, over every
            /// document shape this generator can make: a length is absent or
            /// positive, and there is no third answer.
            #[test]
            fn a_reported_length_is_never_zero(pages in 0usize..24) {
                let (_d, p) = write(&synthetic_pdf(pages, None), "p.pdf");
                if let Ok(info) = pdf_info(&p) {
                    prop_assert!(info.page_count.is_none_or(|n| n > 0));
                    if pages > 0 {
                        prop_assert_eq!(info.page_count, Some(pages as i64));
                    }
                }
            }

            /// A title that survives the filter is the trimmed input, and one
            /// that does not is absence. Never an empty string, which is the
            /// third state a `String` return would have allowed.
            #[test]
            fn a_title_is_absent_or_non_empty(s in ".{0,80}") {
                match title_or_none(&s) {
                    Some(t) => {
                        prop_assert!(!t.is_empty());
                        prop_assert_eq!(t, s.trim());
                    }
                    None => prop_assert!(
                        s.trim().is_empty()
                            || AUTHORING_EXTENSIONS
                                .iter()
                                .any(|e| s.trim().to_ascii_lowercase().ends_with(e))
                    ),
                }
            }
        }
    }
}
