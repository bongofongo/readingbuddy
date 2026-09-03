//! A paired reader reaching its paired computer over the LAN (item 15b).
//!
//! This is the second of the two ways annotations come off a reader, and it is
//! deliberately built as a **transport for the import we already have** rather
//! than as a second import. The bytes that cross are the sidecar bytes the
//! reader is already holding; they land in
//! [`crate::koreader::import_book_from_sidecar_src`], which is the same parser,
//! the same idempotency rule and the same goldens the cable path uses. A second
//! wire format would be a second parser for the same information, tested by
//! nobody, and its first divergence would appear as highlights importing
//! differently depending on which cable they came down.
//!
//! # The rendezvous, and why it is one protocol used twice
//!
//! Each verb has a side that is **open** and a side that **seeks**, and push
//! and pull swap them. So there is one rule rather than two designs:
//!
//! > **Whoever is open answers probes. Whoever seeks probes and listens.**
//!
//! ```text
//! seeker → broadcast   HELLO  {device_id, nonce}
//! opener → unicast     HERE   {name, tcp_port, mac(token, nonce, port)}
//! seeker → tcp connect        (verify the mac before sending one byte)
//! ```
//!
//! The consequence that makes the desktop's toggle a feature rather than a
//! limitation: **discovery only works while the other side is open.** With the
//! listener off there is no service to find, nothing to fingerprint and nothing
//! to leak, and the reader's failure message writes itself — *no readingbuddy
//! answered; is it listening?* That is `docs/decisions.md`'s **fails closed** at
//! the transport layer.
//!
//! # The token never crosses the wire
//!
//! 15a minted a 32-byte token over USB and wrote it into `pairing.lua`. It is a
//! **challenge-response credential**: both sides prove possession and neither
//! transmits it. That buys the property that matters more than confidentiality
//! here — **the reader verifies identity, not address** — so a rogue responder
//! on a café LAN that answers a broadcast first cannot make a reader send it a
//! single highlight.
//!
//! Two details are load-bearing and were both measured rather than assumed.
//!
//! **The key is the token's 64 hex characters, not the 32 bytes they encode.**
//! koreader-base's `ffi/sha2.lua` computes correct HMAC-SHA256 either way, and
//! the two answers are completely different with neither side looking wrong —
//! see `docs/spec-15b-the-wireless-link.md` for the pinned vectors. The hex is
//! chosen because it is the string `pairing.lua` literally holds, so the device
//! needs no `hex_to_bin` and cannot get a decode wrong somewhere nobody can
//! attach a debugger.
//!
//! **Every MAC is domain-separated by a prefix.** `here:`, `open:` and `body:`
//! cannot be replayed as one another, which is what stops a captured discovery
//! reply from being presented as an authorisation to push.
//!
//! # What is tested, and what cannot be
//!
//! `watch.rs`'s split, applied exactly: [`UdpBeacon`] is the adapter that
//! cannot run in CI, and everything else here can. The rendezvous transport is
//! behind the [`Beacon`] trait and injected — a discovery path that can only be
//! driven by plugging in real hardware is a discovery path with no tests, which
//! is the same sentence that file writes about `notify`. **No test in this
//! module emits a broadcast packet.** The TCP half runs on loopback, which is
//! already ordinary here (`tests/provider_http.rs` binds a real local port via
//! `wiremock`), because "no network in tests, ever" means nothing leaves the
//! machine.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::diagnostic::Diagnostic;
use crate::error::Result;
use crate::koreader::{PullReport, import_book_from_sidecar_src};
use crate::storage::{PairedDevice, Storage, now_unix};

/// The one fixed port in the protocol.
///
/// A `const` and **not a config knob**, for `MOUNT_QUIET`'s reason: a knob
/// means the tested value and the shipped value can differ, and a rendezvous
/// that works in the test suite and not on the user's LAN is the worst possible
/// place for that. The TCP port deliberately is *not* fixed — it is announced
/// in the reply, so a busy port is a runtime fact rather than a support
/// conversation.
///
/// **61862 was checked, not picked.** The IANA service-name registry has
/// **zero** assignments at or above 61000 (the whole 49152–65535 range is
/// Dynamic/Private and unregistered), and Linux's default ephemeral range tops
/// out at 60999 — so a fixed port above 61000 collides with neither a
/// registered service nor a kernel-allocated one.
pub const RENDEZVOUS_PORT: u16 = 61862;

/// Bumped when a message changes shape. Both sides refuse a version they do not
/// know rather than guessing at a field.
pub const PROTOCOL_VERSION: u32 = 1;

/// How long *Listen now* stays open when the caller names no number.
const DEFAULT_WINDOW_MINUTES: u32 = 5;

/// The largest sidecar the listener will read from one entry.
///
/// Not tuning, and the same argument as the daemon's `MAX_LINE`: without it
/// anything that can open a TCP connection can declare a length and make us
/// allocate for it. A `metadata.epub.lua` for a heavily annotated book is tens
/// of kilobytes; four megabytes is far above any real one and far below a
/// problem.
const MAX_SIDECAR_BYTES: u64 = 4 * 1024 * 1024;

/// The longest header line. A push frame is a small JSON object; anything
/// larger is not one.
const MAX_HEADER_BYTES: u64 = 64 * 1024;

// ---- the listener's three states -------------------------------------------

