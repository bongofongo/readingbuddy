//! The wireless rendezvous, end to end, with nothing leaving the machine
//! (item 15b, stage 2).
//!
//! Two rules shape every test here and they are not the same rule:
//!
//! - **No broadcast packet is ever emitted.** The UDP half goes through
//!   [`ScriptedBeacon`], which is `watch.rs`'s injected `MountStir` channel
//!   applied to discovery: a rendezvous that can only be driven by plugging in
//!   real hardware is a rendezvous with no tests, and the refusals are exactly
//!   the part with bugs in it.
//! - **Loopback is fine.** `tests/provider_http.rs` already binds a real local
//!   port through `wiremock`. "No network in tests, ever" means nothing leaves
//!   the machine, so the TCP half is the real socket, the real framing and the
//!   real import.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use readingbuddy::wireless::{
    Ack, Beacon, Hello, Here, Listener, ListenerMode, Open, PushFrame, body_challenge,
    here_challenge, mac, open_challenge,
};
use readingbuddy::{Engine, EngineConfig};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// The clock, spelled here because `storage::now_unix` is `pub(crate)` — and
/// it stays that way: a facade that exposed *what time is it* would be a
/// surface with no product behind it.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
/// A sidecar that **carries a `partial_md5_checksum`**, which is what makes an
/// import of it idempotent — see `pushing_the_same_thing_twice_changes_nothing`
/// and the test below it for the case where one does not.
const SIDECAR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/koreader/synthetic/Multi-Chapter.sdr/metadata.epub.lua"
);

/// One that does not. `import_book_from_sidecar` says so in its own doc: with
/// nothing to key a mapping on there is no de-duplication to be had.
const UNIDENTIFIED_SIDECAR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/koreader/synthetic/The-Trial.sdr/metadata.epub.lua"
);

// ---- the injected transport ------------------------------------------------

/// A [`Beacon`] made of a queue and a log. Nothing is sent anywhere.
#[derive(Debug, Default)]
struct ScriptedBeacon {
    probes: Mutex<Vec<(Vec<u8>, SocketAddr)>>,
    replies: Mutex<Vec<(Vec<u8>, SocketAddr)>>,
    /// Held open so the responder task parks rather than ending, which is what
    /// a real socket does between datagrams.
    exhausted: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl Beacon for ScriptedBeacon {
    async fn recv(&self) -> Option<(Vec<u8>, SocketAddr)> {
        loop {
            if let Some(next) = self.probes.lock().await.pop() {
                return Some(next);
            }
            self.exhausted.notified().await;
        }
    }
    async fn send_to(&self, bytes: &[u8], to: SocketAddr) -> std::io::Result<()> {
        self.replies.lock().await.push((bytes.to_vec(), to));
        Ok(())
    }
}

impl ScriptedBeacon {
    async fn probe(&self, hello: &Hello) {
        let from: SocketAddr = "192.168.1.55:61862".parse().unwrap();
        self.probes
            .lock()
            .await
            .push((serde_json::to_vec(hello).unwrap(), from));
        self.exhausted.notify_waiters();
    }

