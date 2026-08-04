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
    /// The sidecar carries no root `partial_md5_checksum`, so there is nothing
    /// to key a `device_books` mapping on.
    ///
    /// The pull still happens — refusing would be a dead end for a file the
    /// user explicitly pointed at — but it cannot be made idempotent: a second
    /// pull of the same sidecar creates a second book. Saying so is the whole
    /// point of the variant, because the duplicate appears silently and long
    /// after the fact.
    SidecarNotIdentified {
        path: PathBuf,
    },

    // ---- goodreads (migration `0009`) -------------------------------------
    //
    // Seven variants rather than one carrying a reason string, for the reason
    // the module doc gives: a degradation the caller cannot tell apart from
    // another degradation is a `String` wearing a type. Each of these has a
    // different next move, and the CLI prints a different one for each.
    /// A CSV row with nothing to match on (no `Title`). The rest of the file
    /// still imports — one bad row must not cost the other four hundred.
    GoodreadsRowSkipped {
        row: usize,
    },
    /// The CSV's review and the review already in the vault disagree. **Ours is
    /// kept**: the words came from Goodreads, but they live here now, and an
    /// import overwriting the user's own edit is the one outcome with no undo.
    GoodreadsReviewDiverged {
        title: String,
    },
    /// The review is already rated on a scale that is not `goodreads`, so the
    /// CSV's integer was not written. The map is many-to-one and its inverse is
    /// a guess, so the two numbers cannot even be compared.
    GoodreadsRatingDiverged {
        title: String,
    },
    /// A row carries a review but describes no reading to anchor it to (it is on
    /// `to-read`). The prose is kept as a plain note rather than opening a
    /// reading the user never started.
    GoodreadsUnanchoredReview {
        title: String,
    },
    /// `Read Count` asked for more readings than the row carries a date to close
    /// them at. A reading with no end date *is* an open one, and
    /// `idx_readings_one_open` permits exactly one per book.
    GoodreadsUndatableRereads {
        title: String,
        dropped: usize,
    },
    /// Export: this book's rating has no Goodreads mapping, so the row was
    /// skipped rather than rounded.
    GoodreadsRatingUnmapped {
        title: String,
    },
    /// Export: Goodreads' importable CSV has no read-count column, so a reread
    /// exports as its most recent reading only.
    GoodreadsRereadsDropped {
        title: String,
        dropped: usize,
    },

    // ---- koreader statistics (item 31) -------------------------------------
    //
    // Four variants rather than one, and the module doc's rule is the reason:
    // *absence is ordinary here*, so these are the ordinary path rather than an
    // exceptional one, and a caller that cannot tell "this device has never run
    // the statistics plugin" from "we do not understand this device's schema"
    // has no way to tell the user which of the two it is looking at.
    /// The volume has no `settings/statistics.sqlite3`. The commonest case by
    /// far — the plugin is optional and a fresh install has never written one —
    /// and emphatically not an error.
    StatisticsDbAbsent {
        path: PathBuf,
    },
    /// The file is there and SQLite would not read it: a partial copy off a
    /// yanked volume, a permissions problem, a file that is not a database.
    StatisticsDbUnreadable {
        path: PathBuf,
        class: ErrorClass,
    },
    /// `PRAGMA user_version` is a schema this build has never seen.
    ///
    /// **Refuses rather than guesses.** KOReader stamps the version precisely so
    /// a reader can tell; the columns we aggregate could have been renamed or
    /// re-scaled under us, and importing a wrong number of minutes is far worse
    /// than importing none, because nothing downstream can tell it apart from a
    /// right one.
    StatisticsSchemaUnknown {
        path: PathBuf,
        version: i64,
    },
    /// A `book` row whose `md5` matches no book in the library.
    ///
    /// Ordinary: the device holds books that were never imported here. Reported
    /// because it is also what a *broken join* looks like, and the two are
    /// indistinguishable in silence.
    StatisticsBookUnmatched {
        md5: String,
    },
    /// A `book` row carrying no `md5` at all, so there is nothing to join on.
    /// The sibling of [`DiagnosticKind::SidecarNotIdentified`].
    StatisticsBookNotIdentified {
        title: String,
    },

    // ---- calibre (item 13) -------------------------------------------------
    /// A calibre row with no title, so there is nothing to match on. The rest
    /// of the library still imports.
    CalibreRowSkipped {
        calibre_id: i64,
    },
    /// A calibre row with no `uuid`. It still imports — refusing would be a
    /// dead end for a book the user plainly has — but it cannot be made
    /// idempotent, and a second run creates a second copy. The sibling of
    /// [`DiagnosticKind::SidecarNotIdentified`], and it exists for the same
    /// reason: the duplicate appears silently and long after the fact.
    CalibreRowNotIdentified {
        calibre_id: i64,
    },
    /// Calibre named a cover we could not copy — its library may live on a
    /// volume that is not mounted. The book still imports without one.
    CalibreCoverUnreadable {
        path: PathBuf,
    },

    // ---- enrichment (item 30) ---------------------------------------------
    /// A provider gave us a cover URL that would not download. The metadata it
    /// came with landed and is kept — the cover is the one field with a free
    /// second chance, since the next enrichment of this book tries again.
    ///
    /// **The URL is not carried.** It is of no use to a user, and the rule that
    /// nothing reaching a diagnostic may contain an API key is easier to keep
    /// than to check.
    CoverUnavailable {
        class: ErrorClass,
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

    pub fn sidecar_not_identified(path: PathBuf) -> Self {
        Diagnostic {
            kind: DiagnosticKind::SidecarNotIdentified { path },
            severity: Severity::Warning,
            detail: "no partial_md5_checksum; this book will be created again on a second pull"
                .to_string(),
        }
    }

    // ---- koreader statistics (item 31) -------------------------------------

    /// No statistics database on this volume. A `Warning`, never an error: the
    /// statistics plugin is optional, and *a device with no measured time is a
    /// device with no measured time*, not a failure.
    pub fn statistics_db_absent(path: PathBuf) -> Self {
        Diagnostic {
            kind: DiagnosticKind::StatisticsDbAbsent { path },
            severity: Severity::Warning,
            detail: "no KOReader statistics database; no reading time to import".to_string(),
        }
    }

    pub fn statistics_db_unreadable(path: PathBuf, err: &EngineError) -> Self {
        Diagnostic {
            kind: DiagnosticKind::StatisticsDbUnreadable {
                path,
                class: ErrorClass::from(err),
            },
            severity: Severity::Warning,
            detail: err.to_string(),
        }
    }

    pub fn statistics_schema_unknown(path: PathBuf, version: i64) -> Self {
        Diagnostic {
            kind: DiagnosticKind::StatisticsSchemaUnknown { path, version },
            severity: Severity::Warning,
            detail: format!(
                "statistics schema {version} is not one this build understands \
                 (expected {}); nothing imported",
                crate::ko_statistics::KNOWN_SCHEMA_VERSION
            ),
        }
    }

    pub fn statistics_book_unmatched(md5: &str) -> Self {
        Diagnostic {
            kind: DiagnosticKind::StatisticsBookUnmatched {
                md5: md5.to_string(),
            },
            severity: Severity::Warning,
            detail: "no book in this library has that file".to_string(),
        }
    }

    pub fn statistics_book_not_identified(title: &str) -> Self {
        Diagnostic {
            kind: DiagnosticKind::StatisticsBookNotIdentified {
                title: title.to_string(),
            },
            severity: Severity::Warning,
            detail: "no md5 on the statistics row; nothing to join on".to_string(),
        }
    }

    pub fn cover_unavailable(err: &EngineError) -> Self {
        Diagnostic {
            kind: DiagnosticKind::CoverUnavailable {
                class: ErrorClass::from(err),
            },
            severity: Severity::Warning,
            detail: crate::providers::googlebooks::scrub_key(&err.to_string()),
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
            | DiagnosticKind::UnknownDeviceStatus { path, .. }
            | DiagnosticKind::SidecarNotIdentified { path } => {
                write!(f, "{}: {}", path.display(), self.detail)
            }
            DiagnosticKind::NoSidecarsFound { path } => {
                write!(f, "no KOReader sidecars found under {}", path.display())
            }
            DiagnosticKind::GoodreadsRowSkipped { row } => {
                write!(f, "row {row}: {}", self.detail)
            }
            DiagnosticKind::GoodreadsReviewDiverged { title }
            | DiagnosticKind::GoodreadsRatingDiverged { title }
            | DiagnosticKind::GoodreadsUnanchoredReview { title }
            | DiagnosticKind::GoodreadsUndatableRereads { title, .. }
            | DiagnosticKind::GoodreadsRatingUnmapped { title }
            | DiagnosticKind::GoodreadsRereadsDropped { title, .. } => {
                write!(f, "{title}: {}", self.detail)
            }
            DiagnosticKind::StatisticsDbAbsent { path }
            | DiagnosticKind::StatisticsDbUnreadable { path, .. }
            | DiagnosticKind::StatisticsSchemaUnknown { path, .. } => {
                write!(f, "{}: {}", path.display(), self.detail)
            }
            DiagnosticKind::StatisticsBookUnmatched { md5 } => {
                write!(f, "{md5}: {}", self.detail)
            }
            DiagnosticKind::StatisticsBookNotIdentified { title } => {
                write!(f, "{title}: {}", self.detail)
            }
            DiagnosticKind::CalibreRowSkipped { calibre_id }
            | DiagnosticKind::CalibreRowNotIdentified { calibre_id } => {
                write!(f, "calibre #{calibre_id}: {}", self.detail)
            }
            DiagnosticKind::CalibreCoverUnreadable { path } => {
                write!(f, "{}: {}", path.display(), self.detail)
            }
            DiagnosticKind::CoverUnavailable { .. } => {
                write!(f, "cover: {}", self.detail)
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
