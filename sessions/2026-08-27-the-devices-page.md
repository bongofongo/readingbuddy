---
title: Devices — the fifth place, and the four gaps drawing it exposed
date: 2026-08-27
scope: item 55 — migration `0020`, three API requests, two response fields,
       `rb ko plugin rename|forget`, `/devices`, `docs/decisions.md` entry 55,
       `docs/prompts/55-the-devices-page.md`
---

# The devices page

Third session of the day, straight after 15a and its device pass. The user
asked for "a connection page": which devices you own, the state of your data on
each, and every way to connect one — the KOReader plugin first, calibre beside
it. Scoped with four questions, then built.

## Decisions locked

- **Scope: connection plus whole-device actions, not the per-book device
  screen.** `docs/decisions.md`'s *A device screen* — New / Unchanged / Updated
  / Unreadable per book, with per-book pull and multi-select — is still a later
  item. This page's device verbs are whole-device: bring everything across,
  bring reading time across.
- **`/devices`, "Devices", a fifth nav link, last.** The four before it are
  about your reading; this one is about the machinery under it, and it is the
  only entry you open because something is plugged in. The shell still has no
  sidebar and five text links still do not need one.
- **All four engine gaps in scope**, including the two that needed a migration
  and a rename. The alternative — draw the page against today's wire — was
  offered and declined, correctly: it would have shipped four false sentences.
- **Item 55 is minted fresh**, so it took the next free register number rather
  than a spec row. `grep '^[0-9]\+\. \*\*' docs/decisions.md` ran to 54.

## The finding worth carrying

**Every request the page needed already existed.** `PairedDevices`,
`CandidateMounts`, `PluginStatus`, `InstallPlugin`, `UninstallPlugin`,
`ScanDevice`, `ImportDeviceStatistics`, `CalibreStatus`, `ImportCalibreLibrary`
— all on the wire, all serving the page, and the page would have been **wrong**
in four places. A surface audit that asks *is there a request* passes here. The
question that catches it is *does the request answer the question the screen
asks*, and these were next-door questions:

- `last_seen_at` moved on **install alone**, so *last connected* meant *last
  time you installed the plugin* — a reader plugged in nightly reported a date
  from April.
- `UninstallPlugin` needs the mount, so a reader sold or lost could never leave
  the list.
- `label` was the mount's directory name frozen at install, with no way to
  change it.
- Nothing anywhere recorded what had come off a device.

## Technical gotchas

- **The stamp goes in `plugin_status`, which makes a read do a write.** Seeing a
  device is an event and the only code that can record it is whoever looked. A
  `note_device_seen` each frontend must remember to call is a column that means
  something different depending on which app you had been using — the same
  argument `reindex_from_body` makes one module over. It stays read-only about
  the *mount*, which is the promise `decisions.md` actually makes.
- **A rename inverted a `COALESCE` that had been right until it existed.**
  `record_pairing` had `label = COALESCE(excluded.label, paired_devices.label)`,
  harmless while the only writer of a label was that function. `install_plugin_at`
  passes `mount.file_name()`, so every plugin upgrade would have quietly restored
  `KOBOeReader` over whatever the reader was called. The mount's directory name
  is a **default**: it fills an empty label and never replaces one.
- **`last_synced_at` needed a device-aware verb.** `sync_device` takes sidecar
  *paths* and cannot know whose they are, so `sync_mount` scans → syncs →
  stamps. It re-scans below the seam rather than trusting paths a caller holds,
  which is `crates/api`'s *handles do not cross* applied to a filesystem. A
  per-book pull deliberately does not stamp: it leaves the question the column
  answers — *is this reader's reading here* — unchanged.
- **`0020` is the repo's fourth non-back-fill and the cleanest of them.** `0012`
  had signals recording who was *consulted*; `0014`/`0016` could not decode a PNG
  or parse a name in SQL. This one has nothing to reason from at all — no row
  anywhere attributes a past import to a device id. `NULL` therefore means *not
  since we started recording*, and both frontends are tested on saying that
  rather than *never*.
- **`PluginCondition` crossed because the CLI had already spelled it once.**
  `installed_version < our_version` is a domain rule with a name
  (`is_version_upgrade`) and `commands/ko.rs` was comparing by hand; TypeScript
  would have been the third dialect. One typed verdict, five arms, `Obstructed`
  winning over every version case because it is the one that gates the action.