    /// Wait briefly for the responder to answer, then report what it said.
    async fn answers(&self) -> Vec<Here> {
        for _ in 0..200 {
            {
                let got = self.replies.lock().await;
                if !got.is_empty() {
                    return got
                        .iter()
                        .map(|(b, _)| serde_json::from_slice(b).unwrap())
                        .collect();
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        Vec::new()
    }

    async fn silent(&self) -> bool {
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        self.replies.lock().await.is_empty()
    }
}

// ---- scaffolding -----------------------------------------------------------

async fn engine() -> (tempfile::TempDir, Engine) {
    let tmp = tempfile::tempdir().unwrap();
    let config = EngineConfig {
        db_url: "sqlite::memory:".into(),
        images_dir: tmp.path().join("database/images"),
        files_dir: tmp.path().join("database/files"),
        vault_dir: tmp.path().join("vault"),
        log_dir: tmp.path().join("logs"),
        google_api_key: None,
        calibre_bin_dir: None,
    };
    (tmp, Engine::open(config).await.unwrap())
}

/// A paired reader, by the same door the installer uses.
async fn pair(engine: &Engine, tmp: &std::path::Path) -> (String, String) {
    let mount = tmp.join("reader");
    std::fs::create_dir_all(mount.join("koreader/frontend")).unwrap();
    std::fs::create_dir_all(mount.join("koreader/plugins")).unwrap();
    std::fs::write(mount.join("koreader/reader.lua"), "").unwrap();
    let report = engine.install_plugin(&mount).await.unwrap();
    let src = std::fs::read_to_string(
        mount
            .join("koreader/plugins/readingbuddy.koplugin")
            .join("pairing.lua"),
    )
    .unwrap();
    // The token is only ever on the device and in our own row; the test reads
    // it the way the plugin does.
    let token = src
        .lines()
        .find_map(|l| l.trim().strip_prefix("token     = "))
        .unwrap()
        .trim_matches(['"', ','])
        .to_string();
    (report.device_id, token)
}

async fn start(listener: &Arc<Listener>, engine: &Engine, beacon: Arc<ScriptedBeacon>, mins: u32) {
    listener
        .start(
            engine.storage().clone(),
            "test-desk".into(),
            LOOPBACK,
            beacon,
            Some(mins),
            unix_now(),
        )
        .await
        .unwrap();
}

/// One push session, spoken by hand the way the plugin's Lua will.
struct Reader {
    stream: BufReader<TcpStream>,
    nonce: String,
    token: String,
}

impl Reader {
    async fn connect(port: u16, device_id: &str, token: &str, nonce: &str) -> (Self, Ack) {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let mut me = Reader {
            stream: BufReader::new(stream),
            nonce: nonce.into(),
            token: token.into(),
        };
        let open = Open {
            v: 1,
            device_id: device_id.into(),
            nonce: nonce.into(),
            mac: mac(token, &open_challenge(nonce)),
        };
        me.send(&open).await;
        let ack = me.ack().await;
        (me, ack)
    }

    async fn send<T: serde::Serialize>(&mut self, v: &T) {
        let mut line = serde_json::to_vec(v).unwrap();
        line.push(b'\n');
        self.stream.get_mut().write_all(&line).await.unwrap();
        self.stream.get_mut().flush().await.unwrap();
    }

    async fn ack(&mut self) -> Ack {
        let mut line = String::new();
        self.stream.read_line(&mut line).await.unwrap();
        serde_json::from_str(line.trim_end()).unwrap_or(Ack {
            ok: false,
            error: Some("no reply".into()),
        })
    }

    async fn push(&mut self, name: &str, body: &str) -> Ack {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(body.as_bytes());
        let hex = digest.iter().fold(String::new(), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        });
        let frame = PushFrame::Entry {
            name: name.into(),
            len: body.len() as u64,
            sha256: hex.clone(),
            mac: mac(&self.token, &body_challenge(&self.nonce, &hex)),
        };
        self.send(&frame).await;
        self.stream
            .get_mut()
            .write_all(body.as_bytes())
            .await
            .unwrap();
        self.stream.get_mut().flush().await.unwrap();
        self.ack().await
    }

    async fn done(&mut self) -> Ack {
        self.send(&PushFrame::Done).await;
        self.ack().await
    }
}

// ---- the tests -------------------------------------------------------------

/// The whole verb: a paired reader finds the desktop, connects, proves itself,
/// and its highlights land through the ordinary import.
#[tokio::test]
async fn a_paired_reader_finds_us_and_pushes_its_highlights() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());
    let beacon = Arc::new(ScriptedBeacon::default());
    start(&listener, &engine, Arc::clone(&beacon), 5).await;

    // Discovery. The reply proves the desktop holds the token, and the reader
    // checks that before sending a byte.
    beacon
        .probe(&Hello {
            v: 1,
            device_id: device_id.clone(),
            nonce: "n1".into(),
        })
        .await;
    let here = beacon.answers().await;
    assert_eq!(here.len(), 1);
    assert_eq!(here[0].name, "test-desk");
    assert_eq!(
        here[0].mac,
        mac(&token, &here_challenge("n1", here[0].tcp_port)),
        "the reader verifies identity, not address"
    );

