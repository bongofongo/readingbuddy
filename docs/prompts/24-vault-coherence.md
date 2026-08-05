---
title: Item 24 — vault coherence
date: 2026-08-05
source: docs/gui/spec-gui-17-28.md item 24; docs/decisions.md's Vault section;
        crates/engine/src/watch.rs's module doc for the constraint to depart from
follows: sessions/2026-08-05-the-derived-facts-layer.md
---

# Prompt — Item 24: vault coherence

Paste into a fresh session at the repo root, on branch `feat/engine-vault-watch`,
branched from `main`. Parallel-safe with items 18, 19, 20 and 22 — see *Launch
order* in `docs/next-thread-handoff.md`.

Read `CLAUDE.md` (**Engine standards** is binding), then
`crates/engine/CLAUDE.md` (the vault section), then item 24 in
`docs/gui/spec-gui-17-28.md`, then **`crates/engine/src/watch.rs`'s module doc**,
which states the constraint this item has to consciously break.

**Engine, plus whatever wires it. No migration** — `0014` is item 20's and
`0015` is item 23's.

## What the item is, and what it is not

**Smaller than it looks, and not the job it appears to be.**
`Engine::refresh_note_from_disk` is **not an orphan**. It has an API facade
method (`crates/api/src/lib.rs`), a request variant (`RefreshNoteFromDisk` in
`protocol.rs`) reachable over the socket today, and a test named
`refresh_note_from_disk_reindexes_an_external_edit`.

What does not exist is anything that calls it **automatically**. No frontend
issues it and nothing watches the vault. So: edit a note in Obsidian — which
`docs/decisions.md` explicitly supports as a courtesy — and `notes_fts` stays
stale indefinitely.

So this is **a watcher, not a wiring job**. And the thing that makes it urgent
rather than tidy: **the GUI is about to add a search box** (item 27), and
`notes_fts` has no triggers — the engine writes it from application code, so a
note whose file changed under us is a note `SearchNotes` cannot find. A search
box that silently misses the note you edited five minutes ago is the worst kind
of bug, because it looks like the note is gone.

## The design decision this item exists to make

`crates/engine/src/watch.rs` is already wired for mounts
(`crates/tui/src/app.rs`), and its **debounce is the right shape to reuse**.

But read its module doc first, because it states a constraint you are about to
break: **it may scan; it may not sync.** It holds **no `Storage` at all** — that
is not an accident of the implementation, it is the property that makes a
background watcher safe to have running.

A vault watcher that refreshes the index is a **write**. So this is a real
departure and it needs an argument, not a copy. Decide and defend:

- **Watch → notify → the frontend issues `RefreshNoteFromDisk`.** Keeps
  `watch.rs`'s property intact; the watcher stays storage-free and the write
  stays on a path that already exists and is already tested. Cost: every frontend
  has to wire it, and the daemon has to have a push channel it does not have
  today. A frontend that forgets is a frontend with a stale index and no symptom.
- **Watch → refresh directly.** One implementation, correct for every frontend
  at once, and the debounce and the write live together where the race is. Cost:
  the engine grows a background task that writes to the database, and every
  argument for `watch.rs` holding no `Storage` now has to be answered — pool
  contention, write ordering against a foreground import, and what happens when
  the watcher fires during a transaction.

Say which, and say why the other one is worse. **That paragraph is the item**;
the code is comparatively short.

## The races to name explicitly

A vault watcher has three and they are the part a first cut misses:

- **We wrote it.** The engine writes vault files itself; the watcher will see its
  own writes and refresh a note from a file it just produced. Idempotent, so not
  a correctness bug — but it is a write amplification loop with a debounce in it,
  and it is worth a test rather than a shrug.
- **A partial write.** Obsidian and most editors write via a temp file and
  rename; some truncate in place. A refresh that reads a half-written file
  indexes garbage. The debounce is the first answer; whether it is a sufficient
  one is yours to establish.
- **Deleted and recreated.** A file that vanishes is not the same as a note that
  was deleted — `docs/decisions.md` gives the vault the courtesy of being
  editable by other tools, which means a file can be moved out and back. Decide
  what an absent file means and do not make it destructive by default.

## One known gap to leave known

`notes.title` is **not unique**, so an edge resolved to one of two same-titled
notes dangles again if that one is deleted. Item 9 pinned this deliberately. It
is **write-side** and this item does not paper over it — do not add a uniqueness
constraint to make your watcher's life easier; that is a migration, it is not
allocated, and it would change what the vault permits.

## What must not happen

- **No migration.** If something here needs storage, that is a finding to report.
- **Do not make the watcher a requirement.** The mount watcher degrades to
  nothing when it cannot run; a vault watcher that fails to start must not stop
  the app opening, and a platform where `notify` is unavailable must still have a
  working vault. Absence is a first-class answer in this repo — an absent calibre
  is not a failure, and neither is an unwatchable directory.
- **No task-completion framing.** Nothing here reports "3 notes out of sync". A
  stale index is the app's problem, not a chore assigned to the user.
- **Nothing above `trace!` carries note bodies.** Highlight text, note bodies and
  search queries are the user's private reading — `CLAUDE.md`'s tracing rule, and
  `tests/tracing_redaction.rs` is the one place log output is asserted on.

## Files you own

`crates/engine/src/watch.rs`, `crates/engine/src/notes.rs`,
`crates/engine/src/lib.rs`, and the TUI/daemon wiring. **No collisions** with
items 18, 19, 20 or 22 beyond the `lib.rs` export list, where conflicts are
textual and trivial.

## Push back rather than comply

Four of five threads in the last wave did, and each time they were right. Two
lines worth arguing with:

- **Whether a watcher is the right answer at all.** A refresh-on-read — check
  the file's mtime when a note is opened or searched — is cheaper, has no
  background task, no races with our own writes, and no platform surface. It is
  also *wrong for search*, because search reads the index and not the file, which
  is precisely the case that motivates the item. If refresh-on-read plus a
  refresh-on-search-open is enough, that is a much smaller item and worth saying
  so.
- **Whether `notes_fts` should have triggers.** The repo's answer today is that
  the engine writes it from application code. Triggers would make the whole
  problem smaller and are a migration, so not yours — but if that is the real
  fix, name it as an item rather than building around it.

## Done means

- `make ci` exit 0.
- The `cargo-tester` agent before you call it done.
- **A test that edits a file behind the engine's back and then searches for it.**
  That is the whole bug; if the suite cannot express it, the item is not done.
- **The corrections this build forced, written into `docs/decisions.md`** —
  including, explicitly, which of the two designs above you chose and why, since
  the next thread to touch `watch.rs` inherits that decision.
- A session log, via the `wrap-session` skill.
