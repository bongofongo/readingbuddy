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
use tracing_subscriber::filter::LevelFilter;
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

/// A **note body and its file path** must never reach a log above `trace!`.
///
/// `CLAUDE.md`'s tracing rule names highlight text, note bodies and search
/// queries as the user's private reading. The vault path is the subtle half and
/// is why this test exists at all: a note's filename is its slugified *title*,
/// and a derived title is the first six words of the body — so
/// `20260805-sunja-s-dignity-under-han-pressure.md` **is** the note's opening
/// words, and a watcher that logged the path it was working on would be logging
/// prose. The mount watcher logs its volume paths at `info!` quite correctly; a
/// vault path is not the same kind of path.
///
/// Filtered to `DEBUG` and above deliberately, so this asserts the *rule* —
/// `trace!` may carry it — rather than asserting that nothing is ever logged.
#[tokio::test]
async fn a_note_body_never_reaches_a_log_above_trace() {
    use readingbuddy::{Engine, EngineConfig, NewNoteInput, VaultStir, VaultWatcher};

    const SECRET: &str = "Sunja's dignity under Hansu's calculation";

    let tmp = tempfile::tempdir().unwrap();
    let engine = Engine::open(EngineConfig::rooted_at(tmp.path()))
        .await
        .unwrap();
    let created = engine
        .create_note(NewNoteInput {
            body: SECRET.into(),
            ..Default::default()
        })
        .await
        .unwrap();
    // Every word of the body ends up in the filename, since the title is
    // derived from it — which is the whole point of this test.
    assert!(
        created.file.to_string_lossy().contains("dignity"),
        "the fixture no longer puts the body's words in the path, so this \
         test would pass for the wrong reason"
    );

    let captured = Captured::default();
    let subscriber = Registry::default().with(captured.clone().with_filter(LevelFilter::DEBUG));

    let storage = engine.storage().clone();
    let vault = engine.vault_dir().to_path_buf();
    let path = created.file.clone();

    // `with_default` is not held across an await — the subscriber guard is not
    // `Send` — so the watcher is driven inside a `block_in_place`-free local
    // runtime instead: one current-thread runtime, entirely inside the scope.
    std::thread::scope(|s| {
        s.spawn(|| {
            tracing::subscriber::with_default(subscriber, || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async {
                    // An edit made outside, then the file removed — both of the
                    // watcher's outcomes, plus the sweep.
                    let raw = std::fs::read_to_string(&path).unwrap();
                    std::fs::write(&path, raw.replace(SECRET, "rewritten outside")).unwrap();

                    let (tx, rx) = tokio::sync::mpsc::channel(8);
                    let mut watcher = VaultWatcher::from_stirs(&vault, storage.clone(), rx)
                        .quiet_for(std::time::Duration::from_millis(30));
                    tx.send(VaultStir(path.clone())).await.unwrap();
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
                        .await;

                    std::fs::remove_file(&path).unwrap();
                    tx.send(VaultStir(path.clone())).await.unwrap();
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
                        .await;
                });
            });
        });
    });

    for line in captured.contents() {
        for word in ["Sunja", "dignity", "Hansu", "calculation"] {
            assert!(
                !line.contains(word),
                "a note's prose reached a log above trace!: {line}"
            );
        }
        assert!(
            !line.contains("vault/"),
            "a vault path reached a log above trace!: {line}"
        );
    }
}

/// A **search query** is the user's private reading too, and a new query path is
/// exactly where a helpful `info!("searching for {q}")` gets added.
///
/// The sibling of the test above, and it covers the two things a search touches
/// that the watcher does not: the words the reader typed, and the text of the
/// highlight that came back. Filtered to `DEBUG` for the same reason — this
/// asserts the *rule* (`trace!` may carry it), not that nothing is logged. The
/// count of hits is fine at `debug!`: a number is not the query.
#[test]
fn a_search_query_never_reaches_a_log_above_trace() {
    use readingbuddy::{Engine, EngineConfig, NewHighlight, NewNoteInput};

    const TYPED: &str = "Hansu";
    const PASSAGE: &str = "Hansu's calculation, which Sunja never saw coming";

    let captured = Captured::default();
    let subscriber = Registry::default().with(captured.clone().with_filter(LevelFilter::DEBUG));

    std::thread::scope(|s| {
        s.spawn(|| {
            tracing::subscriber::with_default(subscriber, || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async {
                    let tmp = tempfile::tempdir().unwrap();
                    let engine = Engine::open(EngineConfig::rooted_at(tmp.path()))
                        .await
                        .unwrap();
                    let book = engine
                        .save_book(&readingbuddy::Book {
                            title: Some("Pachinko".into()),
                            ..Default::default()
                        })
                        .await
                        .unwrap()
                        .id
                        .unwrap();
                    engine
                        .storage()
                        .insert_highlight(
                            book,
                            &NewHighlight {
                                text: PASSAGE.into(),
                                chapter: None,
                                page: None,
                                pos0: None,
                                pos1: None,
                                ko_datetime: Some("2026-01-01 10:00:00".into()),
                                ko_datetime_updated: None,
                                color: None,
                                note: None,
                                source: "koreader".into(),
                            },
                        )
                        .await
                        .unwrap();
                    engine
                        .create_note(NewNoteInput {
                            book_id: Some(book),
                            body: PASSAGE.into(),
                            ..Default::default()
                        })
                        .await
                        .unwrap();

                    let hits = engine.search_marks(TYPED, None, None, 10).await.unwrap();
                    assert_eq!(
                        hits.len(),
                        2,
                        "the fixture has to actually match, or this passes for \
                         the wrong reason"
                    );
                    // A query that matches nothing goes down the same path.
                    assert!(
                        engine
                            .search_marks("thermodynamics", None, None, 10)
                            .await
                            .unwrap()
                            .is_empty()
                    );
                });
            });
        });
    });

    for line in captured.contents() {
        for word in ["Hansu", "Sunja", "calculation", "thermodynamics"] {
            assert!(
                !line.contains(word),
                "a search query or a highlight reached a log above trace!: {line}"
            );
        }
    }
}