    // The push.
    let body = std::fs::read_to_string(SIDECAR).unwrap();
    let (mut reader, ack) = Reader::connect(here[0].tcp_port, &device_id, &token, "push-1").await;
    assert!(ack.ok, "{ack:?}");
    assert!(
        reader
            .push("The-Trial.sdr/metadata.epub.lua", &body)
            .await
            .ok
    );
    assert!(reader.done().await.ok);

    // It landed through the same import a cable feeds.
    let books = engine.list_books(&Default::default()).await.unwrap();
    assert_eq!(books.len(), 1, "the sidecar's own book was created");
    let marks = engine.list_highlights(books[0].id.unwrap()).await.unwrap();
    assert!(!marks.is_empty(), "and its highlights came with it");

    // The reader reached us over the LAN. That is not a cable and not a sync.
    let device = engine
        .paired_devices()
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.device_id == device_id)
        .unwrap();
    assert!(device.last_wireless_at.is_some());
    assert_eq!(device.last_lan_addr.as_deref(), Some("127.0.0.1"));
    assert_eq!(
        device.last_synced_at, None,
        "what a reader chose to send is not everything it had"
    );
}

/// The property the spec asks for, and the one the sidecar payload was chosen
/// to give for free: pushing the same reader twice changes nothing.
#[tokio::test]
async fn pushing_the_same_thing_twice_changes_nothing() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());
    let body = std::fs::read_to_string(SIDECAR).unwrap();

    let mut counts = Vec::new();
    for nonce in ["a", "b"] {
        let beacon = Arc::new(ScriptedBeacon::default());
        // `Some(0)` — always — so the first push does not close the door on
        // the second, which is the point being measured rather than a side
        // effect being worked around.
        listener
            .start(
                engine.storage().clone(),
                "desk".into(),
                LOOPBACK,
                beacon,
                Some(0),
                unix_now(),
            )
            .await
            .unwrap();
        let port = listener.status(0).await.tcp_port.unwrap();
        let (mut reader, ack) = Reader::connect(port, &device_id, &token, nonce).await;
        assert!(ack.ok);
        assert!(
            reader
                .push("The-Trial.sdr/metadata.epub.lua", &body)
                .await
                .ok
        );
        assert!(reader.done().await.ok);

        let books = engine.list_books(&Default::default()).await.unwrap();
        let marks = engine.list_highlights(books[0].id.unwrap()).await.unwrap();
        counts.push((books.len(), marks.len()));
    }
    assert_eq!(counts[0], counts[1], "a second push created something");
    listener.stop().await;
}

/// And the honest limit of that property, which the spec overstates.
///
/// *"Idempotence, which the sidecar payload should give you for free"* is only
/// true for a sidecar that carries a `partial_md5_checksum`. Without one there
/// is nothing to key a mapping on, so `import_book_from_sidecar` creates a book
/// and warns `SidecarNotIdentified` — over a cable and, necessarily, over the
/// wire. The transport inherits the import's properties, and that includes this
/// one. Pinned rather than papered over: a reader that cannot identify a file
/// will duplicate it on every push, and whoever builds the pull side should
/// know that before designing around the opposite.
#[tokio::test]
async fn a_sidecar_with_no_checksum_is_no_more_idempotent_over_the_wire() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());
    let body = std::fs::read_to_string(UNIDENTIFIED_SIDECAR).unwrap();

    for nonce in ["a", "b"] {
        listener
            .start(
                engine.storage().clone(),
                "desk".into(),
                LOOPBACK,
                Arc::new(ScriptedBeacon::default()),
                Some(0),
                unix_now(),
            )
            .await
            .unwrap();
        let port = listener.status(0).await.tcp_port.unwrap();
        let (mut reader, ack) = Reader::connect(port, &device_id, &token, nonce).await;
        assert!(ack.ok);
        assert!(
            reader
                .push("The-Trial.sdr/metadata.epub.lua", &body)
                .await
                .ok
        );
        assert!(reader.done().await.ok);
    }
    assert_eq!(
        engine.list_books(&Default::default()).await.unwrap().len(),
        2,
        "this is the documented cable behaviour, not a wireless bug"
    );
    listener.stop().await;
}

