use anyhow::Result;
use readingbuddy::{Engine, NewNoteInput, NoteKind};

use super::{resolve_note, resolve_one};
use crate::prompt;

pub struct NoteOpts<'a> {
    pub book_selector: Option<&'a str>,
    pub text: Option<String>,
    pub kind: &'a str,
    pub title: Option<String>,
    pub page: Option<i64>,
    pub no_page: bool,
    pub location: Option<String>,
    pub highlight: Option<i64>,
}

pub async fn create(engine: &Engine, opts: NoteOpts<'_>) -> Result<()> {
    let kind: NoteKind = opts.kind.parse()?;
    // A reflection and a review anchor to a *reading*, and there is one of each
    // per reading — which is an index, and which this path knows nothing about.
    // Naming the command that does is the whole of the redirect.
    if kind.is_anchored() {
        let sel = opts.book_selector.unwrap_or("<book>");
        anyhow::bail!(
            "a {} is opened per reading: `readingbuddy {} {sel}`",
            kind.as_str(),
            kind.as_str().replace("reflection", "reflect")
        );
    }
    let book = match opts.book_selector {
        Some(sel) => Some(resolve_one(engine, sel).await?),
        None => None,
    };

    // Auto-anchor to where the reader is: when a book is named and no explicit
    // --page is given (and the user hasn't opted out), fall back to its current
    // reading page. Explicit --page always wins.
    let page = match (opts.page, opts.no_page) {
        (Some(p), _) => Some(p),
        (None, true) => None,
        (None, false) => book.as_ref().and_then(|b| b.current_page),
    };

    let body = match opts.text {
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
            reading_id: None,
            highlight_id: opts.highlight,
            page,
            location: opts.location,
            kind,
            title: opts.title,
            body,
        })
        .await?;
    println!(
        "note #{} “{}” -> {}",
        created.id,
        created.title,
        created.file.display()
    );
    if !created.links.is_empty() {
        println!("links: {}", created.links.join(", "));
    }
    Ok(())
}

/// Bring the note index in line with the vault before reading it.
///
/// The CLI is the one frontend that **cannot** hold a watcher: every command is
/// its own process, so there is no loop for one to live in. A sweep before the
/// two commands that read the *index* — rather than the rows — is the whole of
/// its answer, and it is a `stat` per note on the common path.
///
/// Never fatal. A vault that cannot be swept is a reason to search the index we
/// have, not a reason to refuse to search at all — an unreadable file must not
/// turn `notes -s` into an error.
async fn catch_up(engine: &Engine) {
    if let Err(e) = engine.reconcile_vault().await {
        tracing::warn!(error = %e, "could not reconcile the vault");
    }
}

pub async fn list_or_search(
    engine: &Engine,
    book_selector: Option<&str>,
    query: Option<&str>,
) -> Result<()> {
    if let Some(q) = query {
        catch_up(engine).await;
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
    let notes = engine
        .list_notes(book.as_ref().and_then(|b| b.id), None)
        .await?;
    if notes.is_empty() {
        println!("no notes yet — `readingbuddy note \"first thought\"`");
        return Ok(());
    }
    for n in notes {
        let kind = if n.kind == "note" {
            String::new()
        } else {
            format!(" [{}]", n.kind)
        };
        println!(
            "#{:<4} {}{kind}{}  ({})",
            n.id,
            n.title,
            anchor(&n),
            n.file_path
        );
    }
    Ok(())
}

/// What one note links to, and what links back to it.
///
/// Both directions in one view, because a backlink is only interesting beside
/// the link that made it — and dangling targets are printed as the text they
/// are rather than dropped. A `[[wikilink]]` naming a note nobody has written
/// yet is an ordinary forward reference; hiding it would turn "the note I
/// haven't written" into a dead end, which is the one thing this app is not
/// allowed to be.
pub async fn links(engine: &Engine, selector: &str) -> Result<()> {
    // The graph is written from the file too — a `[[wikilink]]` added in
    // Obsidian is an edge that only exists once somebody has re-read the note.
    catch_up(engine).await;
    let note = resolve_note(engine, selector).await?;
    println!("#{} “{}”  ({})", note.id, note.title, note.file_path);

    println!("links out:");
    let outgoing = engine.outgoing_links(note.id).await?;
    if outgoing.is_empty() {
        println!("  (none — a [[wikilink]] in the body makes one)");
    }
    for link in outgoing {
        match link.to {
            Some(t) => println!("  → #{:<4} “{}”", t.id, t.title),
            // Text, not an error: it resolves itself the moment that note is
            // written, and until then it is a note worth writing.
            None => println!(
                "  → “{}”  (text — no note by that title yet)",
                link.target_title
            ),
        }
    }

    println!("links in:");
    let inbound = engine.backlinks(note.id).await?;
    if inbound.is_empty() {
        println!("  (nothing links here yet)");
    }
    for n in inbound {
        println!("  ← #{:<4} “{}”", n.id, n.title);
    }
    Ok(())
}

/// Compact anchor tag for a note line: page, location, and/or highlight.
fn anchor(n: &readingbuddy::NoteRecord) -> String {
    let mut parts = Vec::new();
    if let Some(p) = n.page {
        parts.push(format!("p.{p}"));
    }
    if let Some(l) = &n.location {
        parts.push(l.clone());
    }
    if let Some(h) = n.highlight_id {
        parts.push(format!("↳hl#{h}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" @{}", parts.join(", "))
    }
}
