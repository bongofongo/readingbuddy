use anyhow::Result;
use readingbuddy::{Engine, NewNoteInput, NoteKind};

use super::resolve_one;
use crate::prompt;

pub async fn create(
    engine: &Engine,
    book_selector: Option<&str>,
    text: Option<String>,
    kind: &str,
    title: Option<String>,
) -> Result<()> {
    let kind: NoteKind = kind.parse()?;
    let book = match book_selector {
        Some(sel) => Some(resolve_one(engine, sel).await?),
        None => None,
    };
    let body = match text {
        Some(t) if !t.trim().is_empty() => t,
        _ => prompt::edit_in_editor("")?,
    };
    if body.trim().is_empty() {
        println!("empty note, nothing saved.");
        return Ok(());
    }

    let created = engine
        .create_note(NewNoteInput {
            book_id: book.as_ref().and_then(|b| b.id),
            highlight_id: None,
            kind,
            title,
            body,
        })
        .await?;
    println!("note #{} “{}” -> {}", created.id, created.title, created.file.display());
    if !created.links.is_empty() {
        println!("links: {}", created.links.join(", "));
    }
    Ok(())
}

pub async fn list_or_search(
    engine: &Engine,
    book_selector: Option<&str>,
    query: Option<&str>,
) -> Result<()> {
    if let Some(q) = query {
        let hits = engine.search_notes(q, 25).await?;
        if hits.is_empty() {
            println!("no notes match '{q}'");
            return Ok(());
        }
        for h in hits {
            println!("#{:<4} {}  ({})", h.note.id, h.note.title, h.note.file_path);
            println!("      {}", h.snippet);
        }
        return Ok(());
    }

    let book = match book_selector {
        Some(sel) => Some(resolve_one(engine, sel).await?),
        None => None,
    };
    let notes = engine.list_notes(book.as_ref().and_then(|b| b.id)).await?;
    if notes.is_empty() {
        println!("no notes yet — `readingbuddy note \"first thought\"`");
        return Ok(());
    }
    for n in notes {
        let kind = if n.kind == "note" { String::new() } else { format!(" [{}]", n.kind) };
        println!("#{:<4} {}{kind}  ({})", n.id, n.title, n.file_path);
    }
    Ok(())
}
