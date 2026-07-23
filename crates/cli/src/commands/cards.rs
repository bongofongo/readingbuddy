use std::path::Path;

use anyhow::Result;
use readingbuddy::Engine;

pub async fn list(engine: &Engine, all: bool) -> Result<()> {
    let cards = engine.list_flashcards(all).await?;
    if cards.is_empty() {
        println!("no flashcard candidates — single-word KOReader highlights land here.");
        return Ok(());
    }
    for c in cards {
        let mark = if c.exported { "✓" } else { " " };
        let ctx = c.context.as_deref().unwrap_or("");
        println!("{mark} #{:<4} {:<24} {ctx}  [{}]", c.id, c.word, c.book_title);
    }
    Ok(())
}

pub async fn export(engine: &Engine, out: &Path, all: bool) -> Result<()> {
    let (tsv, count) = engine.export_flashcards(all).await?;
    if count == 0 {
        println!("nothing to export.");
        return Ok(());
    }
    std::fs::write(out, tsv)?;
    println!("{count} cards -> {} (Anki: File > Import)", out.display());
    Ok(())
}