/// Whether the desktop is reachable, and until when.
///
/// The **UDP responder lives and dies with the TCP listener** — one state, not
/// two. An announcer that outlives the thing it announces is a device telling
/// the user to try again against a closed port, and here that invariant is
/// structural rather than remembered: both tasks are owned by one [`Running`],
/// whose `Drop` aborts both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerMode {
    /// Nothing is bound. **The default**, and the state that makes the whole
    /// design fail closed.
    Off,
    /// Bound until `until` (unix seconds), or until a push completes.
    Window { until: i64 },
    /// Bound for as long as the host runs. For the desktop that lives on the
    /// LAN; the daemon's `--listen` is how it is asked for.
    Always,
}

impl ListenerMode {
    /// Is anything bound, as of `now`?
    pub fn is_open(&self, now: i64) -> bool {
        match self {
            ListenerMode::Off => false,
            ListenerMode::Window { until } => now < *until,
            ListenerMode::Always => true,
        }
    }
}

/// What the listener is doing, for a frontend to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerStatus {
    pub mode: ListenerMode,
    /// The port a reader would connect to, announced in every `HERE`. `None`
    /// when nothing is bound.
    pub tcp_port: Option<u16>,
    /// Completed pushes since this host started. **Not a count of anything
    /// outstanding** — `docs/decisions.md` forbids a number of things waiting,
    /// and this is a number of things done.
    pub pushes: u64,
    pub last_push_at: Option<i64>,
}

/// Why the listener refused.
///
/// An enum rather than an `EngineError::Other(String)` for [`PluginRefusal`]'s
/// reason: a refusal is a decision, and every arm is something a caller might
/// reasonably branch on — a frontend distinguishes *nothing is listening* from
/// *that reader is not ours*, and only one of them is worth a button.
///
/// [`PluginRefusal`]: crate::plugin::PluginRefusal
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WirelessRefusal {
    /// Nothing is bound. The state everything degrades to.
    #[error("readingbuddy is not listening")]
    NotListening,
    /// A `device_id` we have no pairing row for. **Answered with silence over
    /// UDP**: a probe from a stranger gets no reply at all, so an unpaired
    /// scanner cannot even learn that something is here.
    #[error("no reader is paired with this readingbuddy under that id")]
    UnknownDevice,
    /// The MAC did not verify. Deliberately carries no detail — which byte
    /// differed is exactly what an attacker would like to know.
    #[error("that reader could not prove it holds the pairing token")]
    BadCredential,
    /// A nonce this listener has already honoured.
    #[error("that request was already used")]
    ReplayedNonce,
    /// The protocol version is not one we speak.
    #[error("this reader speaks wireless protocol v{theirs}; this readingbuddy speaks v{ours}")]
    UnsupportedVersion { theirs: u32, ours: u32 },
    /// Nobody answered, or nobody who could prove the token did.
    ///
    /// The reader's window is shut, it is on another subnet, or the AP dropped
    /// the broadcast — and a client must not guess between them: *no reader
    /// answered; is its window open?* is the only honest sentence, and it is
    /// the mirror of the push side's *is the door open on your computer?*
    #[error("no paired reader answered; is its window open?")]
    ReaderNotFound,
    #[error("malformed wireless message: {0}")]
    Malformed(String),
    #[error("a pushed sidecar declared {len} bytes; the limit is {max}")]
    TooLarge { len: u64, max: u64 },
}

// ---- the credential --------------------------------------------------------

