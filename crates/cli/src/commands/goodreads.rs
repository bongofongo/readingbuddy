//! `goodreads import` / `goodreads export`.
//!
//! Goodreads' API died in December 2020; the CSV is the interface, both
//! directions. All the printing lives here — the engine does no terminal I/O.

use std::path::Path;

use anyhow::Result;
use readingbuddy::goodreads::ImportOptions;
use readingbuddy::{Engine, GoodreadsBookReport, GoodreadsReport, TextOutcome};

pub async fn import(engine: &Engine, path: &Path, dry_run: bool, new: bool) -> Result<()> {
    let report = engine
        .import_goodreads(
            path,
            ImportOptions {
                dry_run,
                create_ambiguous: new,
            },
        )
        .await?;

    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    for b in &report.books {
        println!("{}", book_line(b, &report));
    }

    // Unmatched is a decision, not a dead end: say what the row probably is, and
    // what the two moves are.
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
        println!(
            "    or it is not       : readingbuddy goodreads import {} --new",
            path.display()
        );
    }

    println!();
    if report.dry_run {
        // The same refusal-with-a-next-move shape `ko sync` has: show what would
        // happen and name the flag, rather than writing a whole shelf unasked.
        println!(
            "{} rows read, {} books would be created, {} left for you to decide about.",
            report.rows,
            report.created(),
            report.unmatched.len()
        );
        println!(
            "  nothing was written. do it: readingbuddy goodreads import {}",
            path.display()
        );
    } else {
        println!(
            "{} rows read, {} books created, {} matched, {} left for you to decide about.",
            report.rows,
            report.created(),
            report.books.len() - report.created(),
            report.unmatched.len()
        );
    }
    Ok(())
}

pub async fn export(engine: &Engine, out: &Path) -> Result<()> {
    let (csv, warnings) = engine.export_goodreads().await?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    let rows = readingbuddy::goodreads::row_count(&csv);
    if rows == 0 {
        println!("nothing to export — the library is empty.");
        return Ok(());
    }
    std::fs::write(out, csv)?;
    println!(
        "{rows} books -> {} (Goodreads: My Books > Import and export > Import)",
        out.display()
    );
    Ok(())
}

fn book_line(b: &GoodreadsBookReport, report: &GoodreadsReport) -> String {
    let mut parts = Vec::new();
    if b.readings_added > 0 {
        parts.push(match b.readings_added {
            1 => "1 reading".to_string(),
            n => format!("{n} readings"),
        });
    }
    if b.tags_added > 0 {
        parts.push(match b.tags_added {
            1 => "1 shelf".to_string(),
            n => format!("{n} shelves"),
        });
    }
    if let Some(r) = b.rating {
        parts.push(format!("rated {r}"));
    }
    match b.review {
        TextOutcome::Written => parts.push("review".to_string()),
        TextOutcome::KeptOurs => parts.push("review kept as yours".to_string()),
        _ => {}
    }
    match b.private_notes {
        TextOutcome::Written => parts.push("private notes".to_string()),
        TextOutcome::KeptOurs => parts.push("notes kept as yours".to_string()),
        _ => {}
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
