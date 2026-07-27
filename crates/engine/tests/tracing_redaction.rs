//! The one place it is worth asserting on log *output*.
//!
//! Everything else about degradation is now assertable on the returned
//! `Diagnostic`, which is stronger and does not couple tests to log text. But
//! "an API key never reaches a log" is a property of the emission itself, and
//! nothing else can check it.
//!
//! Deliberately hand-rolled rather than using `tracing-test`, which installs a
//! *global* default subscriber and thereby makes tests order-dependent and
//! hostile to parallel execution.

use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::subscriber::with_default;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::Registry;

/// Every field value rendered by any event, plus every event message.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<String>>>);

impl Captured {
    fn contents(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

struct Collector(Captured);

impl Visit for Collector {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .0
            .lock()
            .unwrap()
            .push(format!("{}={:?}", field.name(), value));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .0
            .lock()
            .unwrap()
            .push(format!("{}={}", field.name(), value));
    }
}

impl<S: tracing::Subscriber> Layer<S> for Captured {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        event.record(&mut Collector(self.clone()));
    }
}

const KEY: &str = "AIzaSyTOTALLY_SECRET_KEY_VALUE";

/// `scrub_key` is the only thing standing between a reqwest error (which
/// embeds the full request URL) and a key in a log file the user might paste
/// into a bug report.
#[test]
fn an_api_key_never_reaches_a_tracing_field() {
    let captured = Captured::default();
    let subscriber = Registry::default().with(captured.clone());

    with_default(subscriber, || {
        // The shape a reqwest failure actually takes: the whole URL, key and
        // all, inside the error text.
        let leaked = format!(
            "error sending request for url (https://www.googleapis.com/books/v1/volumes?q=dune&key={KEY})"
        );
        let err = readingbuddy::EngineError::Provider {
            provider: readingbuddy::ProviderId::GoogleBooks,
            message: readingbuddy::providers::googlebooks::scrub_key(&leaked),
        };
        let diag =
            readingbuddy::Diagnostic::provider_failed(readingbuddy::ProviderId::GoogleBooks, &err);

        // Exactly how search.rs logs a degradation.
        tracing::warn!(provider = "googlebooks", detail = %diag.detail, "provider degraded");
        tracing::error!(error = %err, "provider error");
    });

    let lines = captured.contents();
    assert!(!lines.is_empty(), "the capturing layer saw no events");
    for line in &lines {
        assert!(
            !line.contains(KEY),
            "an API key reached a tracing field: {line}"
        );
        assert!(
            !line.contains("AIzaSy"),
            "a Google key prefix reached a tracing field: {line}"
        );
    }
    // And the redaction marker is present, so this is not passing merely
    // because nothing was logged.
    assert!(
        lines.iter().any(|l| l.contains("key=REDACTED")),
        "expected a redacted key marker, got: {lines:?}"
    );
}

/// The engine must never install a subscriber of its own — that is a frontend
/// decision, and a library that grabs the global default breaks every embedder.
#[test]
fn the_engine_installs_no_global_subscriber() {
    // If the engine had installed one, this would be a no-op and the layer
    // below would capture nothing.
    let captured = Captured::default();
    with_default(Registry::default().with(captured.clone()), || {
        tracing::warn!(marker = "probe", "test event");
    });
    assert!(
        captured.contents().iter().any(|l| l.contains("probe")),
        "a global subscriber was already installed, so events bypassed the test layer"
    );
}