/// HMAC-SHA256 of `message` under `token`, lowercase hex.
///
/// **`token` is used as its own bytes** — the 64 hex characters, not the 32 they
/// encode. See the module header: both are valid HMAC and the device's own
/// library will compute either without complaint, so the choice has to be
/// written down in one place and this is it.
pub fn mac(token: &str, message: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(token.as_bytes())
        .expect("hmac accepts a key of any length");
    m.update(message.as_bytes());
    let out = m.finalize().into_bytes();
    use std::fmt::Write as _;
    out.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Whether `presented` is the MAC of `message` under `token`.
///
/// Constant-time in the comparison, via the `hmac` crate's own verifier rather
/// than `==` on two strings: a byte-at-a-time comparison of a credential is the
/// textbook timing leak, and it costs nothing to not write one.
pub fn verify(token: &str, message: &str, presented: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let Ok(raw) = hex_to_bytes(presented) else {
        return false;
    };
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(token.as_bytes())
        .expect("hmac accepts a key of any length");
    m.update(message.as_bytes());
    m.verify_slice(&raw).is_ok()
}

fn hex_to_bytes(s: &str) -> std::result::Result<Vec<u8>, ()> {
    if !s.len().is_multiple_of(2) {
        return Err(());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

/// The message a `HERE` reply signs. Binds the announced **port**, so a rogue
/// cannot capture a reply and re-advertise it pointing somewhere else.
pub fn here_challenge(nonce: &str, tcp_port: u16) -> String {
    format!("here:{PROTOCOL_VERSION}:{nonce}:{tcp_port}")
}

/// The message a push's opening frame signs.
pub fn open_challenge(nonce: &str) -> String {
    format!("open:{PROTOCOL_VERSION}:{nonce}")
}

/// The message one pushed sidecar signs.
///
/// It covers the **body's hash**, not just the nonce, so the credential is
/// bound to the payload: a session's opening handshake alone would authorise
/// any bytes at all to follow, and on a LAN with no TLS that is a difference
/// worth three lines of Lua.
pub fn body_challenge(nonce: &str, sha256_hex: &str) -> String {
    format!("body:{PROTOCOL_VERSION}:{nonce}:{sha256_hex}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

// ---- the three messages ----------------------------------------------------
//
// A **small closed protocol**, and it must never accept `readingbuddy_api::Call`.
// A reader is not a trusted local client, and exposing sixty methods to a LAN
// peer in order to serve two verbs is the opposite of `server.rs`'s own rule
// that the transport names no method. These three shapes are the whole surface
// a reader can reach.

/// UDP, reader → broadcast. *Is my computer here?*
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Hello {
    pub v: u32,
    pub device_id: String,
    pub nonce: String,
}

/// UDP, desktop → reader, unicast. *Here, on this port, and here is the proof.*
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Here {
    pub v: u32,
    pub name: String,
    pub tcp_port: u16,
    pub mac: String,
}

/// TCP, first line. Opens a push session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Open {
    pub v: u32,
    pub device_id: String,
    pub nonce: String,
    pub mac: String,
}

/// TCP, one per sidecar, then `Done`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PushFrame {
    /// `len` bytes of sidecar source follow this line, verbatim.
    Entry {
        /// What the reader calls the file. **Diagnostics only** — nothing opens
        /// it, and it is not a handle anybody may follow.
        name: String,
        len: u64,
        sha256: String,
        mac: String,
    },
    Done,
}

/// TCP, our reply to every frame.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ack {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Ack {
    fn ok() -> Self {
        Ack {
            ok: true,
            error: None,
        }
    }
    fn refused(r: &WirelessRefusal) -> Self {
        Ack {
            ok: false,
            error: Some(r.to_string()),
        }
    }
}

/// What one session moved.
///
/// **The same type in both directions**, because the same bytes move the same
/// way: entries always flow *reader → desktop*, whether the reader connected
/// (push) or we did (pull). "Wireless is read-only toward us" is not a policy
/// somebody has to enforce here — it is the shape of the protocol.
#[derive(Debug, Default)]
pub struct WirelessReport {
    pub device_id: String,
    /// One per sidecar that arrived, in the order they did.
    pub pulled: Vec<PullReport>,
    pub warnings: Vec<Diagnostic>,
}

// ---- the rendezvous transport, injected ------------------------------------

/// The UDP half, behind a trait so no test emits a broadcast packet.
///
/// `watch.rs` injects `MountStir`s rather than driving `notify`; this injects
/// datagrams rather than driving a socket, for the identical reason. The
/// ladder's *ordering and fallback* — the part that actually has bugs — is then
/// a unit test with a scripted responder.
#[async_trait::async_trait]
pub trait Beacon: Send + Sync + std::fmt::Debug {
    /// The next probe, and who sent it. `None` when the socket is finished.
    async fn recv(&self) -> Option<(Vec<u8>, SocketAddr)>;
    /// Answer one probe.
    async fn send_to(&self, bytes: &[u8], to: SocketAddr) -> std::io::Result<()>;
}

/// The real one. **The only thing in this module that cannot run in CI**, which
/// is `watch.rs`'s `watch_mounts` in a different costume.
#[derive(Debug)]
pub struct UdpBeacon {
    socket: UdpSocket,
}

impl UdpBeacon {
    /// Bind the fixed rendezvous port with broadcast enabled. The **opener's**
    /// socket: it must be on the well-known port, because that is where probes
    /// arrive.
    pub async fn bind(addr: IpAddr) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddr::new(addr, RENDEZVOUS_PORT)).await?;
        socket.set_broadcast(true)?;
        Ok(UdpBeacon { socket })
    }

    /// Bind **any** port, with broadcast enabled. The *seeker's* socket.
    ///
    /// Deliberately not the fixed port: a desktop that is both listening and
    /// pulling would otherwise be asking to bind a port it already holds, and
    /// the seeker has no need of a well-known address — it sends first, so the
    /// reply comes back to whatever it sent from.
    pub async fn bind_ephemeral(addr: IpAddr) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddr::new(addr, 0)).await?;
        socket.set_broadcast(true)?;
        Ok(UdpBeacon { socket })
    }

    /// Stop waiting after `d`. A seeker must not block for ever on a LAN where
    /// nothing is going to answer.
    pub fn deadline(self, d: std::time::Duration) -> TimedBeacon {
        TimedBeacon {
            inner: self,
            until: tokio::time::Instant::now() + d,
        }
    }
}

/// A [`Beacon`] that gives up. `recv` answers `None` once the deadline passes,
/// which is exactly the *nobody answered* the seeker turns into a refusal.
#[derive(Debug)]
pub struct TimedBeacon {
    inner: UdpBeacon,
    until: tokio::time::Instant,
}

#[async_trait::async_trait]
impl Beacon for TimedBeacon {
    async fn recv(&self) -> Option<(Vec<u8>, SocketAddr)> {
        tokio::time::timeout_at(self.until, self.inner.recv())
            .await
            .ok()
            .flatten()
    }
    async fn send_to(&self, bytes: &[u8], to: SocketAddr) -> std::io::Result<()> {
        self.inner.send_to(bytes, to).await
    }
}

