---
title: Item 24 — vault coherence, and the rule that was never about writing
date: 2026-08-05
follows: sessions/2026-08-05-the-derived-facts-layer.md
branch: feat/engine-vault-watch
---

# Session log

Item 24 of the wave, built in the worktree `rb-wt/24-vault` on
`feat/engine-vault-watch` off `a8ff043`. **No migration** — `0014` is item 20's
and `0015` is item 23's, and nothing here needed storage.

The prompt said the argument was the item and the code was comparatively short.
That was right, and it stayed right: the ruling below is what the next thread to
touch `watch.rs` inherits.

## What was actually broken

`Engine::refresh_note_from_disk` has had a facade method, a wire request
(`RefreshNoteFromDisk`) and a test since item 7. **Nothing had ever called it.**
No frontend issued one and nothing watched the vault, so a note edited in
Obsidian — which `docs/decisions.md` explicitly supports as a courtesy — was a
note `notes_fts` could not find, indefinitely.

Item 27's search box is what makes that urgent rather than tidy, and it is the
worst shape the failure takes: the box does not look broken, the note looks
gone.

## The ruling: watch → refresh directly

`VaultWatcher` holds a `Storage` and does the write itself. That departs from
`watch.rs`'s stated constraint — *it may scan; it may not sync* — and the
argument is that **the rule was never about watchers writing. It is about
consent.**

A mounted reader is somebody else's disk, and a cable is not permission to
modify it. The vault is ours, in our own data directory, and the write in
question is not a write to the user's notes at all: it is a *derived index*
catching up with the file that was already the origin of its content.
Re-deriving a cache from its source is not a sync; it is the cache being
correct.

So the rule is preserved by being restated about the thing each watcher watches:

- **`MountWatcher` never writes to a device.**
- **`VaultWatcher` never writes to the vault** — asserted by
  `never_writes_to_the_vault` over a whole tree of files.

And everything the vault watcher can do to the database is recomputable from
the vault by `reconcile_vault`, which is what makes the departure cheap to be
wrong about.

Each watcher also holds exactly what its consequence needs. A mount's
consequence is a **decision** — scan this, ignore that, ask the user — so the
frontend must make it and a `Storage` there would be an invitation. A file
edit's consequence is a **re-derivation with no decision in it**, the same
answer for every frontend.

### Why notify-the-frontend is worse

1. **The daemon has no push channel and cannot easily grow one.** Every reply
   carries the id it answers; a server-initiated frame has no id to carry. That
   design's first cost is a wire-protocol change in a crate this item does not
   own.
2. **The CLI could not participate at all.** Every command is its own process.
   There is no loop for a notification to arrive in, so the CLI would have been
   permanently stale — and it is a first-class frontend here.
3. **The refresh is not "call a method."** It is *decide which note this path
   is*, *decide whether an absence is a deletion*, *decide whether to trust a
   file still being written*. Three frontends re-deriving those three decisions
   is item 17's finding, restated one wave later.
4. **The failure mode is the worst available.** A frontend that forgets to wire
   it has a stale index and **no symptom**.

What the alternative buys — a storage-free watcher — buys nothing, because the
write it avoids is not one anybody needs to consent to.

### The objection it has to answer, and how

*Pool contention, write ordering against a foreground import, a watcher firing
during a transaction.* **No task is ever spawned.** Both watchers are
pull-driven: nothing happens until the caller polls `next()`, and for the vault
that includes the write. So there is no background writer racing anything — the
refresh runs on the caller's own task and takes the pool the ordinary way. The
TUI's `select!` arm has an **empty handler** and is still doing the whole job,
because polling *is* the work.

Ordering does not need an answer beyond that, either: the file is the origin, and
every refresh converges on what the file currently says.

## The push-back, ruled on

The prompt offered refresh-on-read plus refresh-on-search-open as a much smaller
item. It is **half right, and it is what the CLI actually does** — `rb notes -s`
and `rb links` sweep before they read.

It is not sufficient on its own, and the reason is better than "search reads the
index". **`note_links` is a cross-note index.** Add `[[B]]` to note A in
Obsidian and B's backlinks change — but no read of B will ever notice, because
B's file did not change. Refresh-on-read is structurally blind to that, and
widening it to "sweep before every graph read" *is* a watcher with worse timing.

So both were built, because each covers what the other cannot:

- **`Engine::reconcile_vault`** — the sweep. A watcher only ever sees the
  present, and the ordinary case is a note edited on Tuesday and searched for on
  Thursday. It is also the whole answer where `notify` will not start. Two
  stages, `sidecar_seen`'s pattern: a `stat` per note, a read only where the
  file is newer than the index.
- **`VaultWatcher`** — the liveness, for an edit made while the app is open.

## The three races

- **We wrote it.** Not a loop and unable to become one: the only thing written
  is the database, and the database is not watched, so the echo is one event
  deep however many notes are saved. It does not even cost a transaction —
  `reindex_from_body` compares the file against `Storage::indexed_body` and
  returns without writing when they agree. That also absorbs the far commoner
  event, an editor rewriting a file on focus loss without a character changing.
  Covered by `our_own_write_is_not_re_indexed`, `a_no_op_save_is_not_an_edit`,
  `a_trailing_newline_is_not_an_edit`.
