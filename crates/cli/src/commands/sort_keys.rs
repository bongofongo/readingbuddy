//! `sort-keys` — the door onto item 34's back-fill.
//!
//! Migration `0016` indexes the sort keys, and two of them are *derived*:
//! `sort_title` drops a leading article, `sort_author` packs the engine's parse
//! of a human name. SQLite can do neither, so the migration adds the columns
//! empty and this fills them — `crates/cli/src/commands/covers.rs`'s shape
//! exactly, for `0014`'s reason one layer over.
//!
//! It is a door because a back-fill with no door is a function nothing ever
//! calls: `Engine::measure_stored_covers` sat unexercised for a whole wave on
//! precisely that, and this back-fill has a sharper edge than that one did —
//! until it runs, a book files at the *top* of an author sort rather than under
//! its author.
//!
//! Idempotent and cheap to repeat: the work list is `sort_author IS NULL`, which
//! is *never computed* and never *no author*, so a second run does nothing.
//! `make dev-db` runs it, which means the already-filed line below is the one a
//! user normally sees.
//!
//! All the printing lives here; the engine does no terminal I/O.

use anyhow::Result;
use readingbuddy::Engine;

pub async fn rebuild(engine: &Engine) -> Result<()> {
    let filed = engine.rebuild_sort_keys().await?;
    println!("{}", summary(filed));
    Ok(())
}

/// The report, as a pure function so its wording can be asserted.
///
/// Two rules from `docs/decisions.md` are load-bearing here, and they are the
/// same two `covers` obeys. **Absence is not zero**: a library whose books are
/// all filed has no work, and `filed 0 books` states that as a failure to file
/// anything rather than as a fact about the library. And there is **no
/// task-completion framing** — this says what it did, never how many books are
/// still waiting for something to be done to them.
fn summary(filed: usize) -> String {
    match filed {
        0 => "every book already files under a sort key.".to_string(),
        1 => "filed 1 book.".to_string(),
        n => format!("filed {n} books."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nothing-to-do line must not be a zero.
    ///
    /// `make dev-db` runs this every time, so after the first pass every run
    /// after it lands on this branch — which makes it the line the user sees
    /// most. "filed 0 books" reads as a failure; the library is simply already
    /// filed.
    #[test]
    fn an_already_filed_library_is_never_reported_as_zero() {
        let s = summary(0);
        assert!(!s.contains('0'), "absence is not zero: {s}");
        assert!(s.contains("already files"), "{s}");
    }

    /// One book is a book, and nothing here counts what has not been done.
    #[test]
    fn the_count_is_of_what_it_did() {
        assert_eq!(summary(1), "filed 1 book.");
        assert_eq!(summary(7), "filed 7 books.");
        for s in [summary(0), summary(1), summary(7)] {
            for banned in ["remaining", "left", "still", "waiting", "of "] {
                assert!(!s.contains(banned), "completion framing in {s:?}: {banned}");
            }
        }
    }
}