#[async_trait::async_trait]
impl Beacon for UdpBeacon {
    async fn recv(&self) -> Option<(Vec<u8>, SocketAddr)> {
        // A probe is a small JSON object; anything longer is not one, and
        // reading into a fixed buffer means a flood costs no allocation.
        let mut buf = vec![0u8; 2048];
        match self.socket.recv_from(&mut buf).await {
            Ok((n, from)) => {
                buf.truncate(n);
                Some((buf, from))
            }
            Err(e) => {
                tracing::debug!(error = %e, "rendezvous socket closed");
                None
            }
        }
    }

    async fn send_to(&self, bytes: &[u8], to: SocketAddr) -> std::io::Result<()> {
        self.socket.send_to(bytes, to).await.map(|_| ())
    }
}

// ---- the state machine, with no I/O in it ----------------------------------

/// The listener's state, separated from its sockets so it can be tested
/// without any.
#[derive(Debug)]
struct State {
    mode: ListenerMode,
    tcp_port: Option<u16>,
    pushes: u64,
    last_push_at: Option<i64>,
    /// Nonces this listener has already honoured, so a captured session cannot
    /// be replayed at it.
    ///
    /// **Scoped to one open door and cleared when it shuts**, which is a
    /// decision rather than a convenience. This is defence in depth: a body
    /// frame's MAC covers the body's *hash*, so an attacker who replays a
    /// captured `open` cannot follow it with bytes of their own — the only
    /// thing they can resend is what they captured, which is the same sidecar,
    /// which imports idempotently. Remembering nonces across a closed door
    /// would therefore buy nothing and cost an unbounded set on a desktop left
    /// in `Always` for weeks.
    seen: HashSet<String>,
}

impl State {
    fn status(&self) -> ListenerStatus {
        ListenerStatus {
            mode: self.mode,
            tcp_port: self.tcp_port,
            pushes: self.pushes,
            last_push_at: self.last_push_at,
        }
    }

    /// A completed push closes a **window**, and leaves `Always` alone.
    ///
    /// At the granularity of a *session*, not a sidecar: one tap on the reader
    /// is one session carrying every book it has, and closing after the first
    /// file would strand the other thirty-nine.
    fn completed_push(&mut self, now: i64) {
        self.pushes += 1;
        self.last_push_at = Some(now);
        if matches!(self.mode, ListenerMode::Window { .. }) {
            self.mode = ListenerMode::Off;
        }
    }
}

// ---- the listener ----------------------------------------------------------

/// The two sockets and the tasks that serve them.
///
/// **`Drop` aborts both**, which is how *the UDP responder lives and dies with
/// the TCP listener* stops being a thing somebody has to remember. Replacing or
/// clearing the `Option<Running>` on [`Listener`] is the only way to stop, and
/// it necessarily takes both down together.
#[derive(Debug)]
struct Running {
    tcp: JoinHandle<()>,
    udp: JoinHandle<()>,
    expiry: Option<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.tcp.abort();
        self.udp.abort();
        if let Some(e) = &self.expiry {
            e.abort();
        }
    }
}

/// The desktop's half of the rendezvous.
///
/// # Why this is the engine's and not the daemon's
///
/// `docs/spec-15b-the-wireless-link.md` proposed the daemon, on the grounds
/// that the mode is runtime state that belongs beside the socket. Building it
/// showed that cannot work: `crates/api` depends on the engine and on serde and
/// on **nothing else**, and the GUI links `readingbuddy-api` in-process with no
/// daemon anywhere in the picture. A listener owned by `readingbuddyd` is a
/// listener the devices page can never turn on — and the devices page is this
/// feature's frontend. So the state is here, where every host can reach it
/// through the one boundary they all share.
///
/// # Why this one spawns, when `watch.rs` says the engine never does
///
/// That rule is real and this is a deliberate exception, argued the way item 24
/// argued `VaultWatcher`'s. `MountWatcher` must not act alone because a mount's
/// consequence is a **decision** — scan this, ignore that, ask the user — and a
/// decision belongs to the frontend. A rendezvous responder's consequence is
/// not a decision: it is *answer this probe with our port* and *import these
/// sidecar bytes*, the same answer for every frontend, with nothing to ask
/// anybody. And unlike a watcher, a listening socket has no honest pull-shaped
/// surface across an API seam — `StartListening` travels as JSON and cannot
/// hand back an object to poll.
///
/// The exception is paid for in the two places it could hurt. Nothing is bound
/// until a caller says so, so a `rb list` that never asks spawns nothing; and
/// every task is owned by a [`Running`] whose `Drop` aborts it, so there is no
/// detached work outliving the handle.
#[derive(Debug, Default)]
pub struct Listener {
    state: Mutex<Option<State>>,
    running: Mutex<Option<Running>>,
}

impl Listener {
    pub fn new() -> Self {
        Self::default()
    }

    /// What the listener is doing right now.
    ///
    /// Reports `Off` for a window whose time has passed even if the tasks have
    /// not yet been reaped: the mode is the answer to *would a reader reach
    /// us*, and a lingering task that will refuse everything is not a yes.
    pub async fn status(&self, now: i64) -> ListenerStatus {
        let guard = self.state.lock().await;
        match guard.as_ref() {
            None => ListenerStatus {
                mode: ListenerMode::Off,
                tcp_port: None,
                pushes: 0,
                last_push_at: None,
            },
            Some(s) if !s.mode.is_open(now) => ListenerStatus {
                mode: ListenerMode::Off,
                tcp_port: None,
                ..s.status()
            },
            Some(s) => s.status(),
        }
    }

