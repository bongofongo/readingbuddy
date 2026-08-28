# Prompt — Item 15b: the wireless link

Paste into a fresh Claude Code thread at the repo root, in its own worktree
(`feat/koreader-wireless-link`).

---

Read `docs/spec-15b-the-wireless-link.md` first — it is the design and this
prompt is the boundary around it. Then `docs/decisions.md` entry 15 (slice a,
what building it changed) and `docs/spec-11-16.md` item 15. `CLAUDE.md`'s
**Engine standards** section is binding, and `crates/engine/CLAUDE.md`,
`crates/api/CLAUDE.md` and `crates/daemon/CLAUDE.md` are the three crate files
this touches.

Owns migration **`0021`**. `0020` is the highest applied. Runs alone — no other
item is in flight, and this one must not be split across parallel threads (see
*One thread* below).

**Check your base before writing a line**: `git log --oneline -1` and
`ls crates/engine/migrations/ | tail -2`. If migrations do not stop at `0020`,
`git reset --hard main`. Four of six worktrees in the GUI wave were created ~80
commits behind and every thread caught it only because it was told to look.

## The item

A paired reader can send its annotations to its paired computer over the LAN,
and that computer can fetch them back the other way. Both verbs are a **tap**:
the user opens the plugin's menu entry and chooses *Push*, or clicks *Pull* on
the desktop's devices page. Nothing happens by itself.

Slice a left a reader holding a `device_id`, a 32-byte `token` and **no
endpoint**. The whole point of that token is that this item requires nothing
typed.

**Explicitly not in scope**, and an eager thread will want most of them:

- **no auto-push.** Not on `onAnnotationsModified`, not on `onCloseDocument`,
  not on a timer. Those hooks exist (`readerannotation.lua:515`, `:245`) and
  they are a later item that has earned them;
- **no writing to the reader over the wire.** Wireless is read-only toward us,
  the way mount → import is; every write to a device stays explicit and wired;
- no two-way sync, no statistics, no sending books to the reader;
- no TLS, no mDNS, no relay, no cloud, no port forwarding, nothing that works
  from a coffee shop;
- no TUI screen. CLI + the existing devices page only.

## One thread, three stages

Push and pull share one rendezvous protocol. Three agents on one protocol
produce three dialects of it, which is why items 26–28 were forbidden from
running in parallel. Do the stages in order in this thread:

1. **The plumbing, no network at all.** `pairing.lua` becomes a list of
   computers; `endpoint.lua` becomes a permitted runtime file; migration
   `0021`. Fully offline and fully testable.
2. **The listener and push.**
3. **Pull** — the reader's listening window and beacon, the desktop's seeker.

Stage 1 is worth landing even if 2 and 3 slip: it fixes a reader paired to two
computers being silently broken today.

## Three collisions with shipped code — read these before designing

Each is code that is currently correct and that this item breaks. The spec has
the full argument; this is the short form.

1. **A plugin that caches its endpoint next to itself bricks its own
   installer.** `plugin::inspect` (`plugin.rs:388-405`) skips only
   `installed.lua` and `pairing.lua`; anything else not in the manifest is
   `unrecognised`, and `refuse_if_obstructed` (`:411-434`) then fails **install
   and uninstall both**. Add a third skipped, never-hashed, device-writable
   `endpoint.lua` that uninstall removes — and test that both operations still
   work with one present.
2. **`pairing.lua` holds one computer.** `Engine::install_plugin_at`
   (`lib.rs:1226-1241`) mints a fresh id and token when *this* machine has no
   row for the id it reads, and overwrites the file — so a second readingbuddy
   install silently kills the first one's pairing. Make it a list; the installer
   updates **its own entry** and leaves the others alone.
3. **`crates/daemon` is unix-socket-only by a written argument**
   (`server.rs:9-21`): a TCP port on a laptop "for no gain". This item is the
   gain and the listener toggle is the mitigation. **Overturn it in that file,
   in prose, with the new argument** — do not grow a second listener quietly
   beside it.

## What the KOReader source settles

