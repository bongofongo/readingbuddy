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
    let books = engine.list_books(limit, sort).await?;
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
        let highlights = engine.list_highlights(id).await?;
        if !highlights.is_empty() {
            println!("  {:<14} {}", "highlights", highlights.len());
        }
        // Rereads are first-class, so the history is a list rather than a count.
        let readings = engine.list_readings(id).await?;
        for (i, r) in readings.iter().enumerate() {
            println!("  {}", render::reading_line(r, i + 1, readings.len()));
        }
    }
    Ok(())
}

/// The book's own chapter list, read out of the epub on every run.
///
/// **Three answers, not two**, and the middle one is why this command exists in
/// the shape it does: no file we can read, a file that carries no navigable TOC,
/// and the list. `docs/decisions.md` bans a dead end, and "nothing here" printed
/// for both of the first two would make an ordinary EPUB3 book — whose `nav`
/// document `epub =2.1.4` cannot read — look identical to a book with no file at
/// all, with no move to offer for either.
pub async fn toc(engine: &Engine, selector: &str) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let id = book.id.expect("a stored book has an id");
    let title = book.display_title();

    let Some(toc) = engine.table_of_contents(id).await? else {
        println!("{title}: no epub here to read a chapter list from.");
        // Naming the command rather than "import one first": the move is a
        // file, and `rb epub` is what takes one.
        println!("    readingbuddy epub <path>");
        return Ok(());
    };
    if toc.entries.is_empty() {
        // Not an error and not a gap in the library — plenty of files carry no
        // `toc.ncx` at all. Saying which file was read makes it checkable.
        println!("{title}: this epub carries no table of contents.");
        println!("    read from {}", short_sha(&toc.sha256));
        return Ok(());
    }

    println!("{title} — {} chapters", toc.entries.len());
    for e in &toc.entries {
        // Depth is a column rather than a tree, so the indent is arithmetic.
        // Two spaces per level on top of the two every detail line carries.
        println!("  {}{}", "  ".repeat(e.depth), e.label);
    }
    println!("    read from {}", short_sha(&toc.sha256));
    Ok(())
}

/// Enough of a content address to check one against another, and not the
/// sixty-four characters that would wrap in every pane.
fn short_sha(sha256: &str) -> String {
    sha256.chars().take(12).collect()
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
    reread: bool,
) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let id = book.id.expect("stored book has id");
    if reread {
        if finished {
            bail!("--reread and --finished contradict each other");
        }
        engine.reread(id).await?;
    }
    let updated = engine
        .update_progress(id, page, finished.then_some(true))
        .await?;

    println!("{}", render::book_line(&updated));
    // Name the reading that was touched, not just the book: with rereads a book
    // carries several, and "progress → p.30" against the wrong one is a silent
    // mistake.
    let readings = engine.list_readings(id).await?;
    let touched = engine.active_reading(id).await?;
    let touched = touched.as_ref().or(readings.last());
    if let Some(r) = touched
        && let Some(nth) = readings.iter().position(|x| x.id == r.id)
    {
        println!("  {}", render::reading_line(r, nth + 1, readings.len()));
    }
    if finished {
        println!(
            "\n🎉 finished {}! congratulations.",
            updated.display_title()
        );
        // Not a task to complete — a place to go. The reflection may well
        // already be open and half-written; `reflect` reopens it either way.
        println!("your reflection is here when you want it:  readingbuddy reflect {id}");
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
    println!("  {} readings moved", r.readings_moved);
    Ok(())
}

pub async fn highlights(engine: &Engine, selector: &str) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let id = book.id.expect("stored book has id");
    let hs = engine.list_highlights(id).await?;
    if hs.is_empty() {
        println!(
            "no highlights for {} — try `readingbuddy ko import`",
            book.display_title()
        );
        return Ok(());
    }
    println!("{} — {} highlights\n", book.display_title(), hs.len());

    // Rereads are first-class, so the list is grouped by the read each
    // highlight was captured during — but only when there is more than one read
    // to tell apart. One reading is the ordinary case, and a header over the
    // whole list would be noise that says nothing.
    let readings = engine.list_readings(id).await?;
    if readings.len() < 2 {
        for h in &hs {
            print_highlight(h, "  ");
        }
        return Ok(());
    }

    for (i, r) in readings.iter().enumerate() {
        println!("  {}", render::reading_line(r, i + 1, readings.len()));
        let mine: Vec<_> = hs.iter().filter(|h| h.reading_id == Some(r.id)).collect();
        // Printed even when empty, and that is the informative case: a read
        // that genuinely marked nothing is a different fact from a read this
        // list forgot to mention.
        if mine.is_empty() {
            println!("    (none)");
        }
        for h in mine {
            print_highlight(h, "    ");
        }
        println!();
    }

    // `reading_id` is NULL where no reading's window held the capture date.
    // KOReader's sidecar is per-file and a reread appends to it, so the device
    // cannot supply this — these are ordinary highlights, not a failure and not
    // a queue of work, so they are listed plainly and last.
    let unplaced: Vec<_> = hs.iter().filter(|h| h.reading_id.is_none()).collect();
    if !unplaced.is_empty() {
        println!("  not placed in a reading ({})", unplaced.len());
        for h in unplaced {
            print_highlight(h, "    ");
        }
    }
    Ok(())
}

fn print_highlight(h: &readingbuddy::Highlight, indent: &str) {
    let page = h.page.map(|p| format!("p.{p} ")).unwrap_or_default();
    let chapter = h
        .chapter
        .as_deref()
        .map(|c| format!("[{c}] "))
        .unwrap_or_default();
    println!("{indent}{page}{chapter}“{}”", h.text);
    if let Some(note) = &h.ko_note {
        println!("{indent}    ↳ {note}");
    }
    if let Some(annotation) = &h.annotation {
        println!("{indent}    » {annotation}");
    }
}
