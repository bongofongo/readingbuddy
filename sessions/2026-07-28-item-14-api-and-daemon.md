---
title: "Build item 14 — the API crate and `readingbuddyd`"
date: 2026-07-28
branch: item-14-api
spec: docs/spec-11-16.md
decisions: docs/decisions.md
migrations: none
---

# Item 14 — the API crate and `readingbuddyd`

`docs/spec-11-16.md` states the item as three things in a deliberate order:
DTOs with `From<domain>`, **then** a facade complete enough that `pub storage`
can go private, **then** a transport. It also says the real cost is the seam
rather than the transport, and that turned out to be exactly right — the daemon
is the smallest file in this diff.

Two new crates, no migration, and **zero new third-party dependencies**.

```
crates/api      readingbuddy-api   dto.rs · error.rs · protocol.rs · lib.rs
crates/daemon   readingbuddyd      main.rs · server.rs
```

---

## 1. Closing the seam in the engine

The spec's audit was accurate and worth re-stating as it was found: 35 sites
across the CLI and TUI reached through `Engine::storage`, and 99 more inside the
engine's own `tests/`. Ratings-scale admin, readings and highlight listing had
**no facade method at all** — `readingbuddy rating scale|map|show` was written
end to end against `engine.storage`.

So the field is private now and the facade grew methods for every one of those
call sites: `list_books`, `get_book`, `book_tags`, `list_readings` /
`get_reading` / `active_reading` / `update_progress` / `reread`,
`list_highlights`, `get_note` / `note_for_reading` / `note_path`, `book_file`,
`fetch_cover`, the whole of rating-scale admin, and `uncite`.

**One method is new surface rather than a move: `Engine::set_annotation`.** The
ownership seam of migration `0004` gave the reader a field of their own beside
KOReader's and then gave no frontend a way to write it. That is a genuine gap
and worth closing here; nothing else was invented.

### `config` is six accessors, not one getter

`db_url`, `images_dir`, `vault_dir`, `files_dir`, `log_dir`, `google_api_key`.
A single `config()` returning `&EngineConfig` would have been less code and is
wrong, because of the last one — see §2.

### `Engine::storage()` behind an `internals` feature

`crates/engine/tests/` is a separate crate, so it cannot see a `pub(crate)`
field. The three options were:

- give every `Storage` method a facade twin — bloats the facade with surface no
  product calls, which is the opposite of what this item is for;
- leave a plain `pub fn storage()` — a fig leaf, and the frontends would drift
  straight back through it;
- a feature the engine's own tests switch on.

The third, via a **self dev-dependency** (`readingbuddy = { path = ".",
features = ["internals"] }`), which cargo permits and which is the only way a
package turns its own feature on for its own test targets. The TUI's
dev-dependencies switch it on too, because `test_app` seeds a highlight and a
flashcard directly and both are write paths the facade deliberately lacks — a
highlight comes off a device and a flashcard is derived from one.

**The hole that leaves, and the guard for it.** `cargo clippy --all-targets`
resolves dev-dependencies, so under CI's lint step the feature is on for
*every* target in the graph, shipped binaries included: a frontend could write
`engine.storage()` in production code and pass clippy. So CI's check job grew a
plain **`cargo check --workspace --locked`** (and `make build-check`), which is
the build where the feature is off. That step is not redundant with the clippy
line above it, and the comment in the workflow says so, because it looks
redundant and will be deleted by whoever does not know.

## 2. `set_google_api_key` had to stop taking `&mut self`

This is the one engine change the spec named that turned out to have a
consequence, rather than being a rename.

A transport hands one engine to many connections through an `Arc`. `&mut self`
on a facade method is a method no shared owner can ever call — so `Api` could
not have existed with the signature as it was.

The fix is interior mutability, and its shape is a decision:

```rust
providers: RwLock<Arc<Vec<Box<dyn MetadataProvider>>>>,
google_api_key: RwLock<Option<String>>,
```

**The `Arc` inside the lock is the point.** A federated search takes up to five
seconds per provider; a read guard held across that await would make a key
change wait for a network round trip. Instead the `Arc` is cloned under the
guard, the guard is dropped, and the await happens outside it. Both locks
recover from poisoning (`unwrap_or_else(|e| e.into_inner())`) rather than
panicking: the engine is a library, and neither value has an invariant a
half-finished write could break — each is replaced wholesale or not at all.