Read from a checked-out `koreader/master`. `docs/koreader-format.md` §1 makes
that the order of authority. Do not re-derive; do correct.

- **There is no mDNS, zeroconf or Bonjour anywhere in KOReader** — grep over
  `frontend`, `plugins`, `base` returns nothing. Discovery is broadcast, DNS, or
  typed. This is the single fact that shapes the ladder.
- **Broadcast discovery has a working precedent on these devices**:
  `plugins/calibre.koplugin/wireless.lua:119-139` — `socket.udp4()`,
  `setoption("broadcast", true)`, `setsockname("*", port)`,
  `sendto("255.255.255.255", port)`, `receivefrom()`, 3s timeout. Copy the shape.
- **KOReader can be a server, from core**:
  `frontend/ui/message/simpletcpserver.lua` (LuaSocket, `socket.bind`,
  `settimeout(0.01)`, headers to a blank line), and
  `plugins/httpinspector.koplugin` runs exactly it. That is the pull side; also
  copy its `onEnterStandby` handling.
- **HMAC-SHA256 is on the device**: koreader-base `ffi/sha2.lua` exports `hmac`
  beside `sha256` and `bin_to_hex`. So the token is a challenge-response
  credential and **never crosses the wire**, and the reader verifies *identity,
  not address* before sending a byte.
- **Non-blocking HTTP exists**: `frontend/httpasync.lua` (coroutines, TLS via
  `ssl`), and `frontend/socketutil.lua:8` for the blocking one with timeouts.
  The UI must never block — `httpasync` or `Trapper:dismissableRunInSubprocess`,
  never a bare `socket.http`.
- **Wifi is off by default and transient**: `NetworkMgr:runWhenOnline`
  (`manager.lua:698`), `turnOnWifiAndWaitForConnection` (`:517`),
  `beforeWifiAction`/`afterWifiAction`. Fail closed and silently when the radio
  is down; never block the reader's UI on it.
- **`sorting_hint` must name a real menu id or KOReader crashes** —
  `menusorter.lua:180-181` indexes the result of `findById`. `"tools"` resolves
  in both order tables and is what 15a already uses. Do not change it.

## The surface

Engine — the rendezvous and the listener are the engine's; the *transport* is
injected so tests never emit a packet:

```rust
pub enum ListenerMode { Off, Window { until: i64 }, Always }

pub async fn listener_status(&self) -> Result<ListenerStatus>;
pub async fn start_listening(&self, minutes: Option<u32>) -> Result<ListenerStatus>;
pub async fn stop_listening(&self) -> Result<ListenerStatus>;
pub async fn pull_from_reader(&self, device_id: &str) -> Result<PullReport>;
```