/// **The pairing token is never logged, at any level** — not even `trace!`.
///
/// Migration `0019` says so and is stricter than `CLAUDE.md`'s rule about
/// highlight text and search queries, which permits `trace!`. Item 15b is where
/// a debug line would be reached for: the wireless listener verifies a MAC and
/// refuses, and *"expected a38b… got 26ae…"* is the first thing anybody writes
/// when a credential will not verify on a device they cannot attach a debugger
/// to. It is also the line that puts a reproducible forgery in a log file.
///
/// So the capture is unfiltered — every level, including `TRACE` — and it
/// drives the whole rendezvous: a probe from a paired reader (which makes the
/// listener compute a MAC under the real token), a connection that proves
/// itself, and a connection that fails to. The refusal path matters most: the
/// success path has no reason to mention the secret and the failure path has an
/// excuse.
#[tokio::test]
async fn the_pairing_token_never_reaches_a_log_at_any_level() {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    use readingbuddy::wireless::{Beacon, Listener, mac, open_challenge};
    use readingbuddy::{Engine, EngineConfig};
    use tokio::io::AsyncWriteExt;

    /// A beacon that replays one probe and records nothing. No packet leaves
    /// the machine; see `tests/wireless.rs` for the same seam in full.
    #[derive(Debug)]
    struct OneProbe(tokio::sync::Mutex<Option<Vec<u8>>>);

    #[async_trait::async_trait]
    impl Beacon for OneProbe {
        async fn recv(&self) -> Option<(Vec<u8>, SocketAddr)> {
            match self.0.lock().await.take() {
                Some(b) => Some((b, "192.168.1.9:61862".parse().unwrap())),
                // Park rather than end: a real socket waits between datagrams.
                None => std::future::pending().await,
            }
        }
        async fn send_to(&self, _: &[u8], _: SocketAddr) -> std::io::Result<()> {
            Ok(())
        }
    }

    let tmp = tempfile::tempdir().unwrap();
    let engine = Engine::open(EngineConfig::rooted_at(tmp.path()))
        .await
        .unwrap();

    let mount = tmp.path().join("reader");
    std::fs::create_dir_all(mount.join("koreader/frontend")).unwrap();
    std::fs::create_dir_all(mount.join("koreader/plugins")).unwrap();
    std::fs::write(mount.join("koreader/reader.lua"), "").unwrap();
    let device_id = engine.install_plugin(&mount).await.unwrap().device_id;
    let token = std::fs::read_to_string(
        mount
            .join("koreader/plugins/readingbuddy.koplugin")
            .join("pairing.lua"),
    )
    .unwrap()
    .lines()
    .find_map(|l| l.trim().strip_prefix("token     = "))
    .unwrap()
    .trim_matches(['"', ','])
    .to_string();
    assert_eq!(token.len(), 64, "the fixture stopped being a real token");

    let captured = Captured::default();
    // **No level filter at all.** This is the one secret `trace!` may not carry.
    let subscriber = Registry::default().with(captured.clone());
    let guard = tracing::subscriber::set_default(subscriber);

    let listener = Arc::new(Listener::new());
    let probe = serde_json::to_vec(&serde_json::json!({
        "v": 1, "device_id": device_id, "nonce": "n1"
    }))
    .unwrap();
    listener
        .start(
            engine.storage().clone(),
            "desk".into(),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            Arc::new(OneProbe(tokio::sync::Mutex::new(Some(probe)))),
            Some(0),
            1_700_000_000,
        )
        .await
        .unwrap();
    let port = listener.status(0).await.tcp_port.unwrap();

    // A connection that proves itself, and one that does not.
    for (nonce, key) in [("good", token.as_str()), ("bad", "not-the-token")] {
        let mut s = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let open = serde_json::json!({
            "v": 1, "device_id": device_id, "nonce": nonce,
            "mac": mac(key, &open_challenge(nonce)),
        });
        let mut line = serde_json::to_vec(&open).unwrap();
        line.push(b'\n');
        s.write_all(&line).await.unwrap();
        s.flush().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
    listener.stop().await;
    drop(guard);

    for line in captured.contents() {
        assert!(
            !line.contains(&token),
            "the pairing token reached a log: {line}"
        );
        // A MAC computed under the real token is not the token, but publishing
        // one beside its challenge is a gift to anybody replaying it — and it
        // is what a "why did this not verify" line would print.
        assert!(
            !line.contains(&mac(&token, &open_challenge("good"))),
            "a MAC computed under the pairing token reached a log: {line}"
        );
    }
    assert!(
        !captured.contents().is_empty(),
        "nothing was captured at all, so this test proves nothing"
    );
}