- **A partial write.** The debounce first (`VAULT_QUIET` = 400ms, much shorter
  than `MOUNT_QUIET` because the thing waiting on it is a search box), and
  explicitly **not** claimed sufficient: a write slower than the quiet period
  lands inside it. So `settled_read` stats the file either side of reading it
  and re-arms the debounce when the stamps moved. The remaining hole is stated
  rather than hidden — a write changing neither length nor mtime between the two
  stats is indistinguishable from a file at rest. Tested through an **injected
  reader**, on this module's own rule that a guard which can only be triggered
  by out-running a real text editor is a guard with no test.
- **Deleted and recreated.** The ruling is **absence is never destructive**.
  `Vanished` writes nothing at all. Four reasons: other tools move files
  (Obsidian's `.trash/`, a `git checkout`, a sync client resolving a conflict);
  deletion already has an explicit path that removes row and file together; the
  row holds more than the file ever did (`book_id`, `reading_id`, page,
  location, citations, and every *inbound* edge); and the asymmetry settles it —
  believe a deletion wrongly and something is gone, believe a persistence
  wrongly and the user sees a hit for a note whose file moved. **Recreation then
  needs no case of its own**, which is the ruling's real payoff.

## Two pre-existing bugs the build surfaced

- **`create_note` indexed the body it was handed, not the one it wrote.** They
  differ by a trim and a newline, so no note was ever byte-identical to its own
  index — and the first thing to compare the two would have re-indexed the
  entire vault while looking completely correct. Found by writing
  `our_own_write_is_not_re_indexed`, which failed until it was fixed.
- **`refresh_note_from_disk` was two transactions.** The FTS write and the link
  write were separate, so a cancelled refresh could leave a note whose
  searchable body and whose graph edges came from different versions of the
  file, with nothing on either side looking wrong. `Storage::reindex_note` is
  now one transaction, which is also what makes the watcher cancel-safe.

## Findings reported rather than built

- **`notes_fts` cannot have triggers, and that settles the question rather than
  deferring it.** A trigger copies between tables, and the note body is **in no
  table**: `notes` has no body column and `notes_fts` *is* the only copy. There
  is nothing for a trigger to read. Making triggers possible would mean storing
  every body in a second column beside the index — making the vault a cache of
  the database rather than the other way round, which inverts the ownership
  `docs/decisions.md` states. Not an unallocated migration; a non-option.
- **Unclaimed markdown files are not adopted.** A `.md` file under the vault
  that no note row claims is left alone. Adopting one would have to invent its
  book, its kind and its anchor out of whatever an editor left lying there. If
  that is ever wanted it is its own item, with a frontmatter contract.
- **`notes.title` is still not unique.** Item 9 pinned it; a constraint would
  have made this item's lookups easier and would change what the vault permits.
  Related and newly written down: a **derived title is the note's first six
  words and is indexed beside the body**, so an outside edit leaves the old
  words findable through the title. That is not staleness — the title is the
  `[[wikilink]]` target, and re-deriving it from an outside edit would silently
  repoint every backlink in the vault. It cost one test failure to learn.
- **The sweep's seconds-resolution limit.** `notes.last_modified` is unix
  seconds, so an external edit landing in the same second as one of our own
  index writes is skipped by the sweep. Stated rather than papered over: the
  watcher covers a live edit, and a cold edit is never in the same second.

## Nothing counts what is out of sync

`VaultReconcile` is past tense, for the engine's own log and its tests. There is
no "3 notes out of sync" anywhere, and the TUI's watcher arm shows nothing at
all — an index quietly being right is not news, and announcing it would be a
notification about a chore the user did not have.

## Wiring, four frontends, three shapes

- **TUI** — a fourth `select!` arm with an empty handler, plus one sweep before
  the first frame.
- **CLI** — sweeps before `notes -s` and `links`, the two commands that read the
  *index* rather than the rows. Never fatal.
- **Daemon** — one sweep before `bind`, then one `tokio::spawn` owning the
  watcher. **No wire-protocol change**, which is the ruling paying for itself.
- **GUI (Tauri)** — the same in `InProcess::open`, through the `Api::engine()`
  accessor that already existed for the mount watcher. So item 27's search box
  gets this for free.

All four degrade to a log line. A machine that cannot watch still has a working
vault; what is lost is only liveness.

## Verification

Run from the worktree, `CARGO_INCREMENTAL=0` throughout:

- `make fmt` — clean.
- `make lint` (`clippy --workspace --all-targets -D warnings`) — exit 0.
- `make build-check` (`cargo check --workspace --locked`) — exit 0. This is the
  build where the engine's `internals` feature is off, so the new surface is
  reachable without it.
- `make test` (whole workspace) — exit 0.
- `cargo-tester` agent — run against this worktree.

**`make web-check` and `make routes` were not run**: they degrade silently to
`SKIPPED:` in a worktree with no `gui/node_modules`, and nothing here touches
TypeScript. No DTO changed, so `bindings.ts` is untouched and `make ts-check`
has nothing to say — deliberately: the watcher is a host-lifecycle thing and was
kept off the wire.

## Handoff

Integration is the wave owner's. Nothing was pushed, merged or rebased. The only
file shared with items 18/19/20/22 is `crates/engine/src/lib.rs`'s export list,
where the addition is `VaultReconcile` to the `notes::` line and four names to
the `watch::` line.