API: **new `Request` variants only** — `ListenerStatus` / `StartListening` /
`StopListening` / `PullFromReader`. `API_VERSION` stays **2**. Do **not** add a
field to an existing variant: `ts-rs` emits a new field as required TypeScript
however `#[serde(default)]` the Rust is, which breaks
`gui/src/lib/api/client.ts` invisibly — adding to a *response* DTO is the safe
direction (item 55's finding). Run `make ts` and commit `bindings.ts`.

**The wireless listener must never accept `readingbuddy_api::Call`.** It speaks
three messages of its own and maps them onto engine calls internally. A reader
is not a trusted local client, and `server.rs`'s own rule is that the transport
names no method.

**The payload is the sidecar bytes the reader already has**, not a new wire
format. `koreader.rs` parses that format, it is fuzzed, the import is
idempotent, the goldens cover it, and both corpora are made of it. A second
format is a second parser for the same information, and its first divergence
shows up as highlights that import differently depending on which cable they
came down. If you think this is wrong, say so before building the alternative.

CLI: extend `rb ko plugin` in the tone `commands/ko.rs` uses — when something
refuses, print the next move rather than a bare error.

## What each refusal and each rung is worth as a test

- The **ladder's ordering and fallback**, with a scripted resolver. This is the
  part with bugs in it.
- **No test may emit a broadcast packet.** Put the rendezvous transport behind a
  trait and inject it, exactly as `watch.rs` injects `MountStir`s rather than
  driving `notify` — "a watcher that can only be driven by plugging in real
  hardware is a watcher with no tests" is the same sentence about discovery.
- Loopback **is** allowed: `tests/provider_http.rs` already binds a local port
  via `wiremock`. "No network in tests, ever" means nothing leaves the machine.
- **The listener state machine** under `#[tokio::test(start_paused = true)]`:
  window expiry, close-on-first-push, and the UDP responder dying with the TCP
  listener. An announcer that outlives its listener is a device telling the user
  to retry against a closed port.
- **An HMAC vector pinned on both sides of the language boundary** — same key,
  same nonce, `ffi/sha2` and our Rust agreeing.
- **The token is never logged at any level**, not even `trace!`
  (`0019` says so). Assert it in `tests/tracing_redaction.rs`.
- **A rogue responder is refused**: an opener that cannot prove the token gets
  no bytes.
- Install and uninstall still work with an `endpoint.lua` present, and the
  whole-tree snapshot property from 15a still holds.

### The Lua stops being untested here

`main.lua` is 73 lines that no test executes; the only Lua any test parses is
`_meta.lua`, and that test exists to protect the version-reading trick. That was
defensible for a menu entry and is not defensible for a discovery ladder, a
debounce and an HMAC. **This item owes a Lua gate**: at minimum every shipped
`.lua` loading cleanly in CI, and better, the pure functions run under `mlua`
with `require` stubbed. `mlua` is already a dependency and the sidecar sandbox
is the obvious host.

`KO_HOME=/tmp/ko-test` points desktop KOReader at a scratch data dir, which is
the plugin edit loop with no device in it (`datastorage.lua:20` reads `KO_HOME`
before every other branch). Note in your report that the desktop target and the
device target are different paths.

## Constraints

- Engine + API + daemon + CLI, plus the devices page's *ready* state. No TUI.
- Typed `Diagnostic`s with an `ErrorClass`, never pre-formatted strings and
  never `EngineError::Other` where a caller might branch.
- Never edit an applied migration. `0021` is yours; if you conclude the item
  needs no migration, **say so and leave `0021` unclaimed** rather than
  inventing a use for it.
- The fixed UDP port is a `const`, not a config knob — `MOUNT_QUIET`'s reason:
  a knob means the tested value and the shipped value can differ. Confirm the
  number is unassigned before hard-coding it.
- Properties where an invariant exists: a push followed by a second push of the
  same reader changes nothing (idempotence, which the sidecar payload should
  give you for free); the ladder returns the first reachable candidate for any
  ordering of unreachable ones.

## Done when

`make fmt lint build-check test ts-check` is green — **not `make ci`**, which a
fresh worktree cannot run honestly: with no `gui/node_modules` the `web-check`
leg prints `SKIPPED:` and passes without checking anything. Say which you ran.
Run the `cargo-tester` agent before committing; if you are a subagent and cannot
launch it, run its procedure directly.

Then, by hand and said plainly in the report: a push from a real reader to a
listening desktop, and a pull the other way. If no hardware was available, say
that instead of implying coverage.

## Two things to send back

**The corrections this forced.** Every entry in `docs/decisions.md` records what
building the item changed about the plan, and that paragraph is the most
valuable thing the item produces. What did the KOReader source say that the spec
has wrong? What did a real AP, a real suspend, or LuaSocket on a device make
untrue?

**Push back rather than comply.** Four of five threads in the 11–16 wave did and
each time they were right. Three candidates here, all defensible:

- *the sidecar as payload* — if the reader would have to re-serialise something
  it does not hold, the argument weakens;
- *the listener as the daemon's state rather than the engine's* — if this wants
  a settings table, say so rather than building one silently;
- *push before pull* — if pull turns out to be the cheaper half because the
  desktop is the better side to put discovery on, reorder the stages and explain
  why.

## Still unconfirmed from 15a, and this item is where it bites

**Nobody has ever seen the menu entry appear.** Everything static about that
path checks out against the device's own source, but it has not been observed on
hardware. This item adds a second entry under the same one. Confirm the first
before building on it.
