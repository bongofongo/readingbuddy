//! In-band, user-facing degradation reports.
//!
//! A [`Diagnostic`] is what the engine returns when an operation *partly*
//! succeeded: one provider timed out but the other answered, one sidecar was
//! unreadable but the rest imported. It is distinct from `tracing`, and the two
//! are not substitutes — tracing is out-of-band and for the developer, a
//! diagnostic is part of the return value and for the user. Only a diagnostic
//! can put "openlibrary timed out, these results are partial" in a status bar.
//!
//! These used to be pre-formatted `String`s, which meant no caller could tell a
//! timeout from a 500 and no test could assert on the degradation path. The
//! [`std::fmt::Display`] impls below reproduce those old strings **byte for
//! byte**, so frontends that just print them needed no change.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::EngineError;
use crate::providers::ProviderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The operation continued and returned partial results.
    Warning,
    /// The operation could not proceed.
    Error,
}

/// A coarse, `Copy` classification of an [`EngineError`], for callers that want
/// to branch without matching the full error.
///
/// This exists instead of storing the `EngineError` itself: `EngineError` is
/// not `Clone` (neither `sqlx::Error` nor `reqwest::Error` is), and a
/// `Diagnostic` that cannot be cloned is one the TUI cannot buffer and the test
/// harness cannot compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Network,
    Timeout,
    /// A provider refused us for quota reasons — the keyless Google Books path
    /// hits this, and it is the one class that plausibly deserves its own UI
    /// text ("add an API key"), which is why it is not folded into `Network`.
    RateLimited,
    Decode,
    Parse,
    Io,
    Other,
}

impl From<&EngineError> for ErrorClass {
    fn from(e: &EngineError) -> Self {
        match e {
            EngineError::Timeout { .. } => ErrorClass::Timeout,
            EngineError::Http(h) => {
                if h.status().is_some_and(|s| s.as_u16() == 429) {
                    ErrorClass::RateLimited
                } else if h.is_timeout() {
                    ErrorClass::Timeout
                } else if h.is_decode() {
                    ErrorClass::Decode
                } else {
                    ErrorClass::Network
                }
            }
            EngineError::Json(_) => ErrorClass::Decode,
            EngineError::Sidecar(_) | EngineError::Epub(_) => ErrorClass::Parse,
            EngineError::Io(_) => ErrorClass::Io,
            EngineError::Provider { message, .. } => {
                // Provider errors arrive pre-rendered (scrubbed), so the status
                // has to be recovered from the text.
                if message.contains("429") || message.to_ascii_lowercase().contains("rate limit") {
                    ErrorClass::RateLimited
                } else {
                    ErrorClass::Network
                }
            }
            _ => ErrorClass::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    ProviderFailed {
        provider: ProviderId,
        class: ErrorClass,
    },
    ProviderTimedOut {
        provider: ProviderId,
        after: Duration,
    },
    /// The file could not be read at all (permissions, non-UTF-8 bytes).
    SidecarUnreadable {
        path: PathBuf,
        class: ErrorClass,
    },
    /// The file was read but is not a valid sidecar.
    SidecarUnparsable {
        path: PathBuf,
    },
    NoSidecarsFound {
        path: PathBuf,
    },
    /// The sidecar's `summary.status` was a value KOReader is not known to
    /// write. The import carries it through as `KoStatus::Other` and continues;
    /// this warning exists because it is otherwise the one thing that could
    /// tell us the device grew a status we do not model, and silence would look
    /// exactly like success.
    UnknownDeviceStatus {
        path: PathBuf,
        status: String,
    },
}

/// One degradation, carried in-band on a partly-successful result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub severity: Severity,
    /// Human-readable detail. **Always already scrubbed of API keys** — every
    /// constructor below goes through `googlebooks::scrub_key`.
    pub detail: String,
}

impl Diagnostic {
    pub fn provider_failed(provider: ProviderId, err: &EngineError) -> Self {
        Diagnostic {
            kind: DiagnosticKind::ProviderFailed {
                provider,
                class: ErrorClass::from(err),
            },
            severity: Severity::Warning,
            detail: crate::providers::googlebooks::scrub_key(&err.to_string()),
        }
    }

    pub fn provider_timed_out(provider: ProviderId, after: Duration) -> Self {
        Diagnostic {
            kind: DiagnosticKind::ProviderTimedOut { provider, after },
            severity: Severity::Warning,
            // "timed out" verbatim: this is what the pre-refactor string was,
            // and `search`'s CLI output is pinned to it.
            detail: "timed out".to_string(),
        }
    }

    pub fn sidecar_unreadable(path: PathBuf, err: &EngineError) -> Self {
        Diagnostic {
            kind: DiagnosticKind::SidecarUnreadable {
                path,
                class: ErrorClass::from(err),
            },
            severity: Severity::Warning,
            detail: err.to_string(),
        }
    }

