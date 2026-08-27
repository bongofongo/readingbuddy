# Prompt — Item 55: the devices page

The GUI's fifth place: **`/devices`**. Every reader readingbuddy is paired with,
what is plugged in right now, every way you can connect one, and calibre.

Owns migration **`0020`**. `0019` (item 15a) is the highest applied. Runs alone.

**Check your base before writing a line**: `git log --oneline -1` and
`ls crates/engine/migrations/ | tail -2`. If migrations do not stop at `0019`,
`git reset --hard main`.

Read `docs/decisions.md` (**Device linking**, **KOReader plugin**, **Calibre**,
and entry 15), `gui/CLAUDE.md`, `crates/engine/CLAUDE.md`. `CLAUDE.md`'s
**Engine standards** section is binding.

## The item

A place, not a wizard. It answers three questions with no clicking:

1. **Which readers do I own?** `paired_devices`, plugged in or in a bag.
2. **What is the state of my data on this one?** Last seen, last brought
   across, what is waiting on it right now.
3. **How else could I connect?** The plugin over USB, plain read-only import,
   calibre. Each with the motions of doing it.

## The four API gaps, and they are the item's first half

The audit found the page could be drawn against today's wire and would **say
things that are not true**. All four are engine work, and none is a frontend
workaround.

1. **`last_seen_at` only moves on install.** So "last connected" is really
   "last time you installed the plugin". Fix: `plugin_status` stamps
   `last_seen_at` and `last_mount_path` when the mount names a device we are
   paired with. It stays read-only *about the mount*; recording that we saw a
   reader is a fact about **us**, and putting it here rather than in a
   `note_device_seen` the caller must remember is what keeps the CLI, the TUI
   and the GUI from disagreeing about when a device was last in hand.
2. **Nothing forgets a device you do not have in hand.** `UninstallPlugin`
   takes a mount, so a reader you sold is in the list for ever. Add
   `ForgetDevice { device_id }` — **our side only**, and the copy must say so:
   the plugin is still on that reader and we cannot reach it.
3. **A device cannot be renamed.** `label` is the mount's directory name
   (`Kindle`, `KOBOeReader`) frozen at install. Add
   `RenameDevice { device_id, label }`. Note the trap it creates:
   `record_pairing`'s `label = COALESCE(excluded.label, paired_devices.label)`
   prefers the *mount name*, so every reinstall would clobber a rename. Invert
   it — an existing label wins, and the mount name fills only an empty one.
4. **No per-device sync record.** Migration `0020` adds
   `last_synced_at INTEGER` to `paired_devices`, and it means exactly one
   thing: *when we last brought annotations across from this reader*. Not a
   scan (read-only, and it happens on arrival), not a statistics import.

Stamping it needs a device-aware verb, because `sync_device(paths)` takes
sidecar paths and cannot know whose they are. Add `SyncMount { mount }`:
scan → sync everything syncable → stamp the paired row if the mount carries a
pairing we know. **A new request, never a field on `SyncDevice`** — `ts-rs`
emits a new field as required TypeScript however `#[serde(default)]` the Rust
is. It re-scans server-side rather than trusting paths the caller is holding,
which is this crate's own "handles do not cross".

`API_VERSION` stays **2**: three new requests and one new field on a *response*
DTO are additive. Adding `last_synced_at` to `PairedDeviceDto` is safe for the
same reason it would not be safe on a `Request`.

## What it must not do

- **No auto-install.** `decisions.md` is explicit and this is the surface most
  tempted to break it: mount → scan is automatic and read-only, mount →
  *install* shows `plugin_dir` and waits. A device appearing must never write.
- **No per-book device screen.** New / Unchanged / Updated / Unreadable per
  book, with per-book pull and multi-select, is `decisions.md`'s *device
  screen* and it is a later item. This page's device actions are
  **whole-device**: bring everything across, import reading time.
- **No wireless anything.** 15b does not exist. Where the page would name it,
  it says what is true — the pairing is done and there is nothing to send to
  yet — and renders no dead control.
- **No count on a home surface.** This is not one: `/devices` is a page you
  chose to open, so *3 books waiting* is allowed here exactly as `/life`'s
  figures are. What is still forbidden is the badge — nothing in the nav, and
  no number anywhere that counts what you have **left**.
- **No path picker for calibre.** `list_argv(None)` is calibredb's own default
  library and that is the whole offer in this slice.

## Files

Engine: `migrations/0020_device_sync_record.sql`,
`src/storage/paired_devices.rs`, `src/lib.rs`, `src/device.rs` (the report).
API: `src/dto.rs`, `src/protocol.rs`, `src/lib.rs`, `tests/api.rs`.
CLI: `src/main.rs`, `src/commands/ko.rs`.
GUI: `src/lib/api/{bindings.ts,client.ts,fake.ts}`, `src/lib/devices/**`,
`src/routes/(shell)/devices/+page.svelte`, `src/routes/(shell)/+layout.svelte`,
`tests/routes.spec.ts`.

## Two things to report back

**The corrections it forced.** Every entry in `decisions.md` records what
building the item changed about the plan. Ask for them and they arrive; skip
the question and the next thread rediscovers them.

**Push back rather than comply.** Four of five threads did last wave and each
time they were right.