/// A rogue on the LAN gets nothing at all, at either layer.
#[tokio::test]
async fn a_stranger_is_answered_with_silence_and_refused_on_connect() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());
    let beacon = Arc::new(ScriptedBeacon::default());
    start(&listener, &engine, Arc::clone(&beacon), 5).await;
    let port = listener.status(0).await.tcp_port.unwrap();

    // A probe naming a device we are not paired with is answered with nothing
    // — not a refusal, which would confirm something is here.
    beacon
        .probe(&Hello {
            v: 1,
            device_id: "somebody-elses-reader".into(),
            nonce: "n".into(),
        })
        .await;
    assert!(beacon.silent().await, "an unpaired probe learned we exist");

    // The right device id with the wrong secret is refused, and the session
    // never opens.
    let (_, ack) = Reader::connect(port, &device_id, "not-the-token", "n2").await;
    assert!(!ack.ok);
    assert!(ack.error.unwrap().contains("prove"), "and it is told why");

    // A device id we have never heard of, likewise.
    let (_, ack) = Reader::connect(port, "nobody", &token, "n3").await;
    assert!(!ack.ok);

    // The real reader still works afterwards: a refusal is not a wedge.
    let (_, ack) = Reader::connect(port, &device_id, &token, "n4").await;
    assert!(ack.ok);
}

/// A captured session cannot be replayed at the listener that honoured it.
#[tokio::test]
async fn a_replayed_nonce_is_refused() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());
    // `Always`, because in a *window* the first completed session shuts the
    // door and a replay has nothing to reach. The nonce check is what protects
    // the desktop that stays open on the LAN, which is the mode it is for.
    start(&listener, &engine, Arc::new(ScriptedBeacon::default()), 0).await;
    let port = listener.status(0).await.tcp_port.unwrap();

    let (mut first, ack) = Reader::connect(port, &device_id, &token, "same").await;
    assert!(ack.ok);
    assert!(first.done().await.ok);

    let (_, ack) = Reader::connect(port, &device_id, &token, "same").await;
    assert!(
        !ack.ok,
        "a nonce this listener already honoured was accepted"
    );
    assert!(ack.error.unwrap().contains("already used"));

    // A different nonce on the same open door is fine — the refusal is about
    // the value, not about the reader having been here before.
    let (_, ack) = Reader::connect(port, &device_id, &token, "different").await;
    assert!(ack.ok);
    listener.stop().await;
}

/// **The nonce set is per open door, not per lifetime**, and closing clears it.
///
/// The scope is deliberate rather than an oversight, and it took writing the
/// wrong test to see why. Replay protection is defence in depth here: a body
/// frame's MAC covers the body's *hash*, so an attacker replaying a captured
/// `open` cannot follow it with bytes of their own — the only thing they can
/// resend is what they captured, which is the same sidecar, which imports
/// idempotently. Keeping nonces across a closed door would therefore buy
/// nothing and cost an unbounded set on a desktop left in `Always` for weeks.
#[tokio::test]
async fn closing_the_door_forgets_the_nonces_it_honoured() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());

    for _ in 0..2 {
        start(&listener, &engine, Arc::new(ScriptedBeacon::default()), 5).await;
        let port = listener.status(0).await.tcp_port.unwrap();
        let (mut r, ack) = Reader::connect(port, &device_id, &token, "same-every-time").await;
        assert!(ack.ok, "a fresh window remembered the last one's nonces");
        assert!(r.done().await.ok);
    }
}

/// A body that does not match the hash its MAC covers is refused before it is
/// parsed, and before the credential is even consulted.
#[tokio::test]
async fn a_swapped_body_is_refused_before_it_is_parsed() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());
    start(&listener, &engine, Arc::new(ScriptedBeacon::default()), 5).await;
    let port = listener.status(0).await.tcp_port.unwrap();

    let (mut reader, ack) = Reader::connect(port, &device_id, &token, "n").await;
    assert!(ack.ok);

    // A well-formed frame whose declared hash belongs to different bytes.
    let body = "return { }";
    let frame = PushFrame::Entry {
        name: "swapped.lua".into(),
        len: body.len() as u64,
        sha256: "00".repeat(32),
        mac: mac(&token, &body_challenge("n", &"00".repeat(32))),
    };
    reader.send(&frame).await;
    reader
        .stream
        .get_mut()
        .write_all(body.as_bytes())
        .await
        .unwrap();
    let ack = reader.ack().await;
    assert!(!ack.ok);
    assert!(
        engine
            .list_books(&Default::default())
            .await
            .unwrap()
            .is_empty()
    );
}

