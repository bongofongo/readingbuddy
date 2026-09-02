---
title: The wireless link — item 15b, specced and built
date: 2026-09-02
scope: `docs/spec-15b-the-wireless-link.md`, `docs/prompts/15b-the-wireless-link.md`,
       `crates/engine/src/wireless.rs`, migration `0021`, the plugin's network
       half, the daemon's listener, three API requests, the devices page
---

# The wireless link

One orchestrated session. The user asked what state the KOReader plugin and its
"connection" were in *for testing*; that answer turned into the 15b design, and
the design turned into a build thread that ran all three stages. Stages 1 and 2
are on `main` (`dce744e`); stage 3 is on `feat/koreader-wireless-link`
(`488c497`), deliberately unmerged.

## Where it started

The audit answer, which is worth keeping because it was the reason for
everything below: 15a was well covered (16 plugin tests, 8 `paired_devices`, 15
`device_scan`, 25 `watch`, 5 API, all green) and **15b did not exist at all** —
no listener, no protocol, not stubbed. The real gap was that `main.lua` was 73
lines **no test executed**; the only Lua any test parsed was `_meta.lua`, and
that test exists to protect the version-reading trick rather than the plugin's
behaviour.

## Decisions locked

- **The primary workflow is a tap on the reader**: *Tools → readingbuddy → Push
  to \<computer\>*. Never a hook. `onAnnotationsModified` and `onCloseDocument`
  exist and were deliberately not used — wifi is off by default, so "push when a
  highlight is made" really means "push next time the radio is up".
- **The desktop's listener is a toggle with three states** — off / open for a
  window / always — and the UDP responder lives and dies with the TCP one.
- **Discovery only works while the other side is open**, which the toggle gives
  for free. With the listener off there is no service to find and nothing to
  leak; it is *fails closed* at the transport layer.
- **Pull requires the reader to open its own window** while wifi is already on.
- **The spec gets its own file.** A protocol is its own subject — the rule that
  gave `storage/`, `providers/`, `ui/` and `render3d/` their own `CLAUDE.md`s.
- **One thread, three stages, not three threads.** Push and pull share one
  rendezvous; three agents on one protocol produce three dialects of it (items
  26–28's lesson).
- **Merge stages 1–2, hold stage 3** pending the Kindle firewall question.

## Bugs found

Both pre-existing, both surfaced by building on top of the code rather than by a
test:

- **`PluginStatus.device_id` reported a stranger's id.** It held whatever id the
  file named, so a reader belonging to *another* readingbuddy install reported
  that install's id under a heading meaning "ours" everywhere else. Now `None`
  in that case. One engine test asserted the old meaning and was rewritten.
- **`plugin install` printed "reinstalled v1, still paired as \<brand-new
  id\>"** on a second computer — `upgraded_from` is about files and `paired` is
  about the pairing, and 15b is what pulled them apart (`dbf095d`).

## Technical gotchas

The most valuable section, as usual. Everything about KOReader was read from a
checked-out `koreader/master`, not remembered.

- **`ffi/sha2`'s HMAC key is a silent fork.** `sha.sha256` and `sha.hmac` both
  return **hex, not bytes**, and our token is 32 bytes written as 64 hex
  characters — so "keyed with the token" means two different keys, each
  producing a valid MAC with neither side looking wrong. Vectors, from
  koreader-base's own `sha2.lua`: hex chars → `a38b2086…7d7a7cae`, raw bytes →
  `26ae9d25…11a599fe`. **We take the hex characters**, because that is the
  string `pairing.lua` literally holds, so the device needs no `hex_to_bin`.
  Also `hmac(hash_func, key, msg)` — the hash *function* first.
- **KOReader has no mDNS, zeroconf or Bonjour anywhere** — grep over `frontend`,
  `plugins` and `base` returns nothing. That single fact is why discovery is a
  ladder (cached endpoint → broadcast → directed broadcast → **hostname via
  `socket.dns.toip`** → hand-rolled mDNS → typed), and why the hostname rung is
  the best answer for a laptop that leaves and rejoins.
- **A plugin that caches state in its own directory bricks its own installer.**
  `plugin::inspect` skips only `installed.lua` and `pairing.lua`; anything else
  absent from the manifest is `unrecognised`, and `refuse_if_obstructed` then
  fails **install and uninstall both**. `endpoint.lua` had to become a third
  skipped, never-hashed, device-writable name.
- **`crates/api` links in-process for the GUI with no daemon at all**, so the
  spec's "the listener is the daemon's state" was wrong: it would have been a
  listener the devices page could never turn on. It is the engine's. No settings
  table — `Window` deliberately does not survive a restart, and `Always` is the
  daemon's `--listen` flag.
- **The engine now spawns a task**, which `watch.rs` forbids in prose. Deliberate
  exception on item 24's pattern: a mount's consequence is a *decision* and
  belongs to the frontend, a UDP responder's is not, and `StartListening`
  travels as JSON and cannot hand back an object to poll. Paid for by
  `Running::drop` aborting both sockets — which is how "the UDP responder dies
  with the TCP listener" became structural rather than remembered.
- **An unsolicited beacon is a security regression, and the spec asked for one.**
  It carries no fresh nonce *from the party checking it*, so it can only sign
  something the announcer chose — replayable, and it makes "the seeker verifies
  identity before sending a byte" true in one direction only. Pull reuses
  `HELLO`/`HERE` with the roles swapped instead. **This is the finding most
  likely to be re-opened by accident**: a beacon reads as an obvious
  improvement.
- **"Idempotence for free" is overstated.** Free only for a sidecar carrying
  `partial_md5_checksum`; without one the import duplicates — over a cable as
  much as over the wire.
- **KOReader has no API enumerating a device's sidecars.** `DocSettings` answers
  only *where is this document's*. The source of books is `readhistory` (what
  `exporter.koplugin` uses), so a cleared history entry is not pushed until the
  book is reopened.
