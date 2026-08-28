//! The transport, and nothing else.
//!
//! Every function here moves bytes. **None of them names an API method**, and
//! that is the property to preserve: the moment this file branches on
//! `Request`, the daemon has logic, and iOS — which links
//! [`readingbuddy_api::Api`] in-process with no socket at all — starts getting
//! different behaviour from the same call.
//!
//! ## Why a unix socket and newline-delimited JSON
//!
//! The daemon serves **this machine's own user**, which is what
//! `docs/decisions.md` describes: a Tauri app, a menu-bar companion, a widget's
//! snapshot writer. A unix socket is the transport that matches that scope
//! exactly — it has no port to collide, no interface to accidentally expose, and
//! filesystem permissions are its access control. HTTP would mean a server, a
//! router and a middleware stack in the dependency tree for a protocol with one
//! endpoint, and would put a listening TCP port on a laptop for no gain.
//!
//! ## "For no gain" was overturned by item 15b, and only that clause
//!
//! There is now a gain, and it is the one thing a unix socket structurally
//! cannot do: **a reader on the LAN is not on this machine**, so a paired Kobo
//! pushing its highlights has no filesystem to be given permission on. That is
//! not a better way to serve the API; it is a different peer.
//!
//! So the sentence above stands for *this* transport and the two rules that
//! follow it are what keep the overturning narrow.
//!
//! **The wireless listener is not here.** It lives in the engine
//! (`readingbuddy::wireless`), is `Off` by default, and is turned on through
//! the ordinary API — `StartListening` — which means every host reaches it the
//! same way and the GUI, which links `readingbuddy-api` with no daemon
//! anywhere, is not locked out of the feature. This file grew **no second
//! listener**; `--listen` on the binary is one call to that request before
//! `bind`, and it is the *lifecycle* exception `daemon/CLAUDE.md` already
//! carves for the vault watcher rather than a new one.
//!
//! **It speaks a different protocol on purpose, and must never accept
//! [`Call`].** A reader is not a trusted local client. Three closed messages
//! serve two verbs; exposing sixty methods to a LAN peer to do it would be the
//! exact inverse of the rule below — that the transport names no method — since
//! the whole reason that rule is safe here is that nothing untrusted can reach
//! this socket.
//!
//! Framing is one JSON object per line. `serde_json::to_string` escapes
//! newlines inside strings, so a note body full of them cannot break a frame —
//! and [`Reply::to_line`] is where that guarantee and the terminator live
//! together.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use readingbuddy_api::{Api, ApiError, Call, Reply};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// The longest line the daemon will read before giving up on a connection.
///
/// Not a tuning knob. Without it a client — or anything that manages to write
/// to the socket — can send bytes with no newline for ever and the daemon
/// grows a buffer until it is killed. 8 MiB is far above any real call: the
/// largest thing a client sends is a note body, and the largest thing it
/// *receives* (a Goodreads CSV, a whole calibre library) is not bounded by this
/// at all because it travels the other way.
///
/// A line that hits the cap is unrecoverable rather than merely refused: the
/// remainder of it is still in the stream and would be read as the next call,
/// so the connection is closed after the error rather than resynchronised.
pub const MAX_LINE: usize = 8 * 1024 * 1024;

