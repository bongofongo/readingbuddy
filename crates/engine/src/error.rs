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
    #[error("koreader sidecar error: {0}")]
    Sidecar(String),
    #[error("not found: {0}")]
    NotFound(String),
    /// Last resort. Prefer a specific variant — anything that a caller might
    /// plausibly want to branch on does not belong here.
    #[error("{0}")]
    Other(String),
}