    /// Bind, and start answering.
    ///
    /// `bind` is a parameter rather than a constant so the whole of this can be
    /// exercised on `127.0.0.1`: the engine passes `0.0.0.0`, tests pass
    /// loopback, and nothing leaves the machine in a test either way.
    ///
    /// Starting an already-running listener **replaces** it, which is how
    /// *Listen now* pressed twice extends the window rather than erroring.
    pub async fn start(
        self: &Arc<Self>,
        storage: Storage,
        name: String,
        bind: IpAddr,
        beacon: Arc<dyn Beacon>,
        minutes: Option<u32>,
        now: i64,
    ) -> Result<ListenerStatus> {
        let tcp = TcpListener::bind(SocketAddr::new(bind, 0)).await?;
        let port = tcp.local_addr()?.port();

        let mode = match minutes {
            // `Some(0)` is how a caller asks for "until I say stop". A window
            // of zero minutes is not a thing anybody wants and would otherwise
            // be a listener that closes before the reader finishes probing.
            Some(0) => ListenerMode::Always,
            Some(m) => ListenerMode::Window {
                until: now + i64::from(m) * 60,
            },
            None => ListenerMode::Window {
                until: now + i64::from(DEFAULT_WINDOW_MINUTES) * 60,
            },
        };

        // The state is installed *before* the tasks, so a probe that arrives
        // between the two finds an open listener rather than a closed one.
        {
            let mut guard = self.state.lock().await;
            let (pushes, last_push_at) = guard
                .as_ref()
                .map(|s| (s.pushes, s.last_push_at))
                .unwrap_or((0, None));
            *guard = Some(State {
                mode,
                tcp_port: Some(port),
                pushes,
                last_push_at,
                seen: HashSet::new(),
            });
        }

        let udp_task = {
            let me = Arc::clone(self);
            let storage = storage.clone();
            let beacon = Arc::clone(&beacon);
            tokio::spawn(async move { me.serve_beacon(storage, name, beacon, port).await })
        };
        let tcp_task = {
            let me = Arc::clone(self);
            let storage = storage.clone();
            tokio::spawn(async move { me.serve_tcp(storage, tcp).await })
        };
        let expiry = match mode {
            ListenerMode::Window { until } => {
                let me = Arc::clone(self);
                let secs = (until - now).max(0) as u64;
                Some(tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                    me.stop().await;
                }))
            }
            _ => None,
        };

        // Assigning here drops any previous `Running`, which aborts its tasks —
        // so a restart cannot leave an old responder announcing a dead port.
        *self.running.lock().await = Some(Running {
            tcp: tcp_task,
            udp: udp_task,
            expiry,
        });

        Ok(self.status(now).await)
    }

    /// Close. Both sockets, and the nonce set with them.
    ///
    /// **The state is settled before the sockets are dropped, and the order is
    /// load-bearing.** A window closes on a completed push, so the call that
    /// closes it comes from *inside* the accept loop — and `Running::drop`
    /// aborts that very task. An abort lands at the next `.await`, so anything
    /// after the drop may simply never run: with the mutation second, a push
    /// that closed its own window left `pushes` unincremented and the mode
    /// still open, which is a listener reporting itself available on a socket
    /// it has just destroyed. Found by a test that pushed twice.
    pub async fn stop(&self) -> ListenerStatus {
        let status = {
            let mut guard = self.state.lock().await;
            if let Some(s) = guard.as_mut() {
                s.mode = ListenerMode::Off;
                s.tcp_port = None;
                s.seen.clear();
            }
            guard.as_ref().map(State::status).unwrap_or(ListenerStatus {
                mode: ListenerMode::Off,
                tcp_port: None,
                pushes: 0,
                last_push_at: None,
            })
        };
        // Taken and dropped *outside* the state lock: `Running::drop` aborts
        // tasks that may be waiting for it, and aborting under the lock they
        // want is how a shutdown deadlocks.
        let taken = self.running.lock().await.take();
        drop(taken);
        status
    }

    /// Answer probes for as long as the listener is open.
    async fn serve_beacon(
        &self,
        storage: Storage,
        name: String,
        beacon: Arc<dyn Beacon>,
        port: u16,
    ) {
        while let Some((bytes, from)) = beacon.recv().await {
            match self.answer_probe(&storage, &name, &bytes, port).await {
                Ok(Some(reply)) => {
                    if let Err(e) = beacon.send_to(&reply, from).await {
                        tracing::debug!(error = %e, "could not answer a rendezvous probe");
                    }
                }
                // **Silence, not a refusal.** A probe from something we are not
                // paired with gets no reply at all, so an unpaired scanner
                // cannot learn that anything is here — which is the whole of
                // "there is nothing to fingerprint" when the listener is the
                // only thing that answers.
                Ok(None) => {}
                Err(e) => tracing::debug!(error = %e, "unreadable rendezvous probe"),
            }
        }
    }

    /// The pure half of the beacon: bytes in, an optional reply out.
    ///
    /// Split out so the ladder's refusals are testable without a socket at all.
    async fn answer_probe(
        &self,
        storage: &Storage,
        name: &str,
        bytes: &[u8],
        port: u16,
    ) -> Result<Option<Vec<u8>>> {
        if !self.status(now_unix()).await.mode.is_open(now_unix()) {
            return Ok(None);
        }
        let hello: Hello = match serde_json::from_slice(bytes) {
            Ok(h) => h,
            // Not an error worth a `Diagnostic`: the fixed port is a broadcast
            // address and anything on the LAN may send to it.
            Err(_) => return Ok(None),
        };
        if hello.v != PROTOCOL_VERSION {
            return Ok(None);
        }
        let Some(device) = storage.paired_device(&hello.device_id).await? else {
            return Ok(None);
        };
        let reply = Here {
            v: PROTOCOL_VERSION,
            name: name.to_string(),
            tcp_port: port,
            mac: mac(&device.token, &here_challenge(&hello.nonce, port)),
        };
        Ok(Some(serde_json::to_vec(&reply)?))
    }

    async fn serve_tcp(&self, storage: Storage, tcp: TcpListener) {
        loop {
            let Ok((stream, peer)) = tcp.accept().await else {
                return;
            };
            match self.serve_push(&storage, stream, peer).await {
                Ok(report) => {
                    tracing::info!(
                        device_id = %report.device_id,
                        sidecars = report.pulled.len(),
                        "a paired reader pushed over the network"
                    );
                }
                Err(e) => tracing::debug!(error = %e, "a wireless push did not complete"),
            }
        }
    }

    /// One push session, start to finish.
    ///
    /// Public so a test can drive it over a loopback stream without racing an
    /// accept loop — the same reason `serve` takes a `UnixListener` in the
    /// daemon rather than a path.
    pub async fn serve_push(
        &self,
        storage: &Storage,
        stream: TcpStream,
        peer: SocketAddr,
    ) -> Result<WirelessReport> {
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);

        macro_rules! refuse {
            ($r:expr) => {{
                let r = $r;
                let _ = write
                    .write_all(&to_line(&Ack::refused(&r))?)
                    .await
                    .and(write.flush().await);
                return Err(r.into());
            }};
        }

        let open: Open = match read_json_line(&mut reader).await? {
            Some(o) => o,
            None => refuse!(WirelessRefusal::Malformed("no opening frame".into())),
        };
        if open.v != PROTOCOL_VERSION {
            refuse!(WirelessRefusal::UnsupportedVersion {
                theirs: open.v,
                ours: PROTOCOL_VERSION,
            });
        }

        let device: PairedDevice = match storage.paired_device(&open.device_id).await? {
            Some(d) => d,
            None => refuse!(WirelessRefusal::UnknownDevice),
        };
        if !verify(&device.token, &open_challenge(&open.nonce), &open.mac) {
            refuse!(WirelessRefusal::BadCredential);
        }
        {
            let mut guard = self.state.lock().await;
            let Some(state) = guard.as_mut() else {
                refuse!(WirelessRefusal::NotListening);
            };
            if !state.mode.is_open(now_unix()) {
                refuse!(WirelessRefusal::NotListening);
            }
            if !state.seen.insert(open.nonce.clone()) {
                refuse!(WirelessRefusal::ReplayedNonce);
            }
        }
        write.write_all(&to_line(&Ack::ok())?).await?;
        write.flush().await?;

        let report =
            receive_entries(storage, &device, &open.nonce, &mut reader, &mut write).await?;

        let now = now_unix();
        // The two stamps are separate on purpose. `last_wireless_at` is *this
        // reader reached us*, which a push always is; `last_synced_at` is
        // *everything it had is here*, which a push of some books is not —
        // migration `0020`'s argument, one transport over.
        storage
            .stamp_wireless_contact(&device.device_id, Some(&peer.ip().to_string()))
            .await?;
        {
            let mut guard = self.state.lock().await;
            if let Some(state) = guard.as_mut() {
                state.completed_push(now);
            }
        }

        // **The reader is told before the door is shut.** A window closes on a
        // completed session, and closing it aborts the accept loop this call is
        // running inside — so an ack written afterwards is an ack the reader
        // never receives, and the push looks failed on the device while having
        // fully succeeded here.
        write.write_all(&to_line(&Ack::ok())?).await?;
        write.flush().await?;

        if !self.status(now).await.mode.is_open(now) {
            self.stop().await;
        }
        Ok(report)
    }
}