/// Bind the socket, with the permissions a private library deserves.
///
/// Three things happen here that are not `UnixListener::bind`:
///
/// - **A live daemon is refused, a dead one is cleaned up.** `bind` fails with
///   `AddrInUse` on any existing path, whether or not anything is listening —
///   and a socket file outlives the process that made it if it was killed. So
///   the path is probed by connecting: an answer means somebody is serving and
///   we must not steal their address; a refusal means the file is a corpse.
/// - **Mode 0600.** The database behind this socket is the user's private
///   reading, and on Linux a unix socket honours its file permissions. There is
///   a window between `bind` and `chmod` in which the socket carries the process
///   umask; closing it properly means an anonymous socket in a 0700 directory,
///   which is a bigger change than this earns today.
/// - The parent directory is created, so `--socket` can name somewhere that
///   does not exist yet.
pub async fn bind(path: &Path) -> anyhow::Result<UnixListener> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        match UnixStream::connect(path).await {
            Ok(_) => anyhow::bail!(
                "a readingbuddyd is already listening on {} — stop it, or pass --socket",
                path.display()
            ),
            Err(_) => {
                tracing::warn!(socket = %path.display(), "removing a stale socket");
                std::fs::remove_file(path)?;
            }
        }
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Removes the socket file on drop, including on a panic.
///
/// A daemon that leaves its address behind makes the *next* start take the
/// stale-socket path above, which works but logs a warning about a situation
/// nobody caused. Doing it in `Drop` rather than at the end of `serve` is what
/// covers the panic.
pub struct SocketGuard(pub PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

/// Accept until `shutdown` resolves.
///
/// Takes the listener rather than a path so a test can bind its own in a
/// tempdir — the same reason `watch.rs` takes a channel rather than owning a
/// `notify` watcher. A transport that can only be exercised by starting a real
/// daemon is a transport with no tests.
pub async fn serve<S>(api: Api, listener: UnixListener, shutdown: S) -> anyhow::Result<()>
where
    S: std::future::Future<Output = ()>,
{
    let api = Arc::new(api);
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            // Biased, so a shutdown asked for while connections are queueing is
            // taken now rather than after however many are already waiting.
            biased;
            () = &mut shutdown => {
                tracing::info!("shutting down");
                return Ok(());
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _)) => {
                        let api = Arc::clone(&api);
                        // One task per connection. In-flight work is dropped at
                        // shutdown along with the runtime, which is correct for
                        // reads and acceptable for writes: every engine write
                        // is a committed transaction or nothing.
                        tokio::spawn(async move {
                            if let Err(e) = handle(&api, stream).await {
                                tracing::warn!(error = %e, "connection ended badly");
                            }
                        });
                    }
                    // One failed accept is not a reason to stop serving —
                    // EMFILE clears when a connection closes.
                    Err(e) => tracing::warn!(error = %e, "accept failed"),
                }
            }
        }
    }
}

/// One connection: read a line, reply with a line, repeat.
///
/// The whole of the daemon's decision-making, and there are only three
/// decisions in it — none of them about what a call *means*.
async fn handle(api: &Api, stream: UnixStream) -> io::Result<()> {
    let (rx, mut tx) = stream.into_split();
    let mut reader = BufReader::new(rx);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match read_line_capped(&mut reader, &mut buf, MAX_LINE).await? {
            Line::Eof => return Ok(()), // client hung up
            Line::TooLong => {
                let reply = Reply::err(
                    0,
                    ApiError::bad_request(format!("request exceeded {MAX_LINE} bytes")),
                );
                tx.write_all(reply.to_line().as_bytes()).await?;
                tx.flush().await?;
                // The rest of that line is still in the stream and would be
                // read as the next call, so there is nothing to resynchronise
                // to.
                return Ok(());
            }
            Line::Read => {}
        }
        let line = String::from_utf8_lossy(&buf);
        if line.trim().is_empty() {
            continue;
        }

        let reply = match serde_json::from_str::<Call>(&line) {
            // The one and only place the daemon touches the vocabulary, and it
            // hands the whole thing over without looking inside.
            Ok(call) => api.call(call).await,
            // A malformed line is recoverable: the framing is intact, so the
            // next line is still a call. Id 0 because there was no id to echo.
            Err(e) => Reply::err(0, ApiError::bad_request(e.to_string())),
        };
        tx.write_all(reply.to_line().as_bytes()).await?;
        tx.flush().await?;
    }
}

enum Line {
    Read,
    /// The peer closed with nothing pending.
    Eof,
    /// [`MAX_LINE`] bytes went by without a newline.
    TooLong,
}