**And that is why there is no `config()` getter.** `EngineConfig::google_api_key`
is now only the value the engine was *seeded* with; after a runtime change the
two disagree. A struct getter would hand out the stale copy beside the live one
with no way to tell them apart at the call site. `ui/settings.rs` reads
`engine.google_api_key()`, and would show "not set" a keystroke after the user
set one if it read the config.

## 3. The API crate

### No serde on the domain types, and why that is structural

The spec lists this as the measurable cost of the seam. Three reasons it is not
worth "fixing" by deriving `Serialize` on `Book`:

- `Book` carries `OffsetDateTime`, half the reports carry `PathBuf`,
  `Diagnostic` carries a `Duration`. A derive picks a wire encoding for each **by
  accident** — whatever the dependency does this year — and then that accident
  is the API.
- Every field name becomes a public promise. Renaming `ko_percent` would be a
  breaking API change rather than a refactor.
- The engine gains `serde` on its hot path for a caller it cannot see.

So: DTOs with `From<domain>`, and `From<BookDto> for Book` for the one direction
that flows back. That one **drops `created_at`/`last_modified`** — they are
storage's to stamp, and a client that could set them could backdate a row.

`DiagnosticKind` is mirrored **in full**, all seventeen variants. The cheap
version is `{kind: String, detail: String}` and it would have thrown away
exactly what made `Diagnostic` stop being a `String` in the first place: a
caller has to be able to tell a timeout from a 500, and *which file* was
unparsable, without scraping prose. `DiagnosticDto` also carries `display`, the
engine's own byte-for-byte `Display`, so three clients do not re-implement the
CLI's wording three ways.

**Known limit, stated rather than hidden.** A `PathBuf` is bytes on unix and
JSON is UTF-8, so a path crosses as `to_string_lossy` and a filename that is not
valid UTF-8 does not round-trip. There is no lossless JSON encoding short of
base64, which would make every path in the protocol unreadable to a human
debugging it. The trade is made in favour of the ordinary case, in the module
doc.

### The error taxonomy

`ApiError { code: ErrorCode, message: String }`, and the pattern is
`Diagnostic`'s exactly as the spec says to use it: a typed `Copy` classification
beside a human string, with **no source error inside it** (`EngineError` wraps
`sqlx::Error` and `reqwest::Error`; neither is `Clone`, neither is
`Serialize`).

Two things here are promises rather than code:

- Codes are **appended, never renamed or repurposed**, and `#[serde(other)]` on
  `Internal` makes an unknown code degrade to it instead of failing to parse and
  losing the error entirely. That is asserted
  (`an_unknown_code_degrades_instead_of_failing_to_parse`), not hoped.
- The network-ish half **defers to `ErrorClass`** rather than re-deciding what a
  429 is. One definition of "rate limited", shared with every `Diagnostic` the
  engine emits.

`CalibreMissing` gets its own code for the reason it gets its own `EngineError`
variant: absent calibre is an answer, not a failure, and a client must show
"that feature is not here" rather than an error the user is meant to fix.

### Requests are named; responses are shaped

`Request` is adjacently tagged, one variant per facade method:
`{"method":"get_book","params":{"id":3}}` — a name a human can grep for.

`Response` is deliberately **not** its mirror. Sixty-odd single-use response
names would be sixty things to keep in sync for no information: a reply is
already tied to its call by `Call::id`. What the client needs from the wire is
the *shape*, and there are about thirty.

`Reply::to_line` owns the framing, and that placement is the substance:
`serde_json::to_string` escapes newlines inside strings, so a note body — which
is full of them — cannot break a line-delimited frame. Putting the guarantee and
the terminator in one function is what keeps them true together
(`a_newline_in_the_payload_never_becomes_a_frame_break`).

### `dispatch` is pure fan-out

One arm per request, unpacking arguments and calling the typed method of the
same name. **No arm decides anything**, and that is the property the two ways of
using the crate rest on: a rule implemented in `dispatch` is a rule the
in-process caller (iOS) never meets. `dispatch_and_the_typed_method_agree` is
asserted for a read and for a write.

### Handles do not cross

Three facade methods take a domain struct the caller was previously handed back:
`update_note_body(&NoteRecord)`, `delete_note(&NoteRecord)`,
`file_path(&BookFile)`. Over a transport that means the client echoing state
back, and a client holding a stale `NoteRecord` writes to a path that has since
moved. Here they take an **id** and the row is re-read. `map_rating` gets the
same treatment: by `scale_id`, re-read, because a client echoing a `RatingScale`
could map a point against bounds that had been redefined.

Less a translation of the facade than a correction of it, and the clearest
argument that this seam was worth writing rather than generating.

### What is deliberately not in the vocabulary

