pub mod book;
pub mod cards;
pub mod config;
pub mod ko;
pub mod note;
pub mod rating;
pub mod reflect;
pub mod search;

use anyhow::{Result, bail};
use readingbuddy::{Book, Engine};

use crate::render;

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
