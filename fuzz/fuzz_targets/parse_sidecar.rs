//! Fuzz `koreader::parse_sidecar`.
//!
//! The highest-value target in the crate: it is pure, public, takes `&str`, and
//! **evaluates Lua from a file we did not write**. A `.sdr` sidecar comes off a
//! user's e-reader, and the sandbox (`StdLib::NONE` plus an instruction budget)
//! is the only thing standing between that and the host.
//!
//! Run with `-timeout=10`. That flag is not incidental — it is how a
//! non-terminating chunk is detected, which is exactly the failure mode the
//! instruction budget exists to prevent:
//!
//!   cargo +nightly fuzz run parse_sidecar -- -timeout=10 fuzz/seeds/parse_sidecar

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // The contract is only "does not panic, does not hang". Either outcome is
    // fine: most inputs are not valid Lua at all.
    if let Ok(sidecar) = readingbuddy::koreader::parse_sidecar(data) {
        // If it did parse, the invariants `entry_to_highlight` promises must
        // hold — otherwise the import path downstream is working from a lie.
        for h in &sidecar.highlights {
            assert!(!h.text.is_empty(), "parsed an empty highlight text");
            assert!(h.pos0.is_some(), "parsed a highlight with no pos0");
            assert_eq!(h.source, "koreader");
        }
    }
});
