//! `calibre status` / `calibre convert` / `calibre import`.
//!
//! Feature-detected all the way out to the surface: with calibre absent these
//! commands say so and name nothing to install. `docs/decisions.md` is explicit
//! that readingbuddy never asks the user to install or configure other software,
//! so `status` reports and stops — it does not suggest, and it does not fail.
//!
//! All the printing lives here; the engine does no terminal I/O.

use std::path::{Path, PathBuf};

use anyhow::Result;
use readingbuddy::calibre::ImportOptions;
use readingbuddy::{CalibreBookReport, CalibreReport, Engine, EngineError};

/// What calibre is available for, and where it was found.
///
/// Exists because feature detection is otherwise invisible: with calibre absent
/// the features are simply not there, and without this the user's only way to
/// find that out is to try one and read a refusal.
pub async fn status(engine: &Engine) -> Result<()> {
    let cal = engine.calibre();
    let line = |label: &str, found: Option<&Path>| match found {
        Some(p) => println!("{label:<14} {}", p.display()),
        None => println!("{label:<14} not found"),
    };
    line("ebook-convert", cal.convert_path());
    line("calibredb", cal.calibredb_path());

    println!();
    println!(
        "{}",
        availability(cal.can_convert(), cal.can_read_library())
    );
    Ok(())
}

/// The summary line, as a pure function so its wording can be asserted.
///
/// It has to be, and not through the process: detection falls through to `PATH`
/// and then to the directories calibre installs itself in, so on a machine with
/// calibre the absent branch is unreachable from outside — which is exactly the
/// branch with a rule attached to it. `docs/decisions.md` says readingbuddy
/// never asks the user to install or configure other software, so this must
/// report and stop. Not "install calibre to enable these": they are optional
/// extras on top of an app that works without them.
fn availability(convert: bool, library: bool) -> &'static str {
    match (convert, library) {
        (true, true) => "conversion and library import are available.",
        (true, false) => "conversion is available; library import is not.",
        (false, true) => "library import is available; conversion is not.",
        (false, false) => "calibre is not installed; these features are not available.",
    }
}

/// Tier (i): `ebook-convert IN OUT`.
pub async fn convert(engine: &Engine, input: &Path, output: &Path, force: bool) -> Result<()> {
    match engine.convert_ebook(input, output, force).await {
        Ok(path) => {
            println!("{} -> {}", input.display(), path.display());
            Ok(())
        }
        Err(e @ EngineError::CalibreMissing { .. }) => {
            println!("{e} — conversion needs it, and nothing else here does.");
            Ok(())
        }
        // An existing output is the one refusal with an obvious next move, so it
        // names the flag rather than only complaining.
        Err(EngineError::InvalidInput(m)) if m.contains("already exists") => {
            println!("{m}. overwrite it: --force");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Tier (ii): import the books calibre already holds.
pub async fn import(
    engine: &Engine,
    library: Option<PathBuf>,
    dry_run: bool,
    new: bool,
) -> Result<()> {
    let opts = ImportOptions {
        library,
        dry_run,
        create_ambiguous: new,
    };
    let report = match engine.import_calibre_library(&opts).await {
        Ok(r) => r,
        Err(e @ EngineError::CalibreMissing { .. }) => {
            println!("{e} — library import needs it.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    for b in &report.books {
        println!("{}", book_line(b, &report));
    }

    // Unmatched is a decision, not a dead end — the same shape `ko pull` and
    // `goodreads import` have: say what it probably is, and name both moves.
    for u in &report.unmatched {
        println!(
            "unmatched: {} — {}",
            u.title,
            if u.authors.is_empty() {
                "(unknown author)".to_string()
            } else {
                u.authors.join(", ")
            }
        );
        for c in &u.candidates {
            println!(
                "    maybe #{}: {} ({:.0}%)",
                c.book_id,
                c.title,
                c.score * 100.0
            );
        }
        println!("    it is one of those : readingbuddy merge <new> <kept>, after importing");
        println!("    or it is not       : readingbuddy calibre import --new");
    }

    println!();
    let library = opts
        .library
        .as_ref()
        .map(|p| format!(" --library {}", p.display()))
        .unwrap_or_default();
    if report.dry_run {
        println!(
            "{} books in calibre, {} would be created, {} left for you to decide about.",
            report.rows,
            report.created(),
            report.unmatched.len()
        );
        println!("  nothing was written. do it: readingbuddy calibre import{library}");
    } else {
        println!(
            "{} books in calibre, {} created, {} matched, {} left for you to decide about.",
            report.rows,
            report.created(),
            report.books.len() - report.created(),
            report.unmatched.len()
        );
    }
    Ok(())
}

fn book_line(b: &CalibreBookReport, report: &CalibreReport) -> String {
    let mut parts = Vec::new();
    if b.tags_added > 0 {
        parts.push(match b.tags_added {
            1 => "1 tag".to_string(),
            n => format!("{n} tags"),
        });
    }
    if b.cover {
        parts.push("cover".to_string());
    }
    if b.files_linked > 0 {
        parts.push(match b.files_linked {
            1 => "1 file identified".to_string(),
            n => format!("{n} files identified"),
        });
    }
    if parts.is_empty() {
        parts.push("nothing new".to_string());
    }
    let mode = if report.dry_run { " (dry run)" } else { "" };
    format!(
        "{}{mode}: {} (matched by {})",
        b.title,
        parts.join(", "),
        b.matched_by
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absence is a report, and it names nothing to go and install.
    ///
    /// The rule is `docs/decisions.md`'s, and it is the sort that erodes by
    /// helpfulness: "calibre is not installed — get it from calibre-ebook.com"
    /// is one obliging edit away and is precisely what this app does not do.
    #[test]
    fn an_absent_calibre_is_reported_and_never_prescribed() {
        let absent = availability(false, false);
        assert_eq!(
            absent,
            "calibre is not installed; these features are not available."
        );
        // Phrases, not the bare word "install" — the honest sentence *has* to
        // say "not installed". What is banned is the imperative that follows it.
        for prescription in [
            "install calibre",
            "install it",
            "download",
            "brew ",
            "http",
            ".com",
            "you need",
            "to enable",
        ] {
            assert!(
                !absent.to_lowercase().contains(prescription),
                "{absent:?} tells the user to go and configure other software ({prescription:?})"
            );
        }
        // A half install degrades to the half that works, rather than to
        // nothing — the two tiers are independent features.
        assert!(availability(true, false).starts_with("conversion is available"));
        assert!(availability(false, true).starts_with("library import is available"));
        assert_eq!(
            availability(true, true),
            "conversion and library import are available."
        );
    }
}
