---
title: The KOReader plugin, installed and paired
date: 2026-08-27
scope: item 15 slice a — `plugin.rs`, migration `0019`, `rb ko plugin`, four API
       requests, `docs/decisions.md` entry 15,
       `docs/prompts/15a-the-plugin-and-the-pairing.md`
---

# The plugin and the pairing

One session, no wave. The user asked to start "the KOReader connection plugin
part", scoped to *purely a streamlined connection between readingbuddy and
KOReader*. That maps onto item 15's first half; the session scoped it, ran
`new-wave-item`, and built it.

## Decisions locked

- **Item 15 splits into 15a (the link) and 15b (what travels over it).** The
  user chose installer + pairing only. Slice a is entirely offline-testable and
  designs no protocol; slice b's first question is the listener.
- **The item keeps its spec number.** `docs/decisions.md`'s register
  (`grep '^[0-9]\+\. \*\*'`) runs to 54 and 15 is not in it — an item built from
  a spec row keeps that row's number and is appended in build order, which 37,
  34 and 26 all did. 55 is for items minted fresh.
- **`pairing.lua` carries no host and no port.** We do not know our LAN address,
  there is no listener to name, and inventing the endpoint before the protocol
  exists designs it twice. The point of minting a **token** now is that 15b
  requires nothing typed: the reader was in the user's hand once, over USB, and
  that was the pairing.
- **Pairing state lives inside our own plugin directory**, breaking kosync's
  convention (`settings/kosync.lua`, outside its own dir). Ours would leave a
  token behind after an uninstall we have promised is exact — and at install
  time our directory is the only place the safety rules let us write anyway.
- **An upgrade is not a re-pairing**: reinstalling keeps the device id *and* the
  token and does not move `installed_at`.
- **The token never crosses the wire.** `PairedDeviceDto` has no field for it.

## Technical gotchas

Highest-value section. All four came from reading KOReader's source rather than
guessing, which is `docs/koreader-format.md` §1 applied a second time.

- **`_meta.lua` must never `require("gettext")`** — every KOReader plugin's does.
  Ours stays a pure table literal so the installer can read an installed
  plugin's `version` through the **sidecar sandbox `koreader.rs` already has**
  (`StdLib::NONE`, no `require`). That is what makes "refuse to overwrite a newer
  plugin" cost no manifest format and no second parser. A later contributor will
  add the gettext line by reflex; the guard test is
  `the_shipped_meta_is_readable_by_the_sidecar_sandbox`.
- **On a device, `{data_dir}/plugins/` *is* `koreader/plugins/`.**
  `datastorage.lua:getDataDir()` returns `"."` (cwd is the install dir) and
  `pluginloader.lua:196` registers the extra path only `if data_dir ~= "."`. So
  we must **never write `extra_plugin_paths`** — that would be editing
  `settings.reader.lua` for no gain. On desktop the same directory is
  `$XDG_CONFIG_HOME/koreader/plugins/`, so the install target and the dev loop
  are different paths.
- **`KO_HOME` is read before every other branch** in `getDataDir()`. So
  `KO_HOME=/tmp/ko-test` points desktop KOReader at a scratch data dir — the
  plugin edit loop with no device in it. This is what made "item 15 needs
  hardware" untrue.
- **Our directory survives a KOReader OTA update**, and this was the risk that
  could have moved the install target. The update is a tar unpack with **no
  `--delete`**; the only removal is a manifest diff,
  `grep -xvFf "${KOREADER_DIR}/ota/package.index" /tmp/package.index | xargs -r rm -vf`,
  identical in the Kobo, Kindle, Cervantes and remarkable `koreader.sh`. It
  deletes files in the *old* KOReader manifest and absent from the *new* one; a
  path that never appeared in any KOReader manifest is untouched. Recorded in
  `plugin.rs`'s header so nobody re-derives it.
- **FAT32 ⇒ sha256, never mtime.** Two-second timestamp granularity, no
  permissions, no symlinks, case-insensitive. An mtime-based "did the user edit
  this" check is green in `tempfile` and wrong on hardware — the worst pairing.
- **`#[from]` on a `thiserror` variant requires the source to impl `Error`.**
  `PluginRefusal` was first written with a hand-rolled `Display`; the derive
  fails with a confusing `as_dyn_error` message until the inner type derives
  `thiserror::Error` too.
- **Fetching KOReader files one at a time via WebFetch was the wrong tool** —
  `otamanager.lua` delegates to `ffi/updater` and per-device `install()`, and
  `frontend/datastorage.lua` is actually `datastorage.lua` at the repo root. A
  `git clone --depth 1 --filter=blob:none` (30 MB) answered every remaining
  question in three greps.
- **A test asserting "this file contains no `require(`" fails on the comment
  explaining why it contains no `require(`.** Filter comment lines first.

## Bugs found

