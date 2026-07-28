use std::path::Path;

use anyhow::{Result, bail};
use readingbuddy::{BookImportStats, Engine, MatchCandidate};

use super::resolve_one;

pub async fn import(engine: &Engine, path: &Path, dry_run: bool) -> Result<()> {
    let report = engine.import_koreader(path, dry_run).await?;
    let mode = if dry_run { " (dry run)" } else { "" };

    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    for s in &report.imported {
        println!("{}", stats_line(s, mode));
    }
    for u in &report.unmatched {
        println!(
            "unmatched{mode}: {} ({})",
            u.title.as_deref().unwrap_or("unknown title"),
            u.path.display()
        );
        // Unmatched is a decision, not a dead end. Say what the two moves are,
        // and — when the library holds something close — which book it probably
        // is, rather than leaving the user to find it.
        print_candidates(&u.candidates);
        println!("    pull it in : readingbuddy ko pull {}", u.path.display());
        println!(
            "    or link it : readingbuddy ko link {} <book>",
            u.path.display()
        );
    }
    if report.imported.is_empty() && report.unmatched.is_empty() && report.warnings.is_empty() {
        println!("nothing to import.");
    }
    Ok(())
}

/// Create the book from the sidecar's own metadata and import its highlights.
pub async fn pull(engine: &Engine, path: &Path, new: bool) -> Result<()> {
    // Look before creating. The whole reason `match_candidates` exists is that a
    // variant title used to become a silent duplicate, and creating first and
    // warning after would reproduce exactly that.
    if !new {
        let candidates = engine.sidecar_candidates(path).await?;
        if !candidates.is_empty() {
            println!("{} looks like a book you already have:", path.display());
            print_candidates(&candidates);
            println!(
                "    link it    : readingbuddy ko link {} <book>",
                path.display()
            );
            println!("    or pull it as a new book: --new");
            return Ok(());
        }
    }

    let report = engine.pull_book_from_sidecar(path).await?;
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    println!("{}", stats_line(&report.stats, ""));
    println!(
        "  #{} — no ISBN, cover or description yet; `readingbuddy search` then \
         `readingbuddy ko link` to enrich it",
        report.stats.book_id
    );
    Ok(())
}

/// Record that this sidecar is that book, then import into it.
pub async fn link(engine: &Engine, path: &Path, selector: &str) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let Some(book_id) = book.id else {
        bail!("'{selector}' resolved to an unsaved book");
    };
    let md5 = engine.link_sidecar(path, book_id).await?;
    println!(
        "linked {} to {} ({md5})",
        path.display(),
        book.display_title()
    );
    // The link only pays off on the next import, so run it now: otherwise
    // `link` looks like it did nothing.
    import(engine, path, false).await
}

fn stats_line(s: &BookImportStats, mode: &str) -> String {
    format!(
        "{}{mode}: {} new, {} updated from the device, {} already known, {} flashcard candidates \
         (matched by {})",
        s.book_title, s.inserted, s.updated, s.skipped, s.flashcards, s.matched_by
    )
}

fn print_candidates(candidates: &[MatchCandidate]) {
    for c in candidates {
        println!(
            "    maybe #{}: {} ({:.0}%)",
            c.book_id,
            c.title,
            c.score * 100.0
        );
    }
}
