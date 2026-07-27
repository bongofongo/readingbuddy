//! Fuzz `epub::epub_info` over arbitrary bytes.
//!
//! Honest about its value: most of what this exercises is the third-party
//! `epub` crate and the zip reader beneath it, so a finding here is usually an
//! upstream bug. It earns its place anyway because a malformed epub is a
//! *realistic* input — users import arbitrary files — and a panic in that path
//! crashes the app rather than reporting a bad file.
//!
//! The cheap 80% of this is already covered by
//! `epub::tests::corrupt_epubs_are_rejected_rather_than_panicking`, which runs
//! on stable in CI. This target is for the long tail.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // `epub_info` takes a path, so the bytes have to reach the disk.
    let Ok(dir) = tempfile::tempdir() else { return };
    let path = dir.path().join("fuzz.epub");
    if std::fs::write(&path, data).is_err() {
        return;
    }

    if let Ok(info) = readingbuddy::epub::epub_info(&path) {
        // Any ISBN that comes out must already be normalized and valid —
        // epub_info is one of the entry points that feeds the library.
        if let Some(isbn) = &info.isbn {
            assert!(
                readingbuddy::normalize_isbn(isbn).as_deref() == Some(isbn.as_str()),
                "epub_info returned a non-normalized ISBN: {isbn:?}"
            );
        }
    }

    let _ = readingbuddy::epub::extract_cover(&path, &dir.path().join("images"));
});