- None pre-existing. One clippy `collapsible_match` in new code, fixed.
- Near-miss worth recording: the `docs/decisions.md` entry was first written as
  `15a. **…**`, which the register grep `^[0-9]\+\. \*\*` does not match — the
  item would have been invisible to the exact check `new-wave-item` exists to
  enforce. Now `15. **… (slice a)**`.

## Verification

- `make fmt lint build-check test ts-check` all exit 0. 563 engine tests
  (14 new in `plugin::tests`, 4 in `storage::paired_devices`), 58 API tests
  (3 new).
- The load-bearing test is `install_and_uninstall_restore_any_tree`: a
  before/after sha256 snapshot of the **whole mount**, as a proptest. "Write
  only inside our own directory" is a claim about every other byte on the
  volume, and only a whole-tree diff checks it rather than asserting it.
- Live smoke against a fake mount tree: status → install → status round-trips,
  and the install prints its destination before writing.
- **`make web-check` / `make routes` did not run — `pnpm` is not on PATH on this
  machine.** The only `gui/` change is the regenerated `bindings.ts` and it is
  purely additive (new types, new union members), so nothing existing can have
  broken; that is an argument, not a check. Run `make ci` where node exists.
- `cargo deny` not run — not installed locally. `getrandom = "0.3"` is a new
  direct dependency, MIT OR Apache-2.0, both on `deny.toml`'s allow list, and
  the version was already in `Cargo.lock`. CI is the gate.

## Deferred

- **15b: the listener and the push.** `crates/daemon` is unix-socket-only by a
  written argument (`server.rs:9-21`) — "HTTP … would put a listening TCP port
  on a laptop for no gain". Item 15 is the case that expires that premise, and
  `docs/ux-positioning.md:298` says the plugin is what *pays for* the daemon.
  Overturn it deliberately, with a note.
- **Hardware confirmation.** `koreader_dir` has only ever run against `tempfile`
  trees; the three real mount layouts, FAT32's actual behaviour, and whether
  KOReader's loader accepts our directory are all unconfirmed. The user is
  plugging a device in next.
- **Real sidecars into `tests/fixtures/koreader/real/`** — open since July, and
  the device pass is the moment to do it.
- **No TUI and no GUI surface** for the plugin. CLI only, deliberately, so the
  API is provable before a frontend is shaped around it.

---

# Addendum — the device pass

Same day, second thread. The user plugged the Kindle in and asked for 15a to be
tried on it. Everything the item deferred to hardware is now confirmed, and the
hardware found four bugs plus one bad test.

Device: Kindle, `/run/media/oliver/Kindle`, KOReader **v2025.08**, vfat,
22 sidecars across `Calibre/` and `koreader/help/`.

## Bugs found

- **`ko scan` invented a book on every real device.** `is_sidecar_file` was
  `starts_with("metadata.") && ends_with(".lua")`, which also accepts bare
  `metadata.lua` — and **KOReader ships one**, at
  `plugins/calibre.koplugin/metadata.lua`, a Lua *module* that `require`s four
  others. So every scan of any KOReader install reported an extra unreadable
  book called "calibre", the sidecar sandbox refusing the `require` exactly as
  designed, about a file that was never a sidecar. Fixed by requiring a
  **non-empty extension segment**, which follows from `getSidecarFilename`'s
  literal `doc_path:match(".*%.(.+)")` rather than from the observation.
  Keying on a `.sdr` parent would be **wrong**, not merely stricter: the `dir`
  and `hash` storage layouts file sidecars away from the book.
  `docs/koreader-format.md` §4 said the old test "is the right shape" — corrected
  there and in `crates/engine/CLAUDE.md`.
- **One pairing carried two timestamps.** `plugin::install` stamped `now` into
  the device's `pairing.lua` and `Storage::record_pairing` called `now_unix()`
  *again* for `installed_at`. The very first install on the Kindle came back
  holding `paired_at = …446` against an `installed_at` of `…447`. Worse, a
  **reinstall** rewrote the device's copy to the current clock while the row
  correctly stayed put, so "an upgrade is not a re-pairing" was false on the
  device half of the pairing. Both functions take the instant from the caller
  now; `Engine::install_plugin_at` is the private clock-taking form.
- **"upgraded v1 → v1"** onto a reader that already had v1. `is_upgrade` was
  just `self.installed`. Split into `is_reinstall` / `is_version_upgrade`; the
  CLI has three verbs (install / reinstall / upgrade) and says
  "reinstalled v1, still paired as …".
- **Every refusal printed twice.** `#[error("{0}")]` on a `#[from]` variant makes
  the inner error both the Display *and* the `source`, so anyhow renders
  `Error: <sentence>` followed by `Caused by: <the same sentence>`. Now
  `#[error(transparent)]`.

## Technical gotchas

