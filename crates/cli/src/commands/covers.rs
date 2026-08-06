//! `covers` — the door onto item 20's back-fill.
//!
//! `Engine::measure_stored_covers` has existed since item 20 and nothing called
//! it, so every library built before migration `0014` — including the one
//! `make dev-db` builds — had a `cover_path` on every book and a NULL
//! `cover_aspect` on every book. A shelf reading that column would have
//! concluded the column does not work.
//!
//! **A command and not a migration**, for the engine's own reason: `cover_width`
//! is the result of decoding a PNG and SQLite cannot decode one. It is
//! idempotent and cheap to repeat — the work list is
//! `cover_path IS NOT NULL AND cover_width IS NULL`, so a second run measures
//! nothing and a file that would not decode is retried for free.
//!
//! All the printing lives here; the engine does no terminal I/O.

use anyhow::Result;
use readingbuddy::Engine;

pub async fn measure(engine: &Engine) -> Result<()> {
    let (measured, unreadable) = engine.measure_stored_covers().await?;
    println!("{}", summary(measured, unreadable));
    Ok(())
}

/// The report, as a pure function so its wording can be asserted.
///
/// Two rules from `docs/decisions.md` are load-bearing here. **Absence is not
/// zero**: a library whose covers are all measured has no work, and printing
/// `measured 0 covers` states that as a quantity of failure rather than as a
/// fact about the library. And there is **no task-completion framing** — this
/// reports what it did, and never how many books are still waiting for
/// something to be done to them.
fn summary(measured: usize, unreadable: usize) -> String {
    let covers = |n: usize| if n == 1 { "cover" } else { "covers" };
    let retry = "left as they were, and the next run tries them again";
    match (measured, unreadable) {
        (0, 0) => "every stored cover is already measured.".to_string(),
        (m, 0) => format!("measured {m} {}.", covers(m)),
        (0, u) => format!("{u} {} would not decode — {retry}.", covers(u)),
        (m, u) => format!(
            "measured {m} {}; {u} would not decode — {retry}.",
            covers(m)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nothing-to-do line must not be a zero.
    ///
    /// `make dev-db` runs this every time, and after the first pass every run
    /// after it lands on this branch — so this is the line the user sees most.
    /// "measured 0 covers" reads as a failure to measure anything; the library
    /// is simply already measured.
    #[test]
    fn an_already_measured_library_is_never_reported_as_zero() {
        let s = summary(0, 0);
        assert!(!s.contains('0'), "absence is not zero: {s}");
        assert!(s.contains("already measured"), "{s}");
    }

    /// An unreadable file is a fact, not a queue.
    ///
    /// It names what happens next (nothing was written; the next run retries)
    /// rather than counting work the user has not done.
    #[test]
    fn an_unreadable_cover_states_what_happened_to_it() {
        for s in [summary(0, 1), summary(3, 2)] {
            assert!(s.contains("would not decode"), "{s}");
            assert!(s.contains("next run tries them again"), "{s}");
        }
        assert!(summary(3, 0).contains("measured 3 covers"));
        assert!(summary(1, 0).contains("measured 1 cover"));
    }
}
