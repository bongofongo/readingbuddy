# crates/daemon (bin `readingbuddyd`)

Moved here from the root `CLAUDE.md` unchanged.

**If a feature is being added here, it is in the wrong crate.** The logic lives
in [`../api/CLAUDE.md`](../api/CLAUDE.md); this is transport.

the transport and **nothing else**; if a feature is ever added here it is in the wrong crate. Unix socket, one JSON object per line, `Api::call` per line, no branch on any method name anywhere. Zero new third-party dependencies — HTTP would have meant a server, a router and a middleware stack for a protocol with one endpoint, and a listening TCP port on a laptop. `bind` **probes before it takes an address**: `UnixListener::bind` fails on any existing path, so a live daemon is refused and a corpse (what a SIGKILL leaves) is removed, and the socket is `chmod 0600` because the database behind it is the user's private reading. `MAX_LINE` (8 MiB) is not tuning — without it anything that can write to the socket grows a buffer until the daemon is killed; a line that hits it closes the connection rather than resynchronising, because the rest of that line would be read as the next call. `serve` takes the `UnixListener` rather than a path, the same seam `watch.rs` took a channel for, which is what makes all nine tests run with no daemon to start.

Run it: `cargo run -p readingbuddyd -- --data-dir .` — listens on
`<data-dir>/readingbuddyd.sock`. Poke it with
`printf '{"id":1,"request":{"method":"list_books","params":{"limit":5}}}\n' | nc -U ./readingbuddyd.sock`.
One JSON object per line, each reply carrying the id it answers.