- **A Kindle firewalls inbound ports.** `httpinspector` punches an `iptables`
  hole for its port and removes it on stop. Harmless for push (the reader dials
  out); it lands squarely on **pull**, and it is why stage 3 is unmerged.
- **`SimpleTCPServer` has no run loop** — one non-blocking `accept` per
  `waitEvent`, and it reads headers *in line* with a 100 ms timeout. "Never
  block the UI" binds the pull side hardest.
- **Four stop hooks, not one**: `onEnterStandby`, `onSuspend`, `onExit`,
  `onCloseWidget`.
- **`stop()` ordering cost a bug each way.** A window closing from inside the
  accept loop aborts that task at the next `.await`, so state must settle before
  the sockets drop (otherwise the listener advertises a socket it just
  destroyed), and the final ack must be written before the door shuts (otherwise
  the reader sees a failure it never had).
- **The replay-nonce set is scoped to one open door.** Looks like a leak, is
  correct: a body MAC covers the body's hash, so a replayed `open` cannot be
  followed by forged bytes, and remembering nonces across a closed door buys
  nothing and costs an unbounded set.
- **An `mlua::Value` returned past the `Lua`'s lifetime panics** with *"Lua
  instance is destroyed"* — stringify inside the closure.

### Orchestration gotchas

- **A worker's `make web-check` prints `SKIPPED:` and passes**, because a fresh
  worktree has no `gui/node_modules`. The cheap fix, used twice this session:
  **symlink the main checkout's `node_modules` into the worktree and run
  `make web-check` there** — it verifies the frontend half *without* merging
  first. Both runs came back exit 0 (svelte-check 440 files, 0 errors).
- **Editor diagnostics from a worktree with no `node_modules` are noise** —
  missing `vitest`, `padStart does not exist`, private-identifier errors. One
  that looked real (`FakeClient` missing `pullFromReader`) was a mid-edit
  snapshot; the method was there. Check the file before believing the squiggle.
- **`git merge -F -` does not read stdin** the way `git commit -F -` does
  (`error: could not read file '-'`). Write the message to a file.
- **`ln -sfn target existing_real_dir` fails safely** rather than clobbering the
  directory — `unlink` on a directory is `EISDIR`. Worth knowing before pointing
  a symlink at a populated `node_modules`.
- **The base check earned its place a fifth time**: the worktree was created at
  `7d145b3`, two commits behind and without the spec it was told to build from.
  Caught only because the brief said to look.

## Verification

- **`make ci` green on `main` at `dce744e`** — 589 engine unit tests, 295 in the
  large integration binary, 61 API, 23 CLI, svelte-check `440 FILES 0 ERRORS`,
  vitest 314 tests in 23 files, production build. **No `SKIPPED:` lines
  anywhere**, which for this item is the point: it is the first run of the
  wireless work where nothing green asserted nothing.
- Stage 2 and stage 3 each had `make web-check` run against the branch here, via
  the symlink above. Exit 0 both times.
- **Live smoke on real sockets** (the thread's own, on this LAN): broadcast
  discovery of `arch-fongo` at `10.2.0.2`, MAC verified, a push landing, `rb
  list` showing the book, and `ko plugin status` still reporting *"everything
  brought across: not since readingbuddy started recording it"* — which is
  `0020`'s "a push is not a sync" holding in a shipped binary. Pull: `rb ko
  fetch` reported *"1 book came across: 4 new, 0 updated"*, and with the window
  shut, *"no paired reader answered; is its window open?"*
- **The Lua stopped being untested.** Two gates: every shipped `.lua` compiles
  under mlua, and the pure selection logic runs under mlua with `require`
  stubbed, against exactly the bytes the installer writes.
- **Hardware: the menu entry is confirmed on the device** — open since 15a, and
  the thing everything else rests on. `sorting_hint = "tools"` resolves and the
  loader shows us.

## Deferred

- **Stage 3 (pull) stays on `feat/koreader-wireless-link`** (`488c497`), green
  on its own gate and on `web-check` here, unmerged pending the Kindle
  firewall. The cheap test, while the reader is on the LAN: `nc -vz -w 3
  <kindle-ip> <unused-port>` — *refused* means reachable and unfiltered (pull
  works, merge it), *timeout* means the packets are dropped and the `iptables`
  decision is real.
- **The LAN push has not been run on hardware.** Everything wireless is still
  desktop-to-desktop plus one confirmed menu entry.
- **`docs/decisions.md` has no entry 15 slice b.** The eight corrections are in
  `dce744e`'s commit body, ready to be lifted when the hardware pass finishes.
- Still owed by hardware: the ladder against a real AP, `NetworkMgr` prompting,
  suspend mid-transfer, `Trapper` returning a table across a fork.
- `Engine::start_listening` itself is untested — it binds `0.0.0.0` and is the
  one thing that cannot run in CI, exactly like `watch_mounts`. Everything below
  it is tested on loopback with an injected transport.
- No `iptables` handling, no passive "ready" indicator, no TUI surface.
- **Real sidecars into `tests/fixtures/koreader/real/`** — open since July,
  unchanged, and still the user's data to place.