/// `read_line` with a ceiling.
///
/// Hand-rolled because neither of the ready-made options is both: tokio's
/// `read_line`/`read_until` grow without bound, and wrapping the reader in a
/// `Take` caps the *reader* rather than the line — a `Take` is `AsyncRead` but
/// not `AsyncBufRead`, so `read_line` cannot be called on it at all, and
/// capping the reader would silently split one overlong line into two calls
/// instead of noticing it.
///
/// Bytes are handed over as read, terminator included, and a truncated final
/// line with no newline is still delivered — a client that hangs up
/// mid-message gets its last call answered rather than dropped.
async fn read_line_capped<R>(reader: &mut R, buf: &mut Vec<u8>, max: usize) -> io::Result<Line>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(if buf.is_empty() {
                Line::Eof
            } else {
                Line::Read
            });
        }
        match available.iter().position(|&b| b == b'\n') {
            Some(i) => {
                buf.extend_from_slice(&available[..=i]);
                reader.consume(i + 1);
                return Ok(if buf.len() > max {
                    Line::TooLong
                } else {
                    Line::Read
                });
            }
            None => {
                let n = available.len();
                buf.extend_from_slice(available);
                reader.consume(n);
                if buf.len() > max {
                    return Ok(Line::TooLong);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use readingbuddy::{Engine, EngineConfig};
    use readingbuddy_api::{Request, Response};
    use tokio::io::AsyncReadExt;

    async fn test_api(root: &Path) -> Api {
        let config = EngineConfig {
            db_url: "sqlite::memory:".into(),
            images_dir: root.join("images"),
            files_dir: root.join("files"),
            vault_dir: root.join("vault"),
            log_dir: root.join("logs"),
            google_api_key: None,
            calibre_bin_dir: None,
        };
        Api::new(Arc::new(Engine::open(config).await.expect("engine")))
    }

    /// A client that speaks the protocol, so the tests exercise the framing
    /// rather than a mock of it.
    struct Client {
        reader: BufReader<tokio::net::unix::OwnedReadHalf>,
        writer: tokio::net::unix::OwnedWriteHalf,
        next_id: u64,
    }

    impl Client {
        async fn connect(path: &Path) -> Client {
            let (rx, writer) = UnixStream::connect(path)
                .await
                .expect("connect")
                .into_split();
            Client {
                reader: BufReader::new(rx),
                writer,
                next_id: 1,
            }
        }

        async fn send_raw(&mut self, line: &str) -> String {
            self.writer.write_all(line.as_bytes()).await.unwrap();
            self.writer.flush().await.unwrap();
            let mut out = String::new();
            self.reader.read_line(&mut out).await.unwrap();
            out
        }

        async fn call(&mut self, request: Request) -> Reply {
            let id = self.next_id;
            self.next_id += 1;
            let line = serde_json::to_string(&Call { id, request }).unwrap() + "\n";
            let raw = self.send_raw(&line).await;
            serde_json::from_str(raw.trim_end()).expect("a reply")
        }
    }

    /// Bind, serve, and hand back the socket path plus a shutdown switch.
    async fn spawn(root: &Path) -> (PathBuf, tokio::sync::oneshot::Sender<()>) {
        let sock = root.join("rb.sock");
        let listener = bind(&sock).await.expect("bind");
        let api = test_api(root).await;
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            serve(api, listener, async {
                stop_rx.await.ok();
            })
            .await
            .expect("serve");
        });
        (sock, stop_tx)
    }

    #[tokio::test]
    async fn a_call_comes_back_with_its_own_id() {
        let tmp = tempfile::tempdir().unwrap();
        let (sock, _stop) = spawn(tmp.path()).await;
        let mut c = Client::connect(&sock).await;

        let reply = c.call(Request::ApiVersion).await;
        assert_eq!(reply.id, 1);
        assert_eq!(reply.api_version, readingbuddy_api::API_VERSION);
        match reply.outcome {
            readingbuddy_api::Outcome::Ok {
                response: Response::Version { api, .. },
            } => assert_eq!(api, readingbuddy_api::API_VERSION),
            other => panic!("{other:?}"),
        }

        // And the connection is still good for the next one — this is a stream
        // of calls, not one request per connect.
        let reply = c
            .call(Request::ListBooks {
                limit: 10,
                sort: Default::default(),
                offset: 0,
                filter: None,
            })
            .await;
        assert_eq!(reply.id, 2);
    }

    /// The round trip that proves the seam works end to end: write through the
    /// socket, read back through the socket, and get the row that was written.
    #[tokio::test]
    async fn a_book_written_over_the_socket_reads_back_over_it() {
        let tmp = tempfile::tempdir().unwrap();
        let (sock, _stop) = spawn(tmp.path()).await;
        let mut c = Client::connect(&sock).await;

        let reply = c
            .call(Request::SaveBook {
                book: readingbuddy_api::BookDto {
                    title: Some("Station Eleven".into()),
                    authors: vec!["Emily St. John Mandel".into()],
                    isbn_13: Some("9781447268963".into()),
                    ..Default::default()
                },
            })
            .await;
        let id = match reply.outcome {
            readingbuddy_api::Outcome::Ok {
                response: Response::Book(Some(b)),
            } => b.id.expect("saved book has an id"),
            other => panic!("{other:?}"),
        };

        let reply = c.call(Request::GetBook { id }).await;
        match reply.outcome {
            readingbuddy_api::Outcome::Ok {
                response: Response::Book(Some(b)),
            } => {
                assert_eq!(b.title.as_deref(), Some("Station Eleven"));
                assert_eq!(b.isbn_13.as_deref(), Some("9781447268963"));
            }
            other => panic!("{other:?}"),
        }
    }

    /// An engine error must arrive as a *code*, not as prose the client has to
    /// scrape. This is the whole promise of the error taxonomy.
    #[tokio::test]
    async fn an_engine_error_arrives_as_a_code() {
        let tmp = tempfile::tempdir().unwrap();
        let (sock, _stop) = spawn(tmp.path()).await;
        let mut c = Client::connect(&sock).await;

        let reply = c
            .call(Request::UpdateProgress {
                book_id: 4242,
                page: Some(10),
                finished: None,
            })
            .await;
        match reply.outcome {
            readingbuddy_api::Outcome::Error { error } => {
                assert_eq!(error.code, readingbuddy_api::ErrorCode::NotFound);
            }
            other => panic!("{other:?}"),
        }
    }

    /// Garbage on the wire must not cost the connection: framing is intact, so
    /// the next line is still a call.
    #[tokio::test]
    async fn a_malformed_line_is_answered_and_the_connection_survives() {
        let tmp = tempfile::tempdir().unwrap();
        let (sock, _stop) = spawn(tmp.path()).await;
        let mut c = Client::connect(&sock).await;

        let raw = c.send_raw("{not json at all\n").await;
        let reply: Reply = serde_json::from_str(raw.trim_end()).unwrap();
        match reply.outcome {
            readingbuddy_api::Outcome::Error { error } => {
                assert_eq!(error.code, readingbuddy_api::ErrorCode::BadRequest)
            }
            other => panic!("{other:?}"),
        }
        // A method this build does not have is the same kind of failure — and
        // it keeps its id, because the envelope parsed even though the method
        // did not.
        let raw = c
            .send_raw("{\"id\":7,\"request\":{\"method\":\"invented_later\"}}\n")
            .await;
        let reply: Reply = serde_json::from_str(raw.trim_end()).unwrap();
        assert!(matches!(
            reply.outcome,
            readingbuddy_api::Outcome::Error { .. }
        ));

        // Still alive.
        assert_eq!(c.call(Request::ApiVersion).await.id, 1);
    }

    /// An unterminated flood must not grow a buffer until the daemon dies.
    #[tokio::test]
    async fn an_overlong_line_is_refused_and_the_connection_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let (sock, _stop) = spawn(tmp.path()).await;
        let (rx, mut tx) = UnixStream::connect(&sock).await.unwrap().into_split();
        let mut reader = BufReader::new(rx);

        // **Exactly** one byte past the cap, and then silence. That is what
        // makes the explanatory reply observable at all: the daemon consumes
        // every byte before it decides, so nothing is left unread when it
        // closes. Overshoot instead — flood it, as `a_flood_costs_the_flooder`
        // below does — and the close happens with data still in the receive
        // queue, which on Linux is an RST that discards the reply the client
        // had already been sent. Two properties, two tests, because one of them
        // is only guaranteed on a peer that stops talking.
        let payload = vec![b' '; MAX_LINE + 1];
        tx.write_all(&payload).await.unwrap();
        tx.flush().await.unwrap();

        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let reply: Reply = serde_json::from_str(line.trim_end()).unwrap();
        match reply.outcome {
            readingbuddy_api::Outcome::Error { error } => {
                assert_eq!(error.code, readingbuddy_api::ErrorCode::BadRequest);
                assert!(error.message.contains("exceeded"), "{}", error.message);
            }
            other => panic!("{other:?}"),
        }
        // And then the daemon hangs up rather than trying to resynchronise:
        // the rest of that line would otherwise be read as the next call.
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty(), "expected the connection to be closed");
    }

    /// The property the cap actually exists for: a peer that never sends a
    /// newline costs itself its connection and costs the daemon nothing.
    ///
    /// Asserted by serving somebody else afterwards, rather than by what the
    /// flooder receives — a connection closed with unread bytes still queued is
    /// reset rather than drained, so on Linux the flooder's own read fails
    /// instead of returning the error reply. That is the kernel's call, not the
    /// daemon's, and pinning it would be pinning the platform.
    #[tokio::test]
    async fn a_flood_costs_the_flooder_and_nobody_else() {
        let tmp = tempfile::tempdir().unwrap();
        let (sock, _stop) = spawn(tmp.path()).await;
        let (_rx, mut tx) = UnixStream::connect(&sock).await.unwrap().into_split();

        let chunk = vec![b' '; 1024 * 1024];
        for _ in 0..12 {
            // Once the daemon has given up, our writes start failing. That is
            // the connection being closed, which is the point.
            if tx.write_all(&chunk).await.is_err() {
                break;
            }
        }

        let mut good = Client::connect(&sock).await;
        assert!(matches!(
            good.call(Request::ApiVersion).await.outcome,
            readingbuddy_api::Outcome::Ok { .. }
        ));
    }

    /// Two daemons on one socket would each get an arbitrary half of the
    /// connections, so the second must refuse rather than take the address.
    #[tokio::test]
    async fn a_second_daemon_refuses_a_live_socket() {
        let tmp = tempfile::tempdir().unwrap();
        let (sock, _stop) = spawn(tmp.path()).await;
        // Something has to have connected at least once for the listener to be
        // demonstrably live; `bind`'s probe does that itself.
        let err = bind(&sock).await.expect_err("must refuse");
        assert!(err.to_string().contains("already listening"), "{err}");
    }

    /// A killed daemon leaves its socket file behind. The next start must clear
    /// it, or the daemon can never be restarted without a manual `rm`.
    #[tokio::test]
    async fn a_stale_socket_is_cleared() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("rb.sock");
        // A socket file with nothing behind it, exactly as a SIGKILL leaves.
        drop(UnixListener::bind(&sock).unwrap());
        assert!(sock.exists());
        let listener = bind(&sock).await.expect("stale socket should be cleared");
        drop(listener);
    }

    #[tokio::test]
    async fn the_socket_is_not_world_readable() {
        let tmp = tempfile::tempdir().unwrap();
        let (sock, _stop) = spawn(tmp.path()).await;
        let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "got {mode:o}");
    }

    #[tokio::test]
    async fn the_guard_takes_the_socket_with_it() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("rb.sock");
        {
            let _l = bind(&sock).await.unwrap();
            let _guard = SocketGuard(sock.clone());
            assert!(sock.exists());
        }
        assert!(!sock.exists());
    }
}
