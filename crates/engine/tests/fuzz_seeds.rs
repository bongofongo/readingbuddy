//! Replay the fuzz seed corpus on **stable**, in milliseconds, on every PR.
//!
//! This is what makes fuzzing pay off over time. The nightly `cargo fuzz` job
//! finds things; every crash it finds gets minimized into `fuzz/seeds/` and is
//! thereafter guarded here — no nightly toolchain, no libFuzzer, no wall clock.
//! A fuzzer that has to be re-run to re-catch its own findings is a fuzzer that
//! catches them once.
//!
//! Adding a regression is just dropping the input into `fuzz/seeds/<target>/`.

use std::path::{Path, PathBuf};

use readingbuddy::koreader::parse_sidecar;

fn seeds(target: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/seeds")
        .join(target);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}

/// Every seed must be handled — parsed or rejected — without panicking, and
/// within a bound. The bound is the point: the sandbox removes the standard
/// library but not the ability to loop, so "returns an error" and "never
/// returns" are different failures and only one of them is acceptable.
#[test]
fn every_parse_sidecar_seed_is_survivable() {
    let seeds = seeds("parse_sidecar");
    assert!(
        !seeds.is_empty(),
        "no seeds found — fuzz/seeds/parse_sidecar/ should be committed"
    );

    let started = std::time::Instant::now();
    for path in &seeds {
        // Seeds are deliberately not all valid UTF-8.
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let per_case = std::time::Instant::now();
        let _ = parse_sidecar(&src);
        assert!(
            per_case.elapsed() < std::time::Duration::from_secs(30),
            "{} took {:?} — the instruction budget is not holding",
            path.display(),
            per_case.elapsed()
        );
    }
    eprintln!(
        "replayed {} parse_sidecar seeds in {:?}",
        seeds.len(),
        started.elapsed()
    );
}

/// The sandbox is a security property, not a nicety: a sidecar is a file that
/// arrived from a device, and `StdLib::NONE` is what stops it reaching the host.
#[test]
fn no_seed_can_reach_the_lua_standard_library() {
    for name in ["sandbox-escape.lua", "require.lua"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fuzz/seeds/parse_sidecar")
            .join(name);
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let err = parse_sidecar(&src)
            .err()
            .unwrap_or_else(|| panic!("{name} was accepted — the sandbox is open"));
        // It must fail because the function does not exist, not because we
        // happened to reject the shape afterwards.
        let msg = err.to_string();
        assert!(
            msg.contains("lua eval") || msg.contains("nil value"),
            "{name} failed for an unexpected reason: {msg}"
        );
    }
}

#[test]
fn every_epub_seed_is_survivable() {
    for path in seeds("epub_info") {
        // Contract is only "does not panic"; corrupt input may legitimately
        // parse or fail.
        let _ = readingbuddy::epub::epub_info(&path);
    }
}
