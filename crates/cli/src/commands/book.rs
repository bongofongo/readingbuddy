use std::path::Path;

use anyhow::{Result, bail};
use readingbuddy::{BookSort, Engine};

use super::resolve_one;
use crate::{prompt, render};

pub async fn add_isbn(engine: &Engine, isbn: &str, no_cover: bool) -> Result<()> {
    let Some(mut book) = engine.lookup_isbn(isbn).await? else {
        bail!("no edition found for ISBN {isbn}");
    };
    if !no_cover && book.cover_url.is_some() {
        match engine.download_cover(&mut book).await {
            Ok(Some(p)) => println!("cover -> {}", p.display()),
            Ok(None) => {}
            Err(e) => eprintln!("cover download failed: {e}"),
        }
    }
    let saved = engine.save_book(&book).await?;
    println!("saved {}", render::book_line(&saved));
    Ok(())
}

pub async fn import_epub(engine: &Engine, path: &Path) -> Result<()> {
    let book = engine.import_epub(path).await?;
    println!("imported {}", render::book_line(&book));
    if book.page_count.is_none() {
        println!("note: no page count found — set one with `progress` math in mind");
    }
    Ok(())
}

pub async fn list(engine: &Engine, limit: i64, sort: &str) -> Result<()> {
    let sort = match sort {
        "title" => BookSort::Title,
        "progress" => BookSort::Progress,
        "last-modified" | "last_modified" => BookSort::LastModified,
        other => bail!("unknown sort '{other}' (last-modified | title | progress)"),
    };
    let books = engine.storage.list_books(limit, sort).await?;
    if books.is_empty() {
        println!("library is empty — try `readingbuddy search` or `readingbuddy epub`");
        return Ok(());
    }
    for b in &books {
        println!("{}", render::book_line(b));
    }
    Ok(())
}

pub async fn show(engine: &Engine, selector: &str) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    print!("{}", render::book_details(&book));
    let notes = engine.list_notes(book.id).await?;
    if !notes.is_empty() {
        println!("  {:<14} {}", "notes", notes.len());
    }
    if let Some(id) = book.id {
        let highlights = engine.storage.list_highlights(id).await?;
        if !highlights.is_empty() {
            println!("  {:<14} {}", "highlights", highlights.len());
        }
    }
    Ok(())
}

pub async fn remove(engine: &Engine, selector: &str, yes: bool) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let id = book.id.expect("stored book has id");
    if !yes && !prompt::confirm(&format!("remove {}?", render::book_line(&book)))? {
        println!("kept.");
        return Ok(());
    }
    engine.delete_book(id).await?;
    println!("removed {}", book.display_title());
    Ok(())
}

pub async fn progress(
    engine: &Engine,
    selector: &str,
    page: Option<i64>,
    finished: bool,
) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let id = book.id.expect("stored book has id");
    let updated = engine
        .storage
        .update_progress(id, page, finished.then_some(true))
        .await?;
    println!("{}", render::book_line(&updated));
    if finished {
        println!(
            "\n🎉 finished {}! congratulations.",
            updated.display_title()
        );
        println!("capture your final thoughts:  readingbuddy note --book {id} --kind final");
    }
    Ok(())
}

/// Fold one book into another. The duplicate the ISBN-less pull path
/// guarantees will eventually happen is what this is for.
pub async fn merge(
    engine: &Engine,
    src_selector: &str,
    dst_selector: &str,
    yes: bool,
) -> Result<()> {
    let src = resolve_one(engine, src_selector).await?;
    let dst = resolve_one(engine, dst_selector).await?;
    let (Some(src_id), Some(dst_id)) = (src.id, dst.id) else {
        bail!("both books must be saved before they can be merged");
    };
    if src_id == dst_id {
        println!("{} is already one book.", src.display_title());
        return Ok(());
    }

    if !yes
        && !prompt::confirm(&format!(
            "fold {} into {}? {} is deleted",
            render::book_line(&src),
            render::book_line(&dst),
            src.display_title()
        ))?
    {
        println!("cancelled.");
        return Ok(());
    }

    let r = engine.merge_books(src_id, dst_id).await?;
    println!("merged into {}", render::book_line(&dst));
    println!(
        "  {} highlights moved, {} dropped as duplicates",
        r.highlights_moved, r.highlights_dropped
    );
    println!(
        "  {} notes, {} flashcards moved ({} dropped), {} device links moved",
        r.notes_moved, r.flashcards_moved, r.flashcards_dropped, r.device_links_moved
    );
    Ok(())
}

pub async fn highlights(engine: &Engine, selector: &str) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let id = book.id.expect("stored book has id");
    let hs = engine.storage.list_highlights(id).await?;
    if hs.is_empty() {
        println!(
            "no highlights for {} — try `readingbuddy ko import`",
            book.display_title()
        );
        return Ok(());
    }
    println!("{} — {} highlights\n", book.display_title(), hs.len());
    for h in hs {
        let page = h.page.map(|p| format!("p.{p} ")).unwrap_or_default();
        let chapter = h.chapter.map(|c| format!("[{c}] ")).unwrap_or_default();
        println!("  {page}{chapter}“{}”", h.text);
        if let Some(note) = h.ko_note {
            println!("      ↳ {note}");
        }
        if let Some(annotation) = h.annotation {
            println!("      » {annotation}");
        }
    }
    Ok(())
}