/// Read entry frames until `Done`, importing each.
///
/// **The whole of the transfer, shared by both verbs**, and that sharing is the
/// point of stage 3 rather than an optimisation of it. Push and pull differ in
/// exactly one thing — who opened the connection — and after the handshake the
/// bytes move identically, reader to desktop, because writing to a device over
/// the wire is out of scope by decision. Two copies of this loop would be two
/// dialects of one protocol, which is what items 26–28 taught and what this
/// item was kept in one thread to avoid.
///
/// The order of checks is load-bearing: hash, then MAC, then parse. The MAC
/// covers the *hash*, so a body that does not match its declared hash is
/// refused without the credential being consulted at all, and nothing reaches
/// the Lua sandbox until both hold.
async fn receive_entries<R, W>(
    storage: &Storage,
    device: &PairedDevice,
    nonce: &str,
    reader: &mut BufReader<R>,
    write: &mut W,
) -> Result<WirelessReport>
where
    R: tokio::io::AsyncRead + Unpin + Send,
    W: tokio::io::AsyncWrite + Unpin + Send,
{
    let mut report = WirelessReport {
        device_id: device.device_id.clone(),
        ..Default::default()
    };
    loop {
        let frame: PushFrame = match read_json_line(reader).await? {
            Some(f) => f,
            None => {
                return Err(
                    WirelessRefusal::Malformed("the connection ended mid-session".into()).into(),
                );
            }
        };
        let PushFrame::Entry {
            name,
            len,
            sha256,
            mac: entry_mac,
        } = frame
        else {
            return Ok(report);
        };
        if len > MAX_SIDECAR_BYTES {
            let r = WirelessRefusal::TooLarge {
                len,
                max: MAX_SIDECAR_BYTES,
            };
            let _ = write.write_all(&to_line(&Ack::refused(&r))?).await;
            return Err(r.into());
        }
        let mut body = vec![0u8; len as usize];
        reader.read_exact(&mut body).await?;

        if sha256_hex(&body) != sha256 {
            let r = WirelessRefusal::Malformed(format!(
                "the body of {name} does not match its declared hash"
            ));
            let _ = write.write_all(&to_line(&Ack::refused(&r))?).await;
            return Err(r.into());
        }
        if !verify(&device.token, &body_challenge(nonce, &sha256), &entry_mac) {
            let r = WirelessRefusal::BadCredential;
            let _ = write.write_all(&to_line(&Ack::refused(&r))?).await;
            return Err(r.into());
        }
        let Ok(src) = String::from_utf8(body) else {
            let r = WirelessRefusal::Malformed(format!("{name} is not UTF-8"));
            let _ = write.write_all(&to_line(&Ack::refused(&r))?).await;
            return Err(r.into());
        };

        // Here, and this is the point of the whole item: the bytes go into
        // exactly the import a cable would have fed.
        match import_book_from_sidecar_src(storage, &src, &PathBuf::from(&name)).await {
            Ok(pull) => {
                report.warnings.extend(pull.warnings.iter().cloned());
                report.pulled.push(pull);
                write.write_all(&to_line(&Ack::ok())?).await?;
            }
            // One bad sidecar does not end a session carrying thirty good ones
            // — the provider rule (`degrade, never abort`) applied to a
            // transport. The far side is told, and the next frame is read.
            Err(e) => {
                report
                    .warnings
                    .push(Diagnostic::sidecar_unreadable(PathBuf::from(&name), &e));
                write
                    .write_all(&to_line(&Ack {
                        ok: false,
                        error: Some(e.to_string()),
                    })?)
                    .await?;
            }
        }
        write.flush().await?;
    }
}