- **A test that pins a timestamp across two calls to the same method is vacuous
  at second resolution.** The first attempt at
  `reinstalling_the_plugin_keeps_the_pairing_it_already_had` called
  `install_plugin` twice and asserted `pairing.lua` was byte-identical — and it
  **passed against the broken code**, because both installs landed in the same
  second. That is almost certainly how the rule shipped looking tested. The fix
  is a private clock-taking form and two timestamps a day apart; the *check* is
  to run the new test against the reverted code before believing it. Three
  pre-existing `record_pairing` unit tests had the same hole and now pass
  distinct instants.
- **The plugged-in device is the fastest copy of KOReader's source.** Last
  session paid for a `git clone --depth 1 --filter=blob:none`; this one just
  grepped `/run/media/oliver/Kindle/koreader/frontend/`. It is also the *right*
  copy — it is the version that will actually load the plugin.
- **`sorting_hint` must name a real menu id or KOReader crashes.**
  `menusorter.lua:180-181` does `findById(...)` then indexes the result, so an
  unknown hint is `attempt to index a nil value` rather than a misplaced entry.
  `"tools"` resolves in **both** `reader_menu_order.lua` and
  `filemanager_menu_order.lua`, which matters because `is_doc_only = false`
  loads us in both. No shipped plugin on the device uses `"tools"`; checked
  against the order tables directly rather than by imitation.
- **`pluginloader.lua:160` is `plugin_module.path = plugin_root`** — confirms
  `self.path` is our own directory, which is what `readPairing`'s
  `dofile(self.path .. "/pairing.lua")` rests on.
- **mlua names an unnamed chunk after the *Rust* source location.** A malformed
  sidecar reported itself to the user as
  `[string "crates/engine/src/koreader.rs:200:10"]:11: …`. `eval_table` already
  had a `what` for the message; it now also `.set_name(what)`s the chunk.
- **`find -name 'metadata.*.lua'` does not match `metadata.lua`**, which is why
  the phantom book took three passes to locate — the shell glob and the Rust
  predicate disagreed in exactly the place the bug lived.
- **A Kindle that has been on a Mac is full of `._*` AppleDouble files**, one per
  real entry, including one per `.koplugin`. They are inert here (they do not
  match `.+%.koplugin` for KOReader or `metadata.<ext>.lua` for us) but they
  double the line count of any tree diff.
- **103 `.sdr` directories, 22 with a `metadata.*.lua` in them.** An `.sdr` with
  no sidecar is ordinary — KOReader makes the directory for things it has merely
  opened. Do not read a scan's book count against a `.sdr` count.
- **`device::install_fake_reader` is now `pub` behind `internals`**, on
  `pdf::synthetic_pdf`'s written argument. It was `pub(crate)` inside
  `device.rs`'s `#[cfg(test)] mod tests` with a comment forbidding a second
  copy — and an integration test needs exactly that one definition.

## Verification

- **The load-bearing claim holds on real hardware.** A full listing of the whole
  6.2 GB volume plus a sha256 of all 2 344 files under `koreader/`, taken before
  the first install and again after the uninstall: **identical**, empty
  directory removed. That is `install_and_uninstall_restore_any_tree` on FAT32
  rather than in `tempfile`.
- Tamper refusal works on real FAT32 — an edited `main.lua` blocks install *and*
  uninstall, as does an unrecognised file in our directory. sha256-not-mtime
  confirmed.
- All four installed files parse under **LuaJIT**, which is what the device runs.
- **`partial_md5` agrees with the device, 3/3, for the first time.**
  `agrees_with_the_device` had skipped permanently for want of the three books it
  names; they are on the Kindle. KOReader's own `partial_md5_checksum` in the
  three sidecars matches the constants in the test exactly, and our
  implementation matches both. The BitOp finding — `lshift(1024, -2)` being
  `lshift(1024, 30)`, so the first window is at offset 0 — is now confirmed
  against a live device rather than against ourselves. Reached by symlinking the
  three files into the gitignored `personal_data/Calibre/`. The degradation was
  then observed rather than predicted: the device was unplugged during the
  commit-gate run, and the test went back to three `SKIPPED:` lines — `is_file()`
  follows symlinks, so a dangling one is simply absent. **Which is also the
  warning**: that run *looked* like a pass while asserting nothing, and only the
  `checked N/N` line distinguishes the two. Remount and it checks 3/3 again.
- Scan on real data: 22 books, and a second scan parsed **0** — the
  `sidecar_seen` pre-filter working on hardware.
- `make fmt lint build-check test ts-check` green; 2 new tests.

## Still not confirmed

- **Whether KOReader's loader actually shows the menu entry.** Everything static
  about that path checks out against the device's own source, but it needs the
  device unplugged and KOReader opened: *Tools → readingbuddy*.
- Real sidecars into `tests/fixtures/koreader/real/` — still open, still the
  user's data to place.
- Kobo and PocketBook layouts. Only the Kindle layout has met hardware.