/// Off is the default, and with the listener closed there is nothing to find
/// and nothing to connect to. `docs/decisions.md`'s *fails closed*, at the
/// transport layer.
#[tokio::test]
async fn with_the_listener_off_there_is_nothing_to_find() {
    let (tmp, engine) = engine().await;
    let (device_id, _) = pair(&engine, tmp.path()).await;
    assert_eq!(
        engine.listener_status().await.unwrap().mode,
        ListenerMode::Off,
        "nothing is bound until somebody asks"
    );
    assert_eq!(engine.listener_status().await.unwrap().tcp_port, None);

    let listener = Arc::new(Listener::new());
    let beacon = Arc::new(ScriptedBeacon::default());
    start(&listener, &engine, Arc::clone(&beacon), 5).await;
    let port = listener.status(0).await.tcp_port.unwrap();
    listener.stop().await;

    // A probe after the door shut is not answered, and the port is gone with
    // it — the UDP responder lives and dies with the TCP listener.
    beacon
        .probe(&Hello {
            v: 1,
            device_id,
            nonce: "n".into(),
        })
        .await;
    assert!(beacon.silent().await);
    assert!(
        TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_err(),
        "the tcp listener outlived the stop"
    );
    assert_eq!(listener.status(0).await.mode, ListenerMode::Off);

    // Stopping twice is not an error: a frontend cannot know the window did
    // not expire between drawing the button and the user pressing it.
    assert_eq!(listener.stop().await.mode, ListenerMode::Off);
    assert_eq!(
        engine.stop_listening().await.unwrap().mode,
        ListenerMode::Off
    );
}

/// A completed session closes a window and the sockets go with it.
#[tokio::test]
async fn a_completed_push_closes_the_window_it_came_through() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());
    start(&listener, &engine, Arc::new(ScriptedBeacon::default()), 5).await;
    let port = listener.status(0).await.tcp_port.unwrap();

    let (mut reader, ack) = Reader::connect(port, &device_id, &token, "n").await;
    assert!(ack.ok);
    let body = std::fs::read_to_string(SIDECAR).unwrap();
    assert!(reader.push("a.lua", &body).await.ok);
    assert!(reader.done().await.ok);

    let after = listener.status(unix_now()).await;
    assert_eq!(after.mode, ListenerMode::Off, "the door stayed open");
    assert_eq!(after.tcp_port, None);
    assert_eq!(after.pushes, 1);
    assert!(after.last_push_at.is_some());
    assert!(
        TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_err(),
        "the socket outlived the window"
    );
}

/// One unreadable sidecar does not end a session carrying good ones — the
/// provider rule (degrade, never abort) applied to a transport.
#[tokio::test]
async fn a_bad_sidecar_is_reported_and_the_session_continues() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let listener = Arc::new(Listener::new());
    start(&listener, &engine, Arc::new(ScriptedBeacon::default()), 5).await;
    let port = listener.status(0).await.tcp_port.unwrap();

    let (mut reader, ack) = Reader::connect(port, &device_id, &token, "n").await;
    assert!(ack.ok);
    let bad = reader
        .push("broken.lua", "this is not lua at all {{{")
        .await;
    assert!(!bad.ok, "a sidecar that cannot be parsed was accepted");

    let body = std::fs::read_to_string(SIDECAR).unwrap();
    assert!(
        reader.push("good.lua", &body).await.ok,
        "one bad file ended the session"
    );
    assert!(reader.done().await.ok);
    assert_eq!(
        engine.list_books(&Default::default()).await.unwrap().len(),
        1
    );
}

// ---- stage 3: pull ---------------------------------------------------------