// ---- the seeker: pull (item 15b, stage 3) ----------------------------------

/// Find a reader whose window is open, and take what it has.
///
/// **The rendezvous, run the other way round, with no new message in it.** The
/// desktop broadcasts the same `HELLO`; a reader with its window open answers
/// the same `HERE`; the desktop verifies the same MAC before connecting and
/// sends the same `OPEN`. The only thing that swaps is who dials — and the
/// entries still travel reader → desktop, so [`receive_entries`] is literally
/// the same function.
///
/// The spec proposed an unsolicited **beacon** here instead, so the devices
/// page could show *ready* with no probe. That was refused and the reason is
/// worth keeping: an announcement nobody challenged carries no fresh nonce, so
/// it can only sign something the reader chose, which makes it **replayable** —
/// and it would be a fourth message type serving a fifth. One datagram sent
/// when somebody asks is not a scan, and it keeps *the seeker verifies identity
/// before sending a byte* true in both directions.
pub async fn pull_from(
    storage: &Storage,
    device: &PairedDevice,
    beacon: &dyn Beacon,
    broadcast: SocketAddr,
    nonce: &str,
) -> Result<WirelessReport> {
    let hello = Hello {
        v: PROTOCOL_VERSION,
        device_id: device.device_id.clone(),
        nonce: nonce.to_string(),
    };
    beacon
        .send_to(&serde_json::to_vec(&hello)?, broadcast)
        .await?;

    // More than one answer is ordinary on a broadcast — other readingbuddys and
    // other readers are entitled to be on the LAN — so this reads until one
    // *proves the token* rather than trusting whoever replied first. That is
    // the property the whole design is built on: identity, not address.
    let (addr, here) = loop {
        let Some((bytes, from)) = beacon.recv().await else {
            return Err(WirelessRefusal::ReaderNotFound.into());
        };
        if let Ok(here) = serde_json::from_slice::<Here>(&bytes)
            && here.v == PROTOCOL_VERSION
            && verify(
                &device.token,
                &here_challenge(nonce, here.tcp_port),
                &here.mac,
            )
        {
            break (from, here);
        }
    };

    let target = SocketAddr::new(addr.ip(), here.tcp_port);
    let stream = TcpStream::connect(target).await?;
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);

    write
        .write_all(&to_line(&Open {
            v: PROTOCOL_VERSION,
            device_id: device.device_id.clone(),
            nonce: nonce.to_string(),
            mac: mac(&device.token, &open_challenge(nonce)),
        })?)
        .await?;
    write.flush().await?;

    match read_json_line::<Ack, _>(&mut reader).await? {
        Some(a) if a.ok => {}
        Some(a) => {
            return Err(WirelessRefusal::Malformed(
                a.error.unwrap_or_else(|| "the reader refused".into()),
            )
            .into());
        }
        None => return Err(WirelessRefusal::Malformed("the reader said nothing".into()).into()),
    }

    let report = receive_entries(storage, device, nonce, &mut reader, &mut write).await?;
    // A pull is *this reader reached us* exactly as a push is — it came down
    // the same wire and the address is as good a breadcrumb either way. It is
    // still not a sync: what the reader had open to give is not everything it
    // has, which is migration `0020`'s argument and does not change with the
    // direction of the dial.
    storage
        .stamp_wireless_contact(&device.device_id, Some(&addr.ip().to_string()))
        .await?;
    Ok(report)
}