- **`ts-rs` makes a new field on a *response* DTO safe and on a `Request` fatal.**
  Two fields were added (`PairedDeviceDto.last_synced_at`,
  `PluginStatusDto.condition`) and three whole requests, rather than a field on
  an existing one. `API_VERSION` stays 2.
- **Playwright's WebKit will not start on Arch.** It needs `libicu74`; the
  system has 76+, and ICU symbols are version-suffixed so a symlink cannot work.
  `PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1` gets past the *validator* and
  the loader then fails on the real thing. Pre-existing — `make shots` and
  `make routes` have never run on this machine.
- **`pkill -f "vite dev …"` matches the shell running it** and killed the
  command that issued it, exit 144.
- **`prettier` in `gui/node_modules` is 3.9.6 and the committed files were
  formatted by something older**, so a bare `prettier --write` reformats 15
  untouched files. Prettier is not in `make web-check`, so this is drift rather
  than a broken gate — reverted rather than swept into this item.

## Bugs found

- **`??` swallowed a cleared name.** `fake.ts` overlaid renames with
  `this.#labels[id] ?? d.label`, which cannot tell *cleared to NULL* from *no
  rename happened*, so a blanked name rendered as the old one for ever. Found by
  the test asserting blank clears; the fix is `in`, not `??`. The same keystroke
  is available in any client.
- **`ko sync --all` scanned twice and printed every warning twice**, then
  returned before it could stamp. Found by running it, not by a test: the `if
  all` block sat below the scan and below the `syncable.is_empty()` early
  return, so a reader you were fully up to date with could never record that it
  was — the CLI and the engine disagreeing about a column whose whole job is to
  be the same on both. `--all` now takes the whole function.
- **A "yet" reached the CLI copy** — *nothing on this reader to read yet*.
  `axiom.test.ts` bans the word across every component and the same argument
  holds here; reworded to a fact about the volume.
- Two more "yet"s were caught by that guard in the GUI before they shipped.

## Four defects the rendered page corrected

All four came from screenshotting it and looking. None is reachable by a type
checker and none would have failed an assertion.

- The card carries a **Plugged in** chip about the cable, and the line under it
  read **Not connected** — a flat contradiction about a thing sitting in a USB
  port. Two facts, two vocabularies: the condition now speaks about whether
  readingbuddy is *on* the reader.
- The accent was on **Put the plugin on again**, the least likely thing anybody
  wants. It now follows the verb the reader needs, which on a current plugin is
  the sync.
- **`auto-fit` twice.** Three columns of unequal-height cards left a hole the
  size of a card, and a lone card stretched across 1300px of nothing. Both grids
  are capped at two, which is the shape the content has.
- The install's destination paragraph was a **flex sibling of the buttons**, so
  *Write it* floated as a tall block beside three lines of prose.

## Verification

- `make lint build-check test ts-check` exit 0. `svelte-check`, `tsc`, `eslint`,
  **306 vitest** (26 new), `vite build` all green.
- Live CLI smoke against a fake reader: install → status → sync `--all` →
  rename → blank-rename → forget, plus the prefix resolver's two refusals. That
  is what found both `ko sync` bugs and the "yet".
- The whole **57-test route suite run under Chromium**, twice, all passing —
  including four new assertions: the install shows its path *before* writing and
  writes nothing at that step; a reader we will not write to has **no** install
  control (absence, not a disabled button); forgetting says the plugin stays on
  the reader; and the page counts no work you have left.
- One selector bug found doing that: `filter({ hasText: 'PLUGGED IN' })` also
  matched the away card's own *Last plugged in at* — a case-insensitive
  substring — so the loose selector picked up the one card the assertion was
  about and passed while checking nothing. Now an exact-text `has`.
- **`make shots` / `make routes` did not run.** See the WebKit note above. The
  route is rendered and reviewed, but the committed **WebKit baselines for
  `devices` and `devices-working` do not exist**, so CI's `frontend` job will
  fail on missing snapshots until `make shots` runs where WebKit starts.

## Deferred

- **The WebKit baselines**, above. The one thing standing between this and a
  green CI.
- **The per-book device screen** — `decisions.md`'s *A device screen* in full.
- **15b: the listener and the push.** Unchanged by this item; the page says so
  in words rather than rendering a dead control.
- **A calibre library path picker.** `list_argv(None)` is calibredb's own
  default library and that is the whole offer; a path is a configuration
  surface.
- **`make dev-db` cannot mint a paired device** and never will — pairing writes
  to a mount. So `/devices` in a real dev run shows its empty state, and this
  page's fixture has no `edge-cases.json` counterpart to drift from.
