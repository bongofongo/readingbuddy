use std::path::Path;

use anyhow::Result;
use readingbuddy::Engine;

pub async fn import(engine: &Engine, path: &Path, dry_run: bool) -> Result<()> {
    let report = engine.import_koreader(path, dry_run).await?;
    let mode = if dry_run { " (dry run)" } else { "" };

    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    for s in &report.imported {
        println!(
            "{}{mode}: {} new, {} already known, {} flashcard candidates",
            s.book_title, s.inserted, s.skipped, s.flashcards
        );
    }
    for u in &report.unmatched {
        println!(
            "unmatched{mode}: {} ({}) — add the book first, then re-import",
            u.title.as_deref().unwrap_or("unknown title"),
            u.path.display()
        );
    }
    if report.imported.is_empty() && report.unmatched.is_empty() && report.warnings.is_empty() {
        println!("nothing to import.");
    }
    Ok(())
}