fn to_line<T: serde::Serialize>(v: &T) -> Result<Vec<u8>> {
    // `serde_json::to_string` escapes newlines inside strings, so a diagnostic
    // full of them cannot break a frame — the daemon's guarantee, and it lives
    // in the same function as the terminator here for the same reason.
    let mut s = serde_json::to_vec(v)?;
    s.push(b'\n');
    Ok(s)
}

async fn read_json_line<T: serde::de::DeserializeOwned, R>(
    reader: &mut BufReader<R>,
) -> Result<Option<T>>
where
    R: tokio::io::AsyncRead + Unpin + Send,
{
    let mut line = String::new();
    let mut limited = reader.take(MAX_HEADER_BYTES);
    let n = limited.read_line(&mut line).await?;
    if n == 0 {
        return Ok(None);
    }
    Ok(serde_json::from_str(line.trim_end()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// The vector pinned in `docs/spec-15b-the-wireless-link.md`, computed by
    /// **koreader-base's own `ffi/sha2.lua`** under lua5.4 — not by us, and then
    /// agreed with by us. `partial_md5.rs`'s `agrees_with_the_device` is the
    /// pattern: a checksum we compute and then assert against our own output
    /// proves only that we are consistent.
    ///
    /// The value that would be wrong is not a typo, it is a *decision*: the
    /// same library given the 32 bytes this hex encodes answers
    /// `26ae9d25…11a599fe`, which is equally valid HMAC-SHA256 and would fail
    /// on hardware with nothing to look at. Both are recorded so the next
    /// reader can see that the choice was made rather than stumbled into.
    #[test]
    fn our_hmac_agrees_with_the_device_and_uses_the_hex_as_the_key() {
        assert_eq!(
            mac(TOKEN, "nonce-0001"),
            "a38b20860c54b13a06e1fac37207b4ca1120db8946e82848818567a07d7a7cae",
            "this is `sha.hmac(sha.sha256, token, nonce)` on the device"
        );
        assert_ne!(
            mac(TOKEN, "nonce-0001"),
            "26ae9d255e7a30de01b32de4544430f32a4bdf4dea6907f07c3c604311a599fe",
            "that is the same function keyed with the decoded bytes"
        );
        assert!(verify(TOKEN, "nonce-0001", &mac(TOKEN, "nonce-0001")));
        assert!(!verify(
            "another-token",
            "nonce-0001",
            &mac(TOKEN, "nonce-0001")
        ));
        // Not hex, wrong length, empty: none of them may panic or pass.
        for bad in ["", "zz", "a38b2086", "not-hex-at-all"] {
            assert!(!verify(TOKEN, "nonce-0001", bad), "{bad}");
        }
    }

    /// The three prefixes exist so one MAC can never be presented as another.
    #[test]
    fn a_discovery_reply_cannot_be_replayed_as_an_authorisation_to_push() {
        let here = mac(TOKEN, &here_challenge("n", 51000));
        let open = mac(TOKEN, &open_challenge("n"));
        let body = mac(TOKEN, &body_challenge("n", "abc"));
        assert_ne!(here, open);
        assert_ne!(open, body);
        assert!(!verify(TOKEN, &open_challenge("n"), &here));
        // The port is inside the `here` challenge, so a captured reply cannot
        // be re-advertised pointing at a different port.
        assert!(!verify(TOKEN, &here_challenge("n", 51001), &here));
    }

    #[test]
    fn a_window_closes_on_time_and_always_does_not() {
        let now = 1_700_000_000;
        assert!(!ListenerMode::Off.is_open(now));
        assert!(ListenerMode::Always.is_open(now + 999_999));
        let w = ListenerMode::Window { until: now + 60 };
        assert!(w.is_open(now));
        assert!(w.is_open(now + 59));
        assert!(!w.is_open(now + 60), "the boundary is closed, not open");
        assert!(!w.is_open(now + 61));
    }

    #[test]
    fn a_completed_push_closes_a_window_and_leaves_always_alone() {
        let mut s = State {
            mode: ListenerMode::Window {
                until: 1_700_000_060,
            },
            tcp_port: Some(1),
            pushes: 0,
            last_push_at: None,
            seen: HashSet::new(),
        };
        s.completed_push(1_700_000_001);
        assert_eq!(s.mode, ListenerMode::Off);
        assert_eq!(s.pushes, 1);
        assert_eq!(s.last_push_at, Some(1_700_000_001));

        s.mode = ListenerMode::Always;
        s.completed_push(1_700_000_002);
        assert_eq!(
            s.mode,
            ListenerMode::Always,
            "a desktop that lives on the LAN does not close after one reader"
        );
        assert_eq!(s.pushes, 2);
    }
}
