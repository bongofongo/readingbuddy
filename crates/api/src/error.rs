//! The stable error taxonomy.
//!
//! [`EngineError`] cannot cross a transport and should not try: it wraps
//! `sqlx::Error`, `reqwest::Error` and `io::Error`, none of which is
//! `Serialize`, and two of which are not even `Clone`. It is also not a
//! contract — a variant may be split or renamed whenever the engine finds a
//! reason to, and a client pinned to its shape would break on a refactor that
//! changed no behaviour.
//!
//! So [`ApiError`] is a **code plus a message**, and the code is the promise.
//! The pattern is `Diagnostic`'s, exactly as `docs/spec-11-16.md` says: a typed,
//! `Copy`, cloneable classification beside a human string, with no source error
//! inside it. The difference is only which way the operation went — a
//! `Diagnostic` rides along on a partial success, an `ApiError` is a failure.

use serde::{Deserialize, Serialize};

use readingbuddy::{EngineError, ErrorClass};

/// What went wrong, coarsely enough to branch on and stably enough to promise.
///
/// **Codes are appended, never renamed or repurposed.** A client that does not
/// know a code must treat it as [`ErrorCode::Internal`] — which is what
/// `#[serde(other)]` on that variant makes happen for free rather than by
/// asking every client to remember.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Nothing by that id, isbn, path or selector.
    NotFound,
    /// The caller asked for something the data cannot represent.
    InvalidInput,
    /// The ISBN failed its checksum, or is not ten or thirteen digits.
    InvalidIsbn,
    /// A review's rating has no entry in the Goodreads lookup table. Its own
    /// code because every caller branches on it: the fix is to add a mapping,
    /// and rounding is exactly what the table exists to refuse.
    UnmappedRating,
    /// Calibre, or the half of it this feature needs, is not installed. **Not a
    /// failure to report as one**: `docs/decisions.md` makes calibre
    /// feature-detected, so a client shows "that feature is not here" and never
    /// tells the user to install anything.
    CalibreMissing,
    /// A calibre tool ran and failed.
    CalibreFailed,
    /// A metadata provider refused or failed.
    Provider,
    /// A provider took too long.
    Timeout,
    /// A provider refused us for quota reasons. Distinct from `Provider` for
    /// the reason [`ErrorClass::RateLimited`] is: it is the one class with its
    /// own sensible UI text.
    RateLimited,
    /// The network was the problem.
    Network,
    /// A file could not be read or written.
    Io,
    /// An epub or a KOReader sidecar would not parse.
    Parse,
    /// A response would not decode.
    Decode,
    /// The database refused.
    Database,
    /// The platform has no filesystem-notification service to offer.
    Watch,
    /// The request itself was malformed — bad JSON, or a method this build does
    /// not have. Produced by the transport, never by the engine.
    BadRequest,
    /// Anything else, and anything a client's build is too old to name.
    #[serde(other)]
    Internal,
}

/// A failure, as it crosses the seam.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    /// The engine's own `Display`, which is what a frontend already prints.
    /// Always already scrubbed of API keys — every provider error path goes
    /// through `googlebooks::scrub_key` before it becomes an `EngineError`.
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError::new(ErrorCode::BadRequest, message)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

impl From<EngineError> for ApiError {
    fn from(e: EngineError) -> Self {
        ApiError {
            code: code_for(&e),
            message: e.to_string(),
        }
    }
}

/// Classify. The network-ish half defers to [`ErrorClass`] rather than
/// re-deciding what a 429 is — one definition of "rate limited", shared with
/// every `Diagnostic` the engine emits.
fn code_for(e: &EngineError) -> ErrorCode {
    match e {
        EngineError::NotFound(_) => ErrorCode::NotFound,
        EngineError::InvalidInput(_) => ErrorCode::InvalidInput,
        EngineError::InvalidIsbn(_) => ErrorCode::InvalidIsbn,
        EngineError::UnmappedRating { .. } => ErrorCode::UnmappedRating,
        EngineError::CalibreMissing { .. } => ErrorCode::CalibreMissing,
        EngineError::Calibre { .. } => ErrorCode::CalibreFailed,
        EngineError::Db(_) | EngineError::Migrate(_) => ErrorCode::Database,
        EngineError::Io(_) => ErrorCode::Io,
        EngineError::Epub(_) | EngineError::Pdf(_) | EngineError::Sidecar(_) => ErrorCode::Parse,
        EngineError::Watch(_) => ErrorCode::Watch,
        EngineError::Timeout { .. } => ErrorCode::Timeout,
        EngineError::Provider { .. } | EngineError::Http(_) => match ErrorClass::from(e) {
            ErrorClass::RateLimited => ErrorCode::RateLimited,
            ErrorClass::Timeout => ErrorCode::Timeout,
            ErrorClass::Decode => ErrorCode::Decode,
            ErrorClass::Network => ErrorCode::Network,
            _ => ErrorCode::Provider,
        },
        EngineError::Json(_) => ErrorCode::Decode,
        EngineError::Url(_) | EngineError::Format(_) | EngineError::Other(_) => ErrorCode::Internal,
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variants_a_caller_branches_on_get_their_own_code() {
        assert_eq!(
            ApiError::from(EngineError::NotFound("book id 3".into())).code,
            ErrorCode::NotFound
        );
        assert_eq!(
            ApiError::from(EngineError::CalibreMissing {
                tool: "calibredb".into()
            })
            .code,
            ErrorCode::CalibreMissing
        );
        assert_eq!(
            ApiError::from(EngineError::UnmappedRating {
                value: 4.5,
                scale: "default".into()
            })
            .code,
            ErrorCode::UnmappedRating
        );
    }

    #[test]
    fn the_message_is_the_engines_own_display() {
        let e = EngineError::InvalidIsbn("978-oops".into());
        let want = e.to_string();
        assert_eq!(ApiError::from(e).message, want);
    }

    /// A rate-limited provider must not read as a plain network failure: it is
    /// the one class with its own next move ("add an API key"), which is why
    /// `ErrorClass` keeps it separate and why this defers to it.
    #[test]
    fn rate_limiting_survives_the_translation() {
        let e = EngineError::Provider {
            provider: readingbuddy::ProviderId::GoogleBooks,
            message: "HTTP 429 rate limit".into(),
        };
        assert_eq!(ApiError::from(e).code, ErrorCode::RateLimited);
    }

    /// The forward-compatibility promise, as a test rather than a comment: a
    /// client built before a code existed must degrade to `Internal`, not fail
    /// to parse the reply and lose the error entirely.
    #[test]
    fn an_unknown_code_degrades_instead_of_failing_to_parse() {
        let raw = r#"{"code":"invented_in_a_later_version","message":"boom"}"#;
        let e: ApiError = serde_json::from_str(raw).unwrap();
        assert_eq!(e.code, ErrorCode::Internal);
        assert_eq!(e.message, "boom");
    }

    #[test]
    fn a_key_never_survives_into_an_api_error() {
        // The scrubbing happens upstream, in the provider. What this pins is
        // that translating an error does not undo it by reaching for some
        // rawer form of the message.
        let e = EngineError::Provider {
            provider: readingbuddy::ProviderId::GoogleBooks,
            message: readingbuddy::providers::googlebooks::scrub_key(
                "https://www.googleapis.com/books/v1/volumes?q=x&key=SUPERSECRET failed",
            ),
        };
        assert!(!ApiError::from(e).message.contains("SUPERSECRET"));
    }
}