/// The reader's half of a pull, spoken by hand the way the plugin's Lua does.
///
/// It exists because the *reader* is the opener here, and there is no Rust
/// implementation of that side to reuse — which is the point rather than a
/// shortcut around it: this is the only place the two implementations of one
/// protocol are checked against each other.
async fn fake_reader(
    token: String,
    bodies: Vec<(String, String)>,
) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);

        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let open: Open = serde_json::from_str(line.trim_end()).unwrap();
        // The reader verifies the desktop exactly as the desktop verifies the
        // reader. The symmetry is the design, not a nicety.
        assert_eq!(open.mac, mac(&token, &open_challenge(&open.nonce)));
        write.write_all(b"{\"ok\":true}\n").await.unwrap();

        for (name, body) in &bodies {
            use sha2::{Digest, Sha256};
            let hex = Sha256::digest(body.as_bytes())
                .iter()
                .fold(String::new(), |mut s, b| {
                    use std::fmt::Write as _;
                    let _ = write!(s, "{b:02x}");
                    s
                });
            let frame = PushFrame::Entry {
                name: name.clone(),
                len: body.len() as u64,
                sha256: hex.clone(),
                mac: mac(&token, &body_challenge(&open.nonce, &hex)),
            };
            let mut out = serde_json::to_vec(&frame).unwrap();
            out.push(b'\n');
            write.write_all(&out).await.unwrap();
            write.write_all(body.as_bytes()).await.unwrap();
            write.flush().await.unwrap();
            let mut ack = String::new();
            reader.read_line(&mut ack).await.unwrap();
        }
        let mut out = serde_json::to_vec(&PushFrame::Done).unwrap();
        out.push(b'\n');
        write.write_all(&out).await.unwrap();
        write.flush().await.unwrap();
    });
    (port, task)
}

/// A beacon that answers one `HELLO` with a `HERE` pointing at a loopback port.
/// **No packet leaves the machine** — the datagram never reaches a socket.
#[derive(Debug)]
struct Responder {
    token: String,
    tcp_port: u16,
    /// Sign the reply with this instead, to play a rogue.
    key: Option<String>,
    replies: Mutex<Vec<(Vec<u8>, SocketAddr)>>,
}

impl Responder {
    fn new(token: &str, tcp_port: u16) -> Self {
        Responder {
            token: token.into(),
            tcp_port,
            key: None,
            replies: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl Beacon for Responder {
    async fn recv(&self) -> Option<(Vec<u8>, SocketAddr)> {
        if let Some(next) = self.replies.lock().await.pop() {
            return Some(next);
        }
        // Nothing queued: the seeker's own deadline ends this, and parking is
        // what a real socket does meanwhile.
        std::future::pending().await
    }
    async fn send_to(&self, bytes: &[u8], _to: SocketAddr) -> std::io::Result<()> {
        let hello: Hello = serde_json::from_slice(bytes).unwrap();
        let key = self.key.clone().unwrap_or_else(|| self.token.clone());
        let here = Here {
            v: 1,
            name: "Kindle".into(),
            tcp_port: self.tcp_port,
            mac: mac(&key, &here_challenge(&hello.nonce, self.tcp_port)),
        };
        self.replies.lock().await.push((
            serde_json::to_vec(&here).unwrap(),
            SocketAddr::new(LOOPBACK, 61862),
        ));
        Ok(())
    }
}

async fn device_row(engine: &Engine, device_id: &str) -> readingbuddy::PairedDevice {
    engine
        .paired_devices()
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.device_id == device_id)
        .unwrap()
}

/// The whole verb, the other way round: the desktop seeks, the reader answers,
/// and the highlights land through the identical import.
#[tokio::test]
async fn the_desktop_can_pull_from_a_reader_whose_window_is_open() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;
    let body = std::fs::read_to_string(SIDECAR).unwrap();
    let (port, task) = fake_reader(
        token.clone(),
        vec![("Multi-Chapter.sdr/metadata.epub.lua".into(), body)],
    )
    .await;

    let device = device_row(&engine, &device_id).await;
    let beacon = Responder::new(&token, port);
    let report = readingbuddy::wireless::pull_from(
        engine.storage(),
        &device,
        &beacon,
        SocketAddr::new(LOOPBACK, 61862),
        "pull-1",
    )
    .await
    .unwrap();
    task.await.unwrap();

    assert_eq!(report.pulled.len(), 1);
    let books = engine.list_books(&Default::default()).await.unwrap();
    assert_eq!(books.len(), 1);
    assert!(
        !engine
            .list_highlights(books[0].id.unwrap())
            .await
            .unwrap()
            .is_empty()
    );

    // Pull stamps the breadcrumb a push does — same wire, same reader — and is
    // no more a *sync* than a push is.
    let after = device_row(&engine, &device_id).await;
    assert!(after.last_wireless_at.is_some());
    assert_eq!(after.last_lan_addr.as_deref(), Some("127.0.0.1"));
    assert_eq!(after.last_synced_at, None);

    // No listener was ever started: a seeker sends first, so pulling with the
    // door shut is the ordinary case rather than a special one.
    assert_eq!(
        engine.listener_status().await.unwrap().mode,
        ListenerMode::Off
    );
}

/// The property that makes the whole design worth its complexity, asserted from
/// the seeking side this time: **a rogue that answers first gets nothing.**
#[tokio::test]
async fn a_responder_that_cannot_prove_the_token_is_never_connected_to() {
    let (tmp, engine) = engine().await;
    let (device_id, token) = pair(&engine, tmp.path()).await;

    // A port that records any connection we might wrongly make to it.
    let trap = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = trap.local_addr().unwrap().port();
    let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&connected);
    tokio::spawn(async move {
        if trap.accept().await.is_ok() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });

    let device = device_row(&engine, &device_id).await;
    let mut beacon = Responder::new(&token, port);
    beacon.key = Some("a-rogue-on-the-cafe-lan".into());

    // The seeker keeps reading rather than trusting whoever replied first, so
    // with only a rogue answering it never stops waiting — which is what the
    // real socket's deadline turns into a refusal.
    let outcome = tokio::time::timeout(
        std::time::Duration::from_millis(300),
        readingbuddy::wireless::pull_from(
            engine.storage(),
            &device,
            &beacon,
            SocketAddr::new(LOOPBACK, 61862),
            "n",
        ),
    )
    .await;
    if let Ok(r) = outcome {
        assert!(r.is_err(), "a rogue's answer was believed");
    }
    assert!(
        !connected.load(std::sync::atomic::Ordering::SeqCst),
        "the desktop dialled a responder that could not prove the token"
    );
    assert!(
        engine
            .list_books(&Default::default())
            .await
            .unwrap()
            .is_empty()
    );
}

/// Nobody answering is a refusal with an action in it — not a hang, and not a
/// silent success.
#[tokio::test]
async fn a_reader_whose_window_is_shut_is_a_refusal_that_says_what_to_do() {
    let (tmp, engine) = engine().await;
    let (device_id, _) = pair(&engine, tmp.path()).await;

    /// A LAN where the reader's window is shut.
    #[derive(Debug)]
    struct Silence;
    #[async_trait::async_trait]
    impl Beacon for Silence {
        async fn recv(&self) -> Option<(Vec<u8>, SocketAddr)> {
            None
        }
        async fn send_to(&self, _: &[u8], _: SocketAddr) -> std::io::Result<()> {
            Ok(())
        }
    }

    let device = device_row(&engine, &device_id).await;
    let err = readingbuddy::wireless::pull_from(
        engine.storage(),
        &device,
        &Silence,
        SocketAddr::new(LOOPBACK, 61862),
        "n",
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("window"), "{err}");

    // And nothing was stamped: a pull that found nobody is not a contact.
    assert_eq!(device_row(&engine, &device_id).await.last_wireless_at, None);
}

/// A reader we have no row for cannot be pulled from, and the refusal lands
/// **before any packet is built** — there is no token to challenge with.
#[tokio::test]
async fn pulling_from_a_reader_we_never_paired_with_is_refused() {
    let (_tmp, engine) = engine().await;
    let err = engine.pull_from_reader("nobody").await.unwrap_err();
    assert!(err.to_string().contains("paired"), "{err}");
}
