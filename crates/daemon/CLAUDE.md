# crates/daemon (bin `readingbuddyd`)

Moved here from the root `CLAUDE.md` unchanged.

**If a feature is being added here, it is in the wrong crate.** The logic lives
in [`../api/CLAUDE.md`](../api/CLAUDE.md); this is transport.

the transport and **nothing else**; if a feature is ever added here it is in the wrong crate. Unix socket, one JSON object per line, `Api::call` per line, no branch on any method name anywhere. Zero new third-party dependencies — HTTP would have meant a server, a router and a middleware stack for a protocol with one endpoint, and a listening TCP port on a laptop. `bind` **probes before it takes an address**: `UnixListener::bind` fails on any existing path, so a live daemon is refused and a corpse (what a SIGKILL leaves) is removed, and the socket is `chmod 0600` because the database behind it is the user's private reading. `MAX_LINE` (8 MiB) is not tuning — without it anything that can write to the socket grows a buffer until the daemon is killed; a line that hits it closes the connection rather than resynchronising, because the rest of that line would be read as the next call. `serve` takes the `UnixListener` rather than a path, the same seam `watch.rs` took a channel for, which is what makes all nine tests run with no daemon to start.

**The vault watcher is the one thing `main` drives that is not transport, and it
does not break the rule above (item 24).** It is *lifecycle*, not logic: a
watcher is a long-lived thing a host owns, and every host owns its own — the
TUI's is a `select!` arm, the GUI's is a `tauri::async_runtime` task, this one is
a `tokio::spawn`, and the CLI cannot have one at all because every command is its
own process. Nothing is decided here; the re-index happens inside
`VaultWatcher::next`, which is precisely why this daemon needs **no wire-protocol
change** to keep a client's note searches correct. Had that write stayed on the
frontend's side, this is the file that would have had to grow server-initiated
frames — and every reply here carries the id it answers, so a push has no id to
carry. `Engine::reconcile_vault` runs once before `bind`, for the edits made
while nothing was running. Both degrade to a `warn!`: a daemon that cannot watch
a vault is still a daemon.

**Item 15b overturned exactly one clause of the argument above, and `server.rs` carries the new one in prose.** "A listening TCP port on a laptop **for no gain**" — there is now a gain, and it is the thing a unix socket structurally cannot do: a reader on the LAN has no filesystem to be given permission on. Two rules keep the overturning narrow. **The wireless listener is not in this crate**: it is `readingbuddy::wireless`, `Off` by default, turned on through the ordinary `StartListening` request — so every host reaches it the same way and the GUI, which has no daemon, is not locked out. `--listen` is **one call to that request before `bind`**, not a second listener, and it is the *lifecycle* exception already carved for the vault watcher rather than a new one; it degrades to a `warn!` for that watcher's reason, since a daemon that could not take the rendezvous port is still a daemon and the unix socket must not be hostage to a feature that is off by default. And **it speaks a different protocol on purpose and must never accept a `Call`** — three closed messages for two verbs, because the rule that this transport names no method is only safe *because* nothing untrusted can reach this socket.

Run it: `cargo run -p readingbuddyd -- --data-dir .` — listens on
`<data-dir>/readingbuddyd.sock`. Poke it with
`printf '{"id":1,"request":{"method":"list_books","params":{"limit":5}}}\n' | nc -U ./readingbuddyd.sock`.
One JSON object per line, each reply carrying the id it answers.
