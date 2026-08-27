use std::time::Duration;

use crate::providers::ProviderId;

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("url error: {0}")]
    Url(#[from] url::ParseError),
    #[error("timestamp format: {0}")]
    Format(#[from] time::error::Format),
    /// A named provider failed. `message` is always already scrubbed of API
    /// keys — construct it via `googlebooks::scrubbed`, never by hand.
    #[error("{provider} error: {message}")]
    Provider {
        provider: ProviderId,
        message: String,
    },
    #[error("{what} timed out after {}s", .after.as_secs())]
    Timeout { what: String, after: Duration },
    #[error("invalid ISBN: {0:?}")]
    InvalidIsbn(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("epub error: {0}")]
    Epub(String),
    /// The bytes are not a PDF this engine can parse. Its own variant beside
    /// `Epub` for the same reason that one has one: `import_file` reads a
    /// file's metadata opportunistically and needs to tell "this format has
    /// nothing to say" apart from "the database is unreachable", and the two
    /// readers fail for entirely different causes.
    ///
    /// **A PDF that parses and will not give a length is not this.** That is
    /// `PdfInfo { page_count: None, .. }` — absence, which the caller stores as
    /// `NULL`. See `crates/engine/src/pdf.rs`.
    #[error("pdf error: {0}")]
    Pdf(String),
    #[error("koreader sidecar error: {0}")]
    Sidecar(String),
    /// Calibre is not installed, or not the half of it this feature needs.
    ///
    /// Its own variant because **absent is a first-class answer here**, not a
    /// failure: `docs/decisions.md` makes calibre feature-detected and never a
    /// hard dependency, so every caller branches on this to say "that feature
    /// is not here" rather than printing an error the user is meant to fix.
    #[error("calibre's `{tool}` is not installed")]
    CalibreMissing { tool: String },
    /// A calibre tool ran and failed. `message` is the tail of its stderr —
    /// calibre is chatty, and the interesting part is at the end.
    #[error("calibre's `{tool}` failed: {message}")]
    Calibre { tool: String, message: String },
    #[error("not found: {0}")]
    NotFound(String),
    /// A review is rated, but the user has never said what that value means on
    /// Goodreads' integer 0–5.
    ///
    /// Its own variant rather than `Other` because every caller branches on it:
    /// an export must skip the row and say why, and the CLI must name the
    /// command that fixes it. Rounding it silently is the one thing that is not
    /// allowed — formulas are always wrong at the ends, which is why the map is
    /// a lookup table in the first place.
    #[error(
        "{value} on the '{scale}' scale has no Goodreads mapping (Goodreads takes integers 0–5, no halves)"
    )]
    UnmappedRating { value: f64, scale: String },
    /// The platform has no filesystem-notification service to offer, or refused
    /// to watch the mount directories.
    ///
    /// Its own variant because the only sane response is to degrade: an app that
    /// cannot notice a mount is the app we had yesterday, and every device
    /// screen, `ko scan` and `ko sync` still works. A caller that treated this
    /// as a startup failure would be trading a whole app for a convenience.
    #[error("cannot watch for devices: {0}")]
    Watch(String),
    /// A write to a mounted reader that readingbuddy declined to make.
    ///
    /// Its own variant, and carrying an enum rather than a string, because
    /// every arm of [`crate::plugin::PluginRefusal`] is a line in
    /// `docs/decisions.md` that a caller is meant to act on differently: a
    /// wrong volume is a different conversation from a file the user edited,
    /// and a frontend cannot tell them apart by reading prose.
    // `transparent`, not `"{0}"`: `#[from]` makes the refusal this error's
    // *source*, so an identical Display renders it twice — the CLI printed
    // "Error: …edited on the device…" followed by "Caused by: …edited on the
    // device…", the same sentence in both halves of an anyhow report.
    #[error(transparent)]
    PluginRefused(#[from] crate::plugin::PluginRefusal),
    /// Last resort. Prefer a specific variant — anything that a caller might
    /// plausibly want to branch on does not belong here.
    #[error("{0}")]
    Other(String),
}
