# 2026-07-28 — item 11, the wired device watcher

Picks up from `58ac5ee` on `main` (items 1–10, migrations `0001`–`0009`) and
implements item 11 of `docs/spec-11-16.md`. One worktree, one PR. **No
migration** — nothing here touches the schema.

Mount → scan, with nothing typed. The engine surface for a mounted reader was
already complete (`scan_device`, `sync_device`, `candidate_mounts`,
`is_koreader_mount`, the `sidecar_seen` pre-filter); what was missing was
*noticing*.

## What was built

- **`crates/engine/src/watch.rs`** — `MountWatcher`, `MountEvent`, `MountStir`,
  `watch_mounts()`, `MOUNT_QUIET`. A debounce over an injected channel, plus a
  thin `notify` adapter.
- **`device.rs`** grew `mount_roots()` and `offers_reader()`, both extracted
  from `candidate_mounts`'s body rather than written a second time.
- **`EngineError::Watch`** — its own variant because the only sane response is
  to degrade.
- **TUI**: `run` takes an `Option<MountWatcher>`, one more `select!` arm,
  `App::on_mount_event`, seven tests.
- **CLI**: `ko watch`.
- `deny.toml` allows CC0-1.0; `notify = "8.2"` in the workspace manifest.

## Decisions

- **The seam is a channel of `MountStir`s, not a trait.** The spec required an
  injectable event source; a `tokio::sync::mpsc::Receiver` is the smallest thing
  that is one. A test sends into the sender; `watch_mounts()` is a `notify`
  callback that sends into the same one. No `async_trait`, no mock impl, and the
  adapter is the only part of the file that cannot run in CI.
- **Virtual time, not sleeps.** Every debounce test is
  `#[tokio::test(start_paused = true)]`, which is the existing house rule for
  `PROVIDER_TIMEOUT`. The whole watch suite runs in 0.01s.
- **`MOUNT_QUIET` is a `const` (2s), with `quiet_for` for tests.** Same
  reasoning as `PROVIDER_TIMEOUT`: making it a user setting would let the tested
  value and the shipped value differ.
- **The watcher reports transitions; what was already mounted is seeded, not
  announced** (`already_here`). `candidate_mounts` already answers "what is
  plugged in", and both the TUI and `ko watch` call it at startup. Announcing it
  here would scan a device the caller had just been told about.
- **The watcher may scan; it may not sync**, per the spec's explicit sentence.
  Enforced by construction rather than by rule: `watch.rs` takes no `Storage`,
  so nothing in it *can* write. The TUI end is asserted
  (`a_reader_arriving_never_brings_anything_across_by_itself` drains every
  deferred unit and checks the library is unchanged).
  - Worth recording, because the two documents read differently at first
    glance: `docs/decisions.md` says "mount → scan → **import**, nothing typed",
    while the spec says "the watcher may scan; it may not sync". Read together,
    "read-only" in decisions.md means *read-only with respect to the device* —
    it is the plugin-install path it is drawing a line against. The spec's
    sentence is the narrower and later one, so this item stops at the scan. An
    auto-import is a decision to take on its own evidence, not one to smuggle in
    under a word that was written about something else.
- **Arrival refreshes the device screen through `pending_scan`**, never inline.
  The existing deferred-work path already exists for exactly this: the walk
  happens after the frame that announced it, one unit per loop iteration.
- **An arrival elsewhere is a status line naming `m`, not a screen change.**
  Being yanked off the book you are reading because a cable was plugged in is
  the other way to get this wrong, and it is worse than a line you might miss.
- **`ko watch` was added although the spec did not ask for it.** The item's
  verification story is "the real thing needs hardware and is local-only", and
  confirming through the TUI means confirming the watcher and a screen at once.
  A headless command is the instrument. It scans and prints the sync command; it
  never syncs.

## Things that were not obvious

- **`notify` is CC0-1.0**, which `deny.toml` did not allow. Added with a comment
  rather than waved through — the file's purpose is that a licence cannot arrive
  with a routine `cargo update`, and that only holds if every licence is named.
  CC0 is a public-domain dedication: strictly fewer obligations than MIT.
- **The `notify` watch must be non-recursive.** `/Volumes` recursively is every
  file on every mounted disk — on a reader, the entire library. Stated in the
  code because the default in most examples is `Recursive`.
- **An event path has to be reduced to the volume it happened on.** A page turn
  rewrites a sidecar; without `normalize`, the reader would be rescanned on
  every page turn.
- **A stir at a *root* is expanded against `present` as well as `read_dir`.**
  Some platforms report an unmount only as a change to the directory that held
  it, and by the time it settles the directory it names is gone — so a departure
  derived only from what is on disk is a departure that never fires.
- **Cancel-safety is not optional here.** The TUI's `select!` drops
  `MountWatcher::next` on every keypress. Every deadline and every
  decided-but-unhandled event therefore lives in the struct;
  `cancelling_the_wait_does_not_lose_the_arrival` polls-and-drops eight times
  across one quiet period and still gets the arrival. `timeout_at` rather than an
  inner `select!`, which also kept `tokio`'s `macros` feature out of the engine.
- **A stir is armed when it is *received*, not when it is sent.** Two stirs sent
  back to back without a `next()` between them settle in the same instant, so
  the ordering test has to poll between the sends. This is real behaviour, not a
  test artefact, and it is harmless: the watcher only exists while something is
  polling it.
- **`std::future::pending()`, not `None`, for a dead source.** A `select!` arm
  that resolves immediately and forever is a spin, and the TUI's loop would have
  burned a core the moment the watcher's channel closed.

## Verification

Two halves, and which is which matters:

- **In CI, with the fake source.** Eleven engine tests (burst → one arrival;
  nothing before the quiet period; a still-writing device keeps the scan
  waiting; a still-mounted volume is never announced twice; unplug and replug;
  a non-reader volume is never announced; already-mounted is not an arrival;
  ordering; root-stir expansion both ways; cancellation; dead source) and seven
  TUI tests. `make ci` equivalent green: fmt, `clippy --workspace --all-targets
  --locked`, `cargo test --workspace`, `cargo deny check licenses bans sources`.
- **On this machine, against real macOS mount events.** Not a KOReader device —
  a 20 MB HFS+ disk image carrying a plausible KOReader install
  (`reader.lua` + `frontend/` + `plugins/`) and one synthetic sidecar, plus a
  second image with nothing on it. `hdiutil attach`/`detach` under a running
  `ko watch`:

  | | result |
  |---|---|
  | attach the reader image | `reader mounted: /Volumes/RBTEST`, scanned, 1 book `new` |
  | attach a plain volume | nothing announced |
  | detach | `unplugged: /Volumes/RBTEST` |
  | re-attach | announced again, and the scan reports `0 read, 1 unchanged` — the `sidecar_seen` cache surviving the round trip |

  A real reader over USB is still unconfirmed, but every layer between FSEvents
  and the scan is.

## Not done

- **Linux** (`inotify` under `/run/media/$USER`, `/media/$USER`) is written and
  compiled but only exercised by the fake source. The adapter is ~30 lines and
  the platform difference is entirely inside `notify`.
- **No auto-import.** See the decision above.