**`MountWatcher`.** It is a stream of events with no request to answer, and
request/response has no shape for "tell me when a reader arrives". A polling
wrapper would be an invention rather than a translation — it would give the far
side a different debounce from the one `watch.rs` guarantees. A subscription is
its own design. `Api::engine()` exists so a host that wants the watcher can
drive it directly.

**The Google Books key, on the way out.** `has_google_api_key` returns a `bool`.
A secret that has been written should not be readable back over a socket:
nothing needs it, and a settings screen shows a mask.

## 4. The daemon

The least interesting crate in the workspace, on purpose. If a feature is ever
added here, it is in the wrong crate.

**Unix socket, newline-delimited JSON.** The daemon serves this machine's own
user — a Tauri app, a menu-bar companion, a widget's snapshot writer — and a
unix socket matches that scope exactly: no port to collide, no interface to
accidentally expose, filesystem permissions as access control. HTTP would have
added a server, a router and a middleware stack for a protocol with one
endpoint, and put a listening TCP port on a laptop for no gain. As it is,
**nothing new enters the dependency tree**.

Three things in `server.rs` are decisions:

- **`bind` probes before it takes an address.** `UnixListener::bind` fails with
  `AddrInUse` on any existing path, whether or not anything is listening — and a
  socket file outlives a SIGKILLed process. So the path is probed by connecting:
  an answer means somebody is serving and we must not steal their address; a
  refusal means the file is a corpse. Without the second half the daemon can
  never be restarted after a kill without a manual `rm`. The socket is then
  `chmod 0600`, because the database behind it is the user's private reading;
  the window between `bind` and `chmod` is noted in the doc comment rather than
  papered over.
- **`MAX_LINE` is not a tuning knob.** Without it anything that can write to the
  socket sends bytes with no newline for ever and the daemon grows a buffer
  until it is killed. It needed a hand-rolled `read_line_capped`: tokio's
  `read_line` grows without bound, and wrapping the reader in a `Take` caps the
  *reader* rather than the line — a `Take` is `AsyncRead` but not `AsyncBufRead`,
  so `read_line` cannot be called on it at all, and capping the reader would
  silently split one overlong line into two calls instead of noticing it. A line
  that hits the cap closes the connection rather than resynchronising, because
  the remainder is still in the stream and would be read as the next call.
- **`serve` takes the `UnixListener`, not a path.** The same seam `watch.rs`
  took a channel for, and for the same reason: all nine tests run without
  starting a daemon.

A malformed line is answered and the connection **survives** — framing is
intact, so the next line is still a call. An overlong one is answered and the
connection closes. That asymmetry is the whole of the daemon's judgement.

## What went wrong on the way

- The first `handle` used `(&mut reader).take(MAX_LINE + 1).read_line(...)`. It
  does not compile, and the reason is the useful part: `Take<&mut BufReader>` is
  `AsyncRead` but not `AsyncBufRead`. Had it compiled it would have been wrong
  anyway — see above.
- A daemon test sent a JSON line **without a trailing newline** and then blocked
  on the reply that could never come. The suite hung for six minutes rather than
  failing. Worth remembering as a shape: in a line-delimited protocol, a test
  that forgets the terminator does not fail, it hangs.
- `cargo deny check advisories` fails on this branch — yanked `spin 0.9.8` via
  `sqlx-sqlite`. **Pre-existing**: it fails identically on `main` with this
  branch stashed. `licenses`, `bans` and `sources` all pass, and no new crate
  entered the tree.

## Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked
  -D warnings`, `cargo check --workspace --locked`, `cargo test --workspace` —
  all green. 39 new tests (18 API unit, 12 API integration, 9 daemon).
- Everything is offline and headless. The daemon's suite binds real unix sockets
  in tempdirs, so it runs on a CI runner unchanged.
- **Not verified by machine:** nothing. There is no hardware half to this item —
  unlike 11, 13 and 15.

## What this leaves for later

- **No CLI or TUI surface for the daemon.** Neither frontend was rewritten onto
  the API crate, and neither should be: they hold an `Engine` in-process and the
  facade is now complete enough for them. The API crate exists for the GUI and
  the companions.
- **`API_VERSION` is 1 and the compatibility story is one-directional.** Adding
  a method does not bump it (an older client never sends the new name; a newer
  client meets `BadRequest` on an older daemon, which is a clear failure rather
  than a silent misread). Changing a shape does. There is no negotiation
  handshake, only a stamped version on every reply.
- **No subscription.** See the watcher, above. Item 15's wireless push is the
  next thing likely to want one.
