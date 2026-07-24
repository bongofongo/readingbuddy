//! System clipboard access, used by the API-key box to paste a copied key.
//!
//! We read the OS clipboard directly (via `arboard`) rather than relying on the
//! terminal's bracketed-paste: it works the same on every terminal and lets the
//! key box bind a plain Ctrl+V.

use anyhow::{Context, Result};

/// The clipboard's current text, trimmed of surrounding whitespace/newlines.
pub fn read() -> Result<String> {
    let mut clipboard = arboard::Clipboard::new().context("open clipboard")?;
    let text = clipboard.get_text().context("read clipboard")?;
    Ok(text.trim().to_string())
}