    pub fn sidecar_unparsable(path: PathBuf, err: &EngineError) -> Self {
        Diagnostic {
            kind: DiagnosticKind::SidecarUnparsable { path },
            severity: Severity::Warning,
            detail: err.to_string(),
        }
    }

    pub fn no_sidecars_found(path: PathBuf) -> Self {
        Diagnostic {
            kind: DiagnosticKind::NoSidecarsFound { path },
            severity: Severity::Warning,
            detail: String::new(),
        }
    }

    pub fn unknown_device_status(path: PathBuf, status: &str) -> Self {
        Diagnostic {
            kind: DiagnosticKind::UnknownDeviceStatus {
                path,
                status: status.to_string(),
            },
            severity: Severity::Warning,
            detail: format!("unknown KOReader status {status:?}; imported as-is"),
        }
    }

    /// The provider this diagnostic is about, if any.
    pub fn provider(&self) -> Option<ProviderId> {
        match self.kind {
            DiagnosticKind::ProviderFailed { provider, .. }
            | DiagnosticKind::ProviderTimedOut { provider, .. } => Some(provider),
            _ => None,
        }
    }

    pub fn is_timeout(&self) -> bool {
        matches!(
            self.kind,
            DiagnosticKind::ProviderTimedOut { .. }
                | DiagnosticKind::ProviderFailed {
                    class: ErrorClass::Timeout,
                    ..
                }
        )
    }

    pub fn is_rate_limited(&self) -> bool {
        matches!(
            self.kind,
            DiagnosticKind::ProviderFailed {
                class: ErrorClass::RateLimited,
                ..
            }
        )
    }
}

impl fmt::Display for Diagnostic {
    /// Byte-for-byte identical to the strings this type replaced. Frontends
    /// print `warning: {d}` and were not touched by the refactor; a change here
    /// is a change to user-visible output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            DiagnosticKind::ProviderFailed { provider, .. }
            | DiagnosticKind::ProviderTimedOut { provider, .. } => {
                write!(f, "{provider}: {}", self.detail)
            }
            DiagnosticKind::SidecarUnreadable { path, .. }
            | DiagnosticKind::SidecarUnparsable { path }
            | DiagnosticKind::UnknownDeviceStatus { path, .. } => {
                write!(f, "{}: {}", path.display(), self.detail)
            }
            DiagnosticKind::NoSidecarsFound { path } => {
                write!(f, "no KOReader sidecars found under {}", path.display())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole compatibility bar for the `Vec<String>` -> `Vec<Diagnostic>`
    /// refactor: these are the exact strings the CLI used to print.
    #[test]
    fn display_matches_the_pre_refactor_strings() {
        let timed_out =
            Diagnostic::provider_timed_out(ProviderId::OpenLibrary, Duration::from_secs(5));
        assert_eq!(timed_out.to_string(), "openlibrary: timed out");

        let failed = Diagnostic::provider_failed(
            ProviderId::GoogleBooks,
            &EngineError::Other("boom".into()),
        );
        assert_eq!(failed.to_string(), "googlebooks: boom");

        let none = Diagnostic::no_sidecars_found(PathBuf::from("/books"));
        assert_eq!(none.to_string(), "no KOReader sidecars found under /books");

        let bad = Diagnostic::sidecar_unparsable(
            PathBuf::from("/books/x.sdr/metadata.epub.lua"),
            &EngineError::Sidecar("lua eval: nope".into()),
        );
        assert_eq!(
            bad.to_string(),
            "/books/x.sdr/metadata.epub.lua: koreader sidecar error: lua eval: nope"
        );
    }

    #[test]
    fn a_key_never_survives_into_a_diagnostic() {
        let leaky = EngineError::Other(
            "https://www.googleapis.com/books/v1/volumes?q=x&key=SUPERSECRET failed".into(),
        );
        let d = Diagnostic::provider_failed(ProviderId::GoogleBooks, &leaky);
        assert!(
            !d.detail.contains("SUPERSECRET"),
            "detail leaked: {}",
            d.detail
        );
        assert!(!d.to_string().contains("SUPERSECRET"));
    }

    #[test]
    fn classification_is_recoverable_from_the_kind() {
        let t = Diagnostic::provider_timed_out(ProviderId::OpenLibrary, Duration::from_secs(5));
        assert!(t.is_timeout());
        assert_eq!(t.provider(), Some(ProviderId::OpenLibrary));

        let r = Diagnostic::provider_failed(
            ProviderId::GoogleBooks,
            &EngineError::Provider {
                provider: ProviderId::GoogleBooks,
                message: "HTTP 429 rate limit".into(),
            },
        );
        assert!(r.is_rate_limited());
        assert!(!r.is_timeout());
    }
}
