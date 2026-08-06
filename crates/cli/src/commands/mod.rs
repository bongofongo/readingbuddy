pub mod activity;
pub mod book;
pub mod calibre;
pub mod cards;
pub mod config;
pub mod covers;
pub mod enrich;
pub mod find;
pub mod goodreads;
pub mod ko;
pub mod note;
pub mod rating;
pub mod reflect;
pub mod search;
pub mod sort_keys;

use anyhow::{Result, bail};
use readingbuddy::{Book, Engine, NoteRecord, NoteScope};

use crate::render;

/// Bring the note index in line with the vault before reading it.
///
/// The CLI is the one frontend that **cannot** hold a watcher: every command is
/// its own process, so there is no loop for one to live in. A sweep before the
/// commands that read the *index* — rather than the rows — is the whole of its
/// answer, and it is a `stat` per note on the common path.
///
/// **Never fatal.** A vault that cannot be swept is a reason to search the
/// index we have, not a reason to refuse to search at all — an unreadable file
/// must not turn `find` or `notes -s` into an error.
///
/// One definition, shared by every command that reads the index or the graph:
/// a note edited in Obsidian is invisible to all of them equally, and two
/// copies of this would eventually catch up in only one place.
pub(crate) async fn catch_up(engine: &Engine) {
    if let Err(e) = engine.reconcile_vault().await {
        tracing::warn!(error = %e, "could not reconcile the vault");
    }
}

/// Resolve a selector (id | ISBN | title fragment) to exactly one book,
/// with a friendly error listing candidates when ambiguous.
pub async fn resolve_one(engine: &Engine, selector: &str) -> Result<Book> {
    let mut candidates = engine.resolve_books(selector).await?;
    match candidates.len() {
        0 => bail!("no book matches '{selector}' — try `readingbuddy list`"),
        1 => Ok(candidates.remove(0)),
        _ => {
            let mut msg = format!("'{selector}' is ambiguous:\n");
            for b in &candidates {
                msg.push_str(&format!("  {}\n", render::book_line(b)));
            }
            msg.push_str("use the #id instead");
            bail!(msg)
        }
    }
}

/// Resolve a selector (id | title fragment) to exactly one note.
///
/// The book selectors accept a fragment as well as an id, and a note's title is
/// the thing a `[[wikilink]]` names — so `links "Reflection: Pachinko"` has to
/// work, or the one command that reads the graph would be the one command that
/// makes you look an id up first.
pub async fn resolve_note(engine: &Engine, selector: &str) -> Result<NoteRecord> {
    if let Ok(id) = selector.parse::<i64>()
        && let Some(note) = engine.get_note(id).await?
    {
        return Ok(note);
    }
    let needle = selector.to_lowercase();
    let mut candidates: Vec<NoteRecord> = engine
        .list_notes(NoteScope::All, None)
        .await?
        .into_iter()
        .filter(|n| n.title.to_lowercase().contains(&needle))
        .collect();
    match candidates.len() {
        0 => bail!("no note matches '{selector}' — try `readingbuddy notes`"),
        1 => Ok(candidates.remove(0)),
        _ => {
            let mut msg = format!("'{selector}' is ambiguous:\n");
            for n in &candidates {
                msg.push_str(&format!("  #{:<4} {}\n", n.id, n.title));
            }
            msg.push_str("use the #id instead");
            bail!(msg)
        }
    }
}
