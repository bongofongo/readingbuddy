---
title: UX Positioning & Feature Direction
status: round 6 — rounds 1–5 answered, round 6 questions open
date: 2026-07-27
---

# Where readingbuddy sits

## Round 1 — the landscape as it actually is

### What the incumbents do and don't do

**Kindle.** Closed loop. Highlights live on Amazon's servers, leak out through
`My Clippings.txt` (unstructured, no stable ids, no linking) or the Kindle
Notebook web page. The annotation is *a scrap attached to a location*. There is
no concept of a highlight relating to anything except the book it came from.
Nothing exists after you finish the book except a star rating.

**KOReader.** Best-in-class *while reading*. Per-book `.sdr` sidecar with
highlights, bookmarks, position; the statistics plugin keeps time-read and
pages/day in its own `statistics.sqlite3`. Its limits are structural, not
oversights:
- notes are **per-book silos** — the sidecar format cannot express "this relates
  to a passage in another book", because it is a file that lives next to one
  epub;
- composing anything longer than a sentence on an e-ink keyboard is punitive;
- there is no library *presentation* — a file browser is not a shelf.

KOReader is an in-the-moment tool. It is excellent at capture and structurally
incapable of synthesis.

**Calibre.** Cataloguing and conversion. Its "library" is a spreadsheet with
cover thumbnails. Superb at *files*, indifferent to *reading*.

**Readwise / Obsidian / Zotero.** The after-reading market, split three ways:
Readwise owns the highlight pipeline (SaaS, subscription, your private reading
on someone else's server), Obsidian owns the vault (but it is a text editor that
happens to hold highlights — it has no idea what a book is), Zotero owns
citation (and is hostile to anyone who isn't writing a paper).

### The gap

Nobody owns the **after-reading loop, self-hosted and terminal-native**. The
unclaimed position is not "another reader" and not "another note app". It is:

> **The desk you sit at after you put the book down** — with the library on the
> shelf behind it.

The division of labour is explicit and, importantly, *honest about what already
works*: KOReader/Kindle is where you read. readingbuddy never tries to be a
reader. Everything it does is downstream of a device that already reads well.

That framing has a second consequence worth stating: **the unit of value is not
the highlight, it is the connection between highlights.** A single highlight is
already served by KOReader. Two highlights from different authors that turn out
to be about the same thing are served by nothing that runs on your own machine.

### Four positions, and what each costs

| | Pitch | Strength | Cost |
|---|---|---|---|
| **A. The desk** | Capture on device, think here | Honest, matches `koreader.rs` today | Two-app friction; user must leave the reading device |
| **B. Self-hosted Readwise** | Own your highlight pipeline + SRS | Crisp value prop; `flashcards.rs` is half-built | Invites comparison with a mature product (article capture, OCR, Kindle sync) |
| **C. The display case** | A library you enjoy looking at | The 3D renderer is genuinely unique; nobody has this | A demo unless the data behind it is deep |
| **D. Zettelkasten for readers** | Vault is the product, books are the index | Matches the stated cross-referencing goal | Competes head-on with Obsidian, which is very good |

**Recommendation: A + C + D, with B as a feature rather than an identity.** The
desk is the frame, the display case is the front door, the zettelkasten is the
substance. "Self-hosted Readwise" is a thing it *does*, not a thing it *is* —
claiming it as identity means losing on a feature checklist to a company.

### What a framework like this can do that e-readers structurally cannot

Ranked by "impossible elsewhere", not by effort:

1. **Cross-book link resolution.** A highlight in book X wikilinks to a note
   that also cites book Y. The engine already keeps `note_links` with dangling
   targets held as text and back-resolved when the target appears — that is
   exactly the substrate for "what else have I written near this idea", and it
   is already built. What's missing is the *surface*: a backlinks pane, and a
   "related" query.

2. **Author view / corpus view.** Everything you've highlighted by one author
   across all their works, orderable by publication date *or* by when you read
   it. "This author changed their mind between book 2 and book 5" is only
   visible if the highlights are pooled — KOReader cannot see across sidecars,
   by construction.

3. **The review as a first-class object.** Not a note. A long-form artefact
   with a rating, a finished date, and **citations by reference into the
   highlight table** — so the review stays live if you edit a highlight, and so
   you can ask "which highlights did I actually use?". Exports to markdown/HTML
   for a blog or as a Goodreads replacement. Today a note anchors to
   page/location/highlight; a review is the same machinery anchored to the
   whole book plus a citation list.

4. **Orphan queue.** Highlights you never wrote about. This is the single best
   candidate for "what the app opens to" — Readwise's daily review, but
   goal-directed: the queue empties as raw capture becomes written thought.
   Cheap to build (a left join), high daily pull.

5. **Reading sessions as narrative.** KOReader's stats DB has the raw data.
   Imported, it gives "read over six weeks, highlighting concentrated in ch. 4"
   — which is context for a review, and material for a year-in-review page.

6. **Quotation surface.** Highlight → formatted citation with page and edition
   ISBN. Zotero territory, minus the academic tax.

7. **Round-trip back to the device.** Write on the desk, push into `.sdr` so
   the note is there while reading. Nobody does this. It is also the riskiest
   thing on the list — KOReader owns that file and will rewrite it.

### The display case, taken seriously

The 3D renderer currently shows one book. Its spine thickness already comes from
`page_count` and its front aspect from the real cover, which means **a shelf is
mostly free**: the same `Model` repeated along an axis is a physically honest
row of books. That unlocks:

- a shelf/wall view of the library, spine-out, thickness true to length;
- "currently reading" pulled proud of the row;
- a finished-books wall, and a year-in-review shelf;
- the book pulling out and turning to cover-forward on selection — which is
  exactly the existing single-book scene, so the transition is one camera path.

This is the part of the app no competitor can copy quickly, and the part that
makes it feel like a place rather than a tool.

---

## Calibre: yes, there is a real CLI, and it's installed here

**calibre 7.26.0, `/opt/homebrew/bin`.** Calibre is CLI-first underneath — the
GUI is a shell over the same Python library — so this is a supported interface,
not a hack.

| Tool | What it gives us |
|---|---|
| `ebook-convert IN OUT [opts]` | **The converter.** Format inferred from extension. epub/mobi/azw3/pdf/fb2/docx/txt/htmlz/rtf/lit/pdb/lrf. Deep option set (`--output-profile=kindle`, `--embed-all-fonts`, per-format groups). Exit code + stderr; **no JSON output** |
| `ebook-meta FILE` | Read/write metadata in place; `--get-cover`, `--to-opf` |
| `calibredb` | The library DB. `list --for-machine` emits **JSON**; also `add`, `export`, `search`, `fts_search`, `show_metadata --as-json`. Can target a remote content server over HTTP |
| `fetch-ebook-metadata` | Their federated metadata search — overlaps `providers/` |
| `calibre-debug -e script.py` | **The escape hatch**: arbitrary Python against calibre's full library API |
| `calibre-server` | HTTP + OPDS |

Three ways to use it, in increasing ambition:

- **(i) Opportunistic conversion.** Feature-detect `ebook-convert` on PATH; if
  present, offer format conversion on import/export; if absent, the feature is
  simply not there. Cheap, and consistent with the standing rule that we never
  ask the user to configure other software.
- **(ii) Calibre library as an import source.** `calibredb list --for-machine`
  → JSON of a library the user has *already curated*, with covers, ISBNs, tags,
  series. This is the single biggest onboarding win available: a new user goes
  from empty to a full shelf without typing one ISBN.
- **(iii) Push to device.** `calibre-smtp` / `ebook-device`. Probably out of
  scope.

**Licensing note, since `deny.toml` already tracks this:** calibre is GPL-3, but
**shelling out to a separate binary is not linking** — no contamination. This is
strictly better than the current in-process GPL dependency (`epub =2.1.4`).

---

## Questions for you

Answer inline under each — I'll write round 2 against your answers.

### Q1. Is the reading device in or out?
Does readingbuddy ever display book *text*, or is it strictly post-reading?
(a) Strictly a desk — never renders a page. (b) Read-only excerpt view around a
highlight, for context when writing. (c) Eventually a full reader in the TUI.

> **A:**

### Q2. What does the app open to?
The home screen defines the product. (a) The shelf/display case. (b) The orphan
queue — "12 highlights you haven't written about". (c) Currently-reading, with
progress. (d) Today's note.

> **A:**

### Q3. Reviews — separate object, or notes with a flag?
A `review` table (rating, finished date, citation list, export target) is more
honest but is schema surface. Or: a note whose anchor is the whole book, plus a
`kind` column. Which?

> **A:**

### Q4. Where does cross-referencing surface?
The engine can already resolve links. Do you want (a) a backlinks pane on every
book/note, (b) an author/corpus view pooling highlights across works, (c) a
graph view, (d) a search-driven "related passages" query — and which *first*?

> **A:**

### Q5. Is the vault Obsidian-compatible on purpose?
Notes are already markdown with frontmatter in `vault/`. Is the goal that a user
opens `vault/` in Obsidian and it Just Works (constrains link syntax, filenames,
frontmatter keys forever), or is the vault our format that happens to be
markdown?

> **A:**

### Q6. Calibre — which of the three?
(i) conversion only, (ii) + import an existing calibre library, (iii) + device
push. And: hard dependency, or feature-detected and degrades?

> **A:**

### Q7. Shelf view — worth it?
It's the differentiator and mostly reuses the existing scene, but it is real
render work (multi-object scene, camera path, selection). Is it a near-term
priority or a later reward?

> **A:**

### Q8. Who is the second user?
Everything above assumes you. Is there a second user in mind — and if so, is it
"KOReader power user", "Obsidian user who reads a lot", or "someone who wants a
pretty library"? The three want different first screens.

> **A:**

### Q9. Multi-device reality?
Is the KOReader device on the same filesystem (mounted USB, syncthing), or is
import a manual copy? This decides whether import is a background watcher or an
explicit command.

> **A:** Linkability between the KOReader device and readingbuddy is essential —
> a big part of the point is a device-linking workflow that *feels* easy. Wired
> and wireless both. A custom KOReader plugin is on the table.

---

# Round 2

## What your answers settled

| | Decision |
|---|---|
| **Reader** | Strictly a desk to start. Ceiling is a read-only excerpt around a highlight. Ebook *upload* is wanted, paired with calibre import |
| **Home screen** | Currently-reading, with progress |
| **Orphan queue** | Not core. Later, if ever |
| **Reviews** | Separate object from notes — and the **central hub that a lot of links pass through** |
| **Cross-refs** | Backlinks + author/corpus + graph, all three; cheapest first |
| **Vault** | Ours first. readingbuddy is the home and primary place for every datum it stores. Obsidian compatibility is a courtesy, never a constraint |
| **Calibre** | All three tiers, in the stated order of importance. Feature-detected, degrades silently, never a hard dependency |
| **Shelf view** | Deferred. TUI graphics are done as they are — the real rendering lands in the GUI |
| **Audience** | You. Obsidian community is the likely advertising channel if it ever needs one |
| **Device** | Essential. Wired + wireless. Plugin considered |
| **Frontends** | TUI stays. A **SwiftUI Mac app** joins it — widget + menu-bar compatible |

## The axiom, stated with teeth

> **A place, not a tool.**

Kept as a decoration this means nothing, so here is what it actually forbids and
requires:

- **The app is the home of its data, not a view onto someone else's.** Already
  answered in Q5, but it generalises: calibre, KOReader and Obsidian are
  *peripherals*. Data arriving from them gets copied in and owned, never
  referenced in place and never authoritative elsewhere.
- **State persists and is visible.** A place you return to remembers where you
  were. Currently-reading as home is exactly this; so is remembering pane
  layout, last section, scroll position.
- **Nothing is modal-by-default and nothing is a dead end.** Already a stated
  TUI rule (the key bar is always present); promote it to a product rule.
- **Idle is not blank.** The ambient layer and the turning book exist for this
  reason. It is why they earn their cost.
- **No task-completion framing.** This is what kills the orphan queue as a home
  screen and your instinct against it was right — a badge counting unwritten
  highlights turns a study into an inbox. Keep it as a *place you can go*
  (a section), never a number that greets you.

Worth promoting into `CLAUDE.md` once round 3 settles, because it decides
arguments that would otherwise get re-litigated per feature.

## SwiftUI changes the architecture question, not the architecture

The good news is structural: the engine's existing **zero-terminal-I/O rule and
typed `EngineError`/`Diagnostic` enums are exactly the discipline a second
frontend needs.** Nothing has to be untangled first. `Diagnostic`'s
`Clone + Eq`, kept for the TUI status buffer, is also what makes it a clean
value type across a language boundary.

Two real questions, though:

**1. In-process bindings, or a daemon?**

- **UniFFI** (`cargo` staticlib + generated Swift): one process, no lifecycle
  to manage, async supported. This repo is already provisioned for it — there
  is a `rust-ffi-reviewer` agent configured for UniFFI/UDL correctness.
- **A daemon** (`readingbuddyd`, a fourth crate) with TUI, CLI, SwiftUI *and
  the KOReader plugin* all as clients.

The daemon looks like over-engineering until Q9 lands: **a KOReader plugin
pushing annotations over the network needs a listener anyway.** Once that
listener exists, the daemon is paid for, and the GUI riding it is free. That is
the argument, and it comes from device-linking, not from the GUI.

**2. SQLite has one writer.**

This is not hypothetical once there are two frontends plus a widget. A macOS
widget is a **separate extension process** — it cannot share a writer with the
host app safely, and App Group + WAL is a coordination problem, not a solution.
The cheap and correct answer: **the widget never touches the database.** The
host app writes a small JSON snapshot (current book, cover, progress, streak)
into the App Group container and calls `WidgetCenter.reloadTimelines`. Widget
reads a file. Done. Do not let the widget become a database client.

## Device linking, concretely

Three mechanisms, increasing in effort and in how good they feel:

**Wired — the free win.** Kobo/Kindle mount as mass storage; on macOS that is
`/Volumes/KOBOeReader` or similar. Watch for the mount, walk it for `.sdr`
directories, import. **Plug the device in and the import already happened** —
no command, no path typed. This is the highest ratio of "feels easy" to work on
the whole list and it needs nothing from KOReader.

**Wireless without a plugin.** KOReader ships cloud-storage and SSH/rsync
plugins; the user's own Syncthing over the library directory also works. All of
these reduce to "the sidecars appear in a local directory", i.e. the same
watcher as the wired path with a different root. Cheap, because it is the same
code — but it is *plumbing the user has to arrange*, which is the opposite of
the goal.

**A custom KOReader plugin — the one that actually feels like linking.**
KOReader plugins are Lua in `koreader/plugins/<name>.koplugin/main.lua`, can do
HTTP, and can hook document events (close, end-of-book, highlight added). So:
device shows a pairing code, app is discovered on the LAN, and closing a book
pushes its annotations. That is a genuinely novel workflow — nothing in this
space does push-from-device.

One constraint it must respect, and it falls straight out of the standing rule
that **features arrange other software's state themselves, reversibly, and
never by asking the user to edit a config**: readingbuddy should *install* the
plugin onto the mounted device itself, over the wired path, and remove it
cleanly on unlink. Wired thus becomes the bootstrap for wireless — which is a
nice ordering, since wired is also the cheapest thing to build.

## Two things we are currently leaving on the floor

Both found while grounding the above, both directly serving your Q2 and Q3
answers:

**1. `koreader.rs` ignores the sidecar's `summary` table.** The parser handles
`annotations` and legacy `highlight`+`bookmarks` — `grep` finds no handling of
`summary`, `rating`, `status`, or `percent_finished`. The sidecar's `summary`
carries the **rating, the status (reading/complete/abandoned), and the user's
own review text**, and the sidecar root carries `percent_finished`. That is:
- your home screen's progress, from the device, free;
- and *KOReader's review, importable directly into the review object* — which
  is the strongest possible argument for your Q3 answer. The review is not a
  note-with-a-flag partly because **it already exists as a distinct record on
  the device.**

**2. The sidecar's `stats` subtable carries KOReader's partial-`md5` book
identity**, which is also the join key into the device's `statistics.sqlite3`
(per-page durations, total read time). That is both a **hard book identity** —
better than our current title/author matching — and the raw material for
"reading sessions as narrative".

*Caveat, stated because it matters:* the above is from the documented sidecar
format, and there is **no real sidecar in this repo to check it against** —
`tests/fixtures/koreader/real/` holds only its README, and the synthetic
fixtures contain no `summary` (correctly: the generator must not learn the
format from our parser). **Round-3 action: drop one real export into `real/`
before any of this is designed against.** The tier-1 generator then gets a
`summary`/`stats` case.

Note also that `notes.kind` is already `note | session | final` — `final` looks
like an earlier stab at "the review". Worth deciding explicitly whether the new
review object supersedes it or whether that column gets cleaned up.

## Proposed build order

Ordered by (serves a settled decision) x (cheap) x (unblocks something else):

1. **Sidecar `summary` + `percent_finished` + `stats.md5` import.** Feeds the
   home screen, seeds the review object, and upgrades book matching from fuzzy
   to exact. Pure engine, fully testable offline, no new UI. Needs a real
   sidecar first.
2. **The review object.** Schema + engine + a TUI surface. Its links are the
   reason cross-referencing gets interesting, so it precedes cross-ref work.
3. **Currently-reading home screen.** Small once (1) supplies real progress.
4. **Backlinks pane.** `note_links` already stores the edges with dangling
   targets back-resolved; a backlinks view is close to one query. Cheapest of
   your three Q4 answers, so it goes first.
5. **Wired device watcher.** Mount → import, no command typed.
6. **Calibre tier (i) + (ii).** Feature-detected conversion, then
   `calibredb list --for-machine` library import — the onboarding win.
7. **Ebook upload / owned files.** Needs a `book_files` table (format, path,
   hash); pairs with (6) and is the precondition for the excerpt view.
8. **Author/corpus view.** Q4 (b).
9. **KOReader plugin + wireless push.**
10. **Graph view, excerpt view, orphan queue, shelf** — GUI-era or later.

**One correction to something I let stand in round 1:** you called the excerpt
view (Q1b) cheap, and it isn't, quite. A KOReader `pos0` is an xpointer into
*cre's* DOM for that specific document — resolving it means reimplementing
enough of that engine to agree with it. The cheap version is to **search the
epub for the highlight's text** and show the surrounding paragraph, which works
for the overwhelming majority of highlights and degrades visibly rather than
wrongly when it misses. Worth knowing before it gets scheduled as a small task.

## Round-2 questions

### Q10. Daemon, or bindings?
Given the plugin needs a listener anyway: `readingbuddyd` as a fourth crate with
every frontend as a client, or UniFFI in-process now and revisit when the plugin
is real? The second is less to build today and more to unpick later.

> **A:**

### Q11. Is the GUI near-term or horizon?
It changes what's worth building in the TUI now. If SwiftUI is within a few
months, TUI work should stay thin and the engine should absorb everything.

> **A:**

### Q12. What is a review, exactly?
Rating scale (KOReader uses 1–5 stars — match it?), one review per book or per
*reading* of a book, and does it live in the vault as markdown like notes do, or
in the DB as a record? Vault keeps it Obsidian-visible and editable; DB makes
the citation graph easier.

> **A:**

### Q13. "Central connection between a lot of links" — what links to what?
Do you mean the review cites highlights, or that reviews link to *each other*
and to notes — so the review is the node the graph actually hangs off, and
book-to-book connection is really review-to-review?

> **A:**

### Q14. Can you drop a real `.sdr` into `tests/fixtures/koreader/real/`?
It's gitignored. Everything in build-order item 1 is guesswork until one exists.

> **A:**

### Q15. Owned ebook files — copy or reference?
Ebook upload plus calibre import: does readingbuddy **copy** files into its own
data root (consistent with "the home of its data", costs disk, duplicates a
calibre library), or reference them in place (cheap, but breaks the axiom and
breaks when calibre moves things)?

> **A:**

### Q16. What does `notes.kind = 'final'` become?
Superseded by the review object, or repurposed?

> **A:** Next iteration.

---

# Round 3

## Correction: the ownership axiom was overstated

You're right, and the round-2 wording was wrong. "Copied in and owned, never
authoritative elsewhere" would make readingbuddy claim highlights it did not
create and cannot recreate — while KOReader is precisely where reading and
light note-taking should happen, and therefore is the origin of that data.

Restated properly:

> **Authority is per-field and provenance is recorded. readingbuddy keeps a
> durable, queryable local copy of everything, but does not claim to be the
> *origin* of what it copies.**

| Origin | Data |
|---|---|
| **KOReader** | Highlight text/position/colour, the note attached at capture time, reading position and percent, per-page reading time, the on-device status and rating |
| **Calibre** | The file, its format, the metadata the user curated there |
| **Providers** | ISBN, publisher, page count, cover, description |
| **readingbuddy** | The vault, links, reviews, cross-book structure, flashcards, everything synthesised across books |

Copy-in buys durability and cross-book queryability. It does not buy authority.
Where a field has an external origin, conflicts resolve toward the origin and
our copy is refreshed — which is a different rule from "we own it now".

**This has a live consequence worth catching before it bites.** The idempotency
key is `identity_hash = sha256(book_id | ko_datetime | pos0 | text)` — the
highlight *text is inside the hash*. So if a highlight's text is ever editable
in readingbuddy, the next device import computes the original hash, finds no
match, and inserts the original back alongside the edit. Duplicates, silently.
Two ways out, and it should be decided before any highlight-editing UI exists:

- **Imported highlights are read-only in readingbuddy** — you annotate them, you
  don't rewrite them. Cleanest, and consistent with the corrected axiom.
- Or the identity hash drops `text`, and edits live in an overlay column with
  the original preserved. More machinery, and `pos0` alone is a weaker key.

The first is the one the axiom argues for.

## The daemon, reconciled with iOS

You said daemon; you also said Mac **and iOS**. Those collide directly — iOS has
no user daemons and no meaningful background process model. A daemon-shaped
architecture is Mac/Linux only, and an iOS app that is merely a network client
of your Mac is useless the moment you leave the house.

The resolution is to be precise about *what* the daemon is:

> **The boundary is the API, not the process.** Put the engine's whole surface
> behind one versioned request/response API crate. `readingbuddyd` is a thin
> transport wrapper over it — unix socket / loopback HTTP — and holds no logic.

Then:
- **TUI / CLI / SwiftUI on Mac** → clients of the daemon. One writer, which is
  the SQLite problem solved properly rather than worked around.
- **The KOReader plugin** → another client. This is what paid for the daemon.
- **iOS** → the same API crate linked *in-process* via UniFFI. Same types, same
  calls, no daemon, works offline.

So "daemon" is right, with the caveat that the daemon must not become the place
logic lives, or iOS is locked out later.

## GUI frameworks, weighed against what you actually asked for

Your named wants: Mac **and iOS**, a **widget**, and **menu-bar** presence.

| | Widget | Menu bar | iOS | 3D book | Cost |
|---|---|---|---|---|---|
| **SwiftUI** | Yes — the only option | `MenuBarExtra`, real | Yes | SceneKit/Metal, or just display the Rust raster | Two languages, UniFFI boundary, Xcode, App Store for iOS. No Linux |
| **Tauri + Svelte** | **No** | Tray icon only | v2, young | WebGL/Canvas — genuinely good | Rust backend with no FFI (commands are Rust fns), one language-ish, Linux for free |
| **egui** | No | No | Rough | Trivial — embeds the existing render directly | Pure Rust, zero FFI. But weak text input/IME, no native chrome |

Two findings decide this:

1. **WidgetKit extensions must be SwiftUI.** There is no Tauri or egui path to a
   real widget. If the widget is a want rather than a wish, SwiftUI is not one
   option among three — it's the only one.
2. **egui fails the axiom.** "A place, not a tool" is the whole differentiator,
   and egui's default register is *debug tool*; reaching a warm, inhabited feel
   costs more custom work than the FFI it saves. It is the right choice for an
   inspector, which is not what this is.

**Recommendation: SwiftUI**, with Tauri as the hedge if Linux ever matters more
than the widget. The FFI cost is smaller than it looks given the API-crate
design above — one typed boundary, and there's already a `rust-ffi-reviewer`
agent provisioned for UniFFI correctness.

Worth noting the 3D book is nearly a non-issue: `render3d/raster.rs` already
produces true RGBA at a requested resolution, so SwiftUI can display a
Rust-rendered image rather than reimplementing the scene. The renderer survives
the frontend change intact — which is another argument for freezing it now, as
you did in Q7.

Also, plainly: **iOS has no KOReader.** So iOS is a review/browse/capture
surface, not an import surface. That changes what it's for. See Q19.

## Reviews: two faces, one record

Your split is the right one and it dissolves Q12 and Q13 together:

- **The private review** — the personal agglomeration. Where final thoughts get
  tied together, where the book connects to other books. This is the graph hub:
  the node that cites highlights, links notes, and links *other reviews*. It is
  the answer to "what does book-to-book connection actually run through".
- **The public review** — rating plus prose written to be read by someone else.
  Goodreads/StoryGraph shaped. Short, spoiler-aware, no half-formed thoughts.

On "one source of truth, two versions": **achievable as one record, not as one
text.** One review row per book — one rating, one finished date, one link set,
one provenance — with **two bodies**. The public body is *drafted from* the
private one by an explicit action, then diverges. Trying to derive it
automatically (a `public:` frontmatter list, or a `--- public ---` divider)
sounds elegant and produces bad public reviews, because a public review isn't a
subset of private thinking — it's a rewrite for a different audience. The shared
truth is the *record*; the bodies are two documents.

Vault-vs-DB then resolves itself: both bodies are markdown in the vault (so
Obsidian sees them, so you edit them in the same editor as notes), and the DB
holds the record, the rating, and the citation edges. That's the same split the
notes system already uses.

### Goodreads: the API is dead, CSV is the interface

Worth knowing before designing around it — **Goodreads shut down its public API
in December 2020.** No new keys, existing ones deprecated. There is no
integration to build against.

What does exist is CSV, both directions, and it's better than an API would be
for our purposes:

- Export gives `Title, Author, ISBN, ISBN13, My Rating, Publisher, Number of
  Pages, Year Published, Original Publication Year, Date Read, Date Added,
  Bookshelves, Exclusive Shelf, My Review, Private Notes, Read Count` — which
  maps close to 1:1 onto our `books` table *plus the review object*, public body
  and private notes included.
- StoryGraph consumes the same format, so one exporter serves both.

That makes **Goodreads CSV import an onboarding win on the scale of the calibre
one** — a user's entire reading history, ratings and reviews included, in one
file. And CSV export is how the public review leaves the building. It belongs in
the build order.

## Owned files, and the deduplication you flagged

readingbuddy owns the files. Dedup matters because three streams (manual upload,
calibre import, device pull) will present the same book under different names.
Three distinct levels, and conflating them is the usual mistake:

1. **Same bytes.** sha256 of the file. Store content-addressed —
   `database/files/<ab>/<sha256>.epub` — and dedup is free and re-import is
   idempotent by construction. Original filename is a column, not the path.
2. **Same book, different file.** epub + azw3 of one book are two files, one
   `book_id`. So `book_files(book_id, sha256, format, original_name, size)`,
   many-to-one.
3. **Same book, different bytes, unknown.** Resolution order: ISBN from embedded
   metadata → **KOReader's partial hash** → the fuzzy title+author fingerprint
   `search.rs` already computes with jaro-winkler. Reuse the last one rather
   than inventing a second matcher.

Level 3's middle step is the interesting one: KOReader identifies books by a
partial hash over sampled chunks (`util.partialMD5`), and that same value is the
join key into the device's `statistics.sqlite3`. Computing it ourselves means
**our file identity agrees with the device's** — the same value dedups our
library, matches a sidecar to a book, and pulls the reading-time history. One
hash, three jobs. It's worth implementing early for that reason alone.

There is a schema gap here: there is no `book_files` table today. It's a new
migration whenever upload or calibre import lands.

## Installing the plugin on the device: yes, it lives on the reader

Correct — KOReader plugins are Lua running in KOReader's own VM on the device,
at `<device>/koreader/plugins/<name>.koplugin/main.lua`. So "install" is
literally copying one directory onto the mounted device. Nothing runs on our
side except the listener.

That is a write to someone else's device, so the guarantees have to be explicit:

- **Verify first.** Refuse unless the mount really is a KOReader install
  (`koreader/` present with its expected contents). Never write to an
  unrecognised volume.
- **Stay in our own directory.** Only ever
  `koreader/plugins/readingbuddy.koplugin/`. Never touch
  `settings.reader.lua`, never touch another plugin, never touch a book file.
- **Create-only; upgrade replaces only our directory.** Never modify a file we
  did not write. Refuse to overwrite a *newer* plugin version than ours.
- **Never automatic.** Mount → import is automatic and read-only, and that's the
  feature. Mount → install is an explicit one-time action that shows the exact
  destination path first.
- **Uninstall is exact and complete** — remove that one directory, nothing else.
- **The plugin fails closed.** Cannot reach the app → does nothing, silently,
  and never blocks or slows the reader's UI. A reading device that stutters
  because a sync plugin is retrying is worse than no plugin.

This is also the standing rule about other software's state being honoured
rather than broken: we arrange it ourselves, reversibly, instead of asking you
to hand-edit a config on the device.

## Real sidecars: where to actually get them

No curated public corpus of `.sdr` directories exists — they're personal reading
data, so what's on GitHub is accidental commits in backup/dotfile repos:
scattered, unrepresentative, and not really ours to vendor.

Three better sources, in order of usefulness:

1. **Mint them with KOReader itself.** KOReader ships desktop builds (Linux
   AppImage, macOS). Run it against the epubs already in `epubs/`, make
   highlights, notes, a rating and a summary, close the book — and harvest
   genuine sidecars, including the `summary` and `stats` tables round 2 found
   missing. Authoritative, repeatable, legally clean, and it satisfies the
   standing rule that fixtures are generated by something *other than our own
   parser* — here the generator is the reference implementation itself, which is
   the ideal case.
2. **Read the KOReader source as the spec.** `docsettings.lua`,
   `readerannotation.lua`, `readerstatistics.lua` pin the format harder than any
   fixture can, and settle the `summary`/`percent_finished`/`stats.md5`
   questions definitively.
3. **Your own export**, still — for the realism the synthetic tiers can't reach:
   a long book, hundreds of highlights, CJK, a legacy-format sidecar from an old
   KOReader version.

(1) and (2) unblock build-order item 1 without waiting on anything.

## Revised build order

Reordered by your round-2 answers — TUI-and-engine focus, GUI deferred until the
engine has a definite shape, so anything that hardens the engine moves up.

1. **KOReader format work, done properly.** Read the source, mint real sidecars
   from desktop KOReader, then implement `summary` (rating/status/review text),
   `percent_finished`, and `stats.md5`. Gives the home screen its progress,
   seeds the review object with the device's own review, and upgrades book
   matching from fuzzy to exact.
2. **`partialMD5`.** Small, and it's the shared key for dedup, sidecar matching
   and the statistics join.
3. **The review object.** One record, two bodies, provenance, citation edges.
   Everything downstream hangs off this.
4. **Currently-reading home screen.** Cheap once (1) supplies real progress.
5. **Backlinks pane.** `note_links` already holds the edges; the cheapest of the
   Q4 three.
6. **Goodreads CSV in/out.** Import is a whole reading history; export is how
   the public review leaves. Pure engine, offline, testable — good engine work.
7. **Wired device watcher.** Mount → import, nothing typed.
8. **`book_files` + owned files + the three-level dedup.**
9. **Calibre (i) conversion, then (ii) library import.**
10. **The API crate + `readingbuddyd`.** Once the engine's shape is definite —
    which is exactly the condition you set for starting the GUI, and this is the
    thing the GUI needs first.
11. **KOReader plugin + wireless push**, with the safety rules above.
12. **Author/corpus view, excerpt view (text search), graph, orphan queue,
    shelf, SwiftUI.**

## Round-3 questions

### Q17. Are imported highlights read-only?
The identity-hash issue above forces a decision before any editing UI. Read-only
and you annotate around them, or an overlay column with the original preserved?

> **A:**

### Q18. Is the widget a real requirement?
It's the single fact that makes SwiftUI the only option. If it's a nice-to-have,
Tauri + Svelte becomes competitive and buys Linux and one less language.

> **A:**

### Q19. What is the iOS app *for*?
No KOReader on iOS, so it can't be an import surface. Reviewing and browsing
away from the desk? Capture from paper books? Just the library as a display
case in your pocket?

> **A:**

### Q20. Does the public review get published from here?
CSV export to Goodreads/StoryGraph is one thing; "generate a blog post / static
page" is another and pulls in a whole rendering path. In or out?

> **A:**

### Q21. Rating scale?
KOReader is 1–5 stars, Goodreads is 1–5 stars. Match them (lossless both ways,
coarse), or keep something finer internally and round on export (expressive,
lossy at both borders)?

> **A:**

### Q22. One review per book, or per reading?
`Read Count` is a Goodreads column and re-reads are real. One review that
accretes, or a review per reading with the book showing a history?

> **A:**

### Q23. Does the device watcher ever write to the device?
Import is read-only, which is safe. But KOReader's own status could be updated
from readingbuddy (mark finished here → device agrees). Strictly one-way for
now, or is two-way progress sync a goal?

> **A:** One-way for now. Two-way is its own goal, and needs the sync and plugin
> infrastructure working very well first.

---

# Round 4

## Highlight text frozen, annotation live — and the seam that needs naming

Right split. It maps onto an existing seam that is currently blurred, though,
and the blur is worth fixing before anything is built on it.

`highlights.note` is documented as "KOReader-attached note" — i.e. **the device
owns that column**. So there are two different things both wanting the word
"annotation":

| Field | Origin | Editable here |
|---|---|---|
| `highlights.text` | KOReader | **No** — frozen, it's in the identity hash |
| `highlights.note` | KOReader | No — it's theirs, refreshed from the device |
| *(new)* `highlights.annotation` | **readingbuddy** | **Yes** — this is your reaction |

Three notes on that:

- The names will cause a bug if left as `note` and `annotation`. Better to make
  the ownership visible in the schema: `ko_note` (theirs) and `annotation`
  (ours). That's a rename migration, which is fine — the rule is never to *edit*
  an applied migration, not never to supersede one.
- `notes.highlight_id` already exists, so a full vault note can already anchor to
  a highlight. That stays for real writing. A one-line reaction shouldn't become
  a file with frontmatter, though, so the lightweight `annotation` column earns
  its place — with **"promote to note"** as the action when a reaction grows into
  something. That's a nice affordance and it keeps the vault uncluttered.
- Our annotation is *ours*, so it is never at risk from re-import. That's the
  whole reason for the split, and it makes the identity-hash problem go away
  rather than needing to be managed.

### A gap this exposes, verified

`crates/engine/src/storage/highlights.rs:55` is
`ON CONFLICT(book_id, identity_hash) DO NOTHING`, and `note` is **not** part of
the identity hash. So: you edit a highlight's note **on the device**, re-import,
and the change is silently discarded — the row exists, conflict fires, nothing
updates. The harness asserts exactly this ("no row mutates" on re-import), so
it's deliberate as written, but it directly contradicts the corrected axiom:
KOReader is the origin of that field, so the device should win on refresh.

The fix is narrow — conflict-update `ko_note` (and colour, and `ko_datetime` if
it moves) from `excluded`, while leaving everything we own untouched. Which is
the same `COALESCE`-style no-clobber merge the books upsert already does. See
Q24, because it changes a golden.

## The widget: yes, and the clean shape isn't the obvious one

Short answer: **yes, possible** — but not by embedding a widget into the Tauri
bundle, which is the version that sounds right and is the fragile one.

The constraint is that a WidgetKit widget is an app *extension*; it can't be
standalone, it's always sandboxed, and it can only read the **App Group
container** it's entitled to. What it does *not* require is being inside the
same app as your main program — two apps from the same developer team can share
an App Group.

So the workable shape on macOS:

- **Tauri desktop app** (not sandboxed, self-distributed) writes a small
  snapshot — current book, cover, progress — into
  `~/Library/Group Containers/<group-id>/`. For a non-sandboxed app that's just
  an ordinary directory; no entitlement gymnastics.
- **A tiny SwiftUI companion** owns the widget extension, is entitled to that
  App Group, and reads the snapshot. It does almost nothing else.

This is the same rule round 2 already landed on — *the widget never touches the
database, it reads a snapshot* — and it turns out to also be what makes the
cross-framework version work at all. Embedding an Xcode-built extension into a
Tauri `.app` and re-signing the result is possible but is a notarization and
entitlement minefield for no gain.

Caveat: Mac App Store distribution would force the Tauri app to be sandboxed and
properly entitled for the group, which is a real chunk of work. Self-distributed,
this is easy.

On iOS there's no Tauri app to pair with, so it's a standalone SwiftUI app —
which matches "read-only display" fine. Cheapest data path for a read-only iOS
app is the same snapshot, synced via iCloud Drive, rather than any live
connection.

### That reverses round 3's framework recommendation

Round 3 said SwiftUI on the strength of one fact: only SwiftUI can ship a
widget. You've now descoped the widget and iOS to display-only companions, and
the companion pattern above means they don't constrain the main app's framework
at all.

With that gone, **Tauri + Svelte is the stronger pick for the main desktop app**:

- **No FFI.** The daemon/API-crate design from round 3 is Rust; a Tauri backend
  is Rust. It can link the API crate directly or speak to `readingbuddyd`. The
  SwiftUI route pays a UniFFI boundary for the main app's entire surface.
- **One language across engine, daemon, CLI, TUI and GUI backend.**
- **Linux for free**, which SwiftUI never gives.
- The 3D book is a non-issue either way — `raster.rs` already emits RGBA, so the
  frontend displays a Rust-rendered image.

SwiftUI keeps two genuine advantages: native menu-bar presence (`MenuBarExtra`
vs a tray icon) and a better-feeling Mac app, which is not nothing given the
axiom. But it's now a preference, not a forcing function. See Q25.

## Reviews: two objects, not one, and they need different names

Scrapping the shared record simplifies things a lot. But once they're fully
separate, calling both of them "review" will cause the same confusion
`note`/`annotation` would have. Proposed:

- **Reflection** — private. The personal agglomeration node: final thoughts,
  ties to other books, contemplation. **This is the graph hub** — it holds the
  citation edges into highlights and the links to notes and to other
  reflections. Book-to-book connection runs reflection-to-reflection.
- **Review** — public. Rating plus prose written to be read. Goodreads/StoryGraph
  shaped, spoiler-aware, exportable.

Both markdown in the vault, both with a DB record, both anchored to a *reading*
(below). The rating lives on the **Review** only — it's a public artifact. A
reflection that wants a private score can have one later; it isn't the same
number and shouldn't share a column.

## Rating: user-defined scale, user-defined export mapping

Doable, with one hard external constraint worth knowing up front: **Goodreads'
CSV `My Rating` is an integer 0–5** (0 meaning unrated) — no halves. StoryGraph
accepts quarter-stars. So a user-defined mapping must land on integers for
Goodreads specifically, and the app should say so rather than silently rounding.

Shape that fits:

- A **scale** the user defines: either numeric (`min`, `max`, `step`) or an
  **ordered list of labels** — which is the interesting case, since it allows
  non-numeric systems (a tier list, "reread / keep / pass") that no other
  reading app supports.
- A **mapping table** the user edits: each scale value → an integer 0–5. Explicit
  lookup, not a formula. Formulas are shorter to store and always wrong at the
  ends; a table is boring, editable, and honest.
- Store the raw value plus the scale id. Never store only the mapped value —
  that's lossy, and the mapping is user-editable so it must be re-derivable.

## Per-reading infrastructure, built now

Agreed, and building it now is much cheaper than retrofitting. It's also the
most structural change on the list, which argues for doing it early:

A `readings` table — `id, book_id, started_at, finished_at, status, source,
current_page`. Then:

- **Progress moves off `books`.** `books.current_page`, `finished`,
  `date_started`, `date_finished` become properties of the active reading. That
  is a real migration touching the upsert's `finished`-merges-with-MAX logic, so
  it wants doing before more code depends on those columns.
- **Reflections and reviews anchor to a reading**, not a book. Reread a book,
  write a second review, both survive with dates. Goodreads' `Read Count` column
  then imports meaningfully instead of being dropped.
- **Highlights are the awkward case, and honestly so.** KOReader's sidecar is
  per-*file* and a reread appends to the same sidecar — the device does not
  separate readings, so we cannot import that attribution. Best available: keep
  `highlights.book_id` authoritative, add a nullable `reading_id` assigned by
  matching `ko_datetime` into a reading's date window. Correct for the common
  case, unattributed rather than wrong when windows are unknown. It should not
  pretend to more precision than the source has.

## One-way sync now, with two-way not designed out

Agreed on one-way. One cheap thing to do now so two-way stays possible: **record
what we last saw from the device, per field.** A future writer needs to know
whether a difference means "the user changed it here" or "the device changed it
there", and that's unrecoverable if we only ever store the merged result.
Concretely, provenance plus a last-seen-device-value for the fields KOReader
owns. Cheap now, expensive to retrofit, and it doesn't commit us to anything.

## Revised build order

Structural schema work moves up, since it's cheapest before more code leans on
the current shape, and GUI is deferred until the engine has definite shape.

1. **KOReader format work.** Read the source, mint real sidecars from desktop
   KOReader, implement `summary` (rating/status/review text),
   `percent_finished`, `stats.md5`.
2. **The highlight ownership seam.** `ko_note` / `annotation` split, conflict-
   update of device-owned fields, device-provenance columns for future two-way.
3. **`readings` table + progress migration.** The big structural one.
4. **`partialMD5`** — dedup key, sidecar match, statistics join, one value.
5. **Reflection + Review as two objects**, anchored to a reading.
6. **Currently-reading home screen.**
7. **Backlinks pane** — cheapest of the Q4 three, and reflections make it worth
   looking at.
8. **Goodreads CSV in/out** — history import incl. `Read Count`, plus review
   export with the user's rating mapping.
9. **Wired device watcher.**
10. **`book_files` + owned files + three-level dedup.**
11. **Calibre (i) conversion, then (ii) library import.**
12. **API crate + `readingbuddyd`.**
13. **KOReader plugin + wireless push.**
14. **Author/corpus view, excerpt view, graph, orphan queue, shelf, GUI,
    widget/iOS companions, two-way sync, publishing.**

## Round-4 questions

### Q24. Do device-owned fields refresh on re-import?
Fixing the dropped `ko_note` update means re-import is no longer "no row
mutates" — it becomes "no row we own mutates". That's a deliberate change to a
tested invariant and a golden. Confirm?

> **A:**

### Q25. Tauri + Svelte, or SwiftUI, for the main desktop app?
Widget no longer forces it. Tauri buys no-FFI, one language, Linux; SwiftUI buys
a real menu bar and a better-feeling Mac app. Given "a place, not a tool", the
feel argument isn't trivial.

> **A:**

### Q26. Names — Reflection and Review?
Or something else. Whatever they're called, they need to be two words, because
they're now two objects.

> **A:**

### Q27. Does a reading need to exist before highlights can import?
If you import a device with no reading recorded, do we auto-open a reading from
the sidecar's dates, or park the highlights on the book unattributed until you
say otherwise?

> **A:**

### Q28. Can a reflection exist without a finished reading?
Mid-book thinking is real. Is a reflection openable at any point (and just
accretes), or is it a thing you write at the end?

> **A:**

### Q29. Non-numeric rating scales — in scope, or numeric-only v1?
The ordered-labels case is the genuinely novel bit, but it's more UI than a
1–10 slider. Worth it in the first cut?

> **A:** Numeric only for now.

---

# Round 5

## Q24 in depth: why re-import drops device edits, and what changes

### The mechanism, in three facts

**1. Identity is four fields, and `note` is not one of them.**

`crates/engine/src/storage/highlights.rs:22`:

```rust
pub fn identity_hash(&self, book_id: i64) -> String {
    hasher.update(book_id.to_string());  // |
    hasher.update(self.ko_datetime …);   // |
    hasher.update(self.pos0 …);          // |
    hasher.update(&self.text);
}
```

So a highlight is identified by *which book, when it was made, where in the
document, and what text*. `note`, `color`, `chapter` and `page` ride along as
payload.

That is the right call, and worth saying why: if `note` were part of identity,
then typing a note on the device would change the hash, and the next import
would insert a **second copy** of the same highlight. Payload-not-identity is
correct.

**2. On conflict, we do nothing at all.**

`highlights.rs:55`:

```sql
INSERT INTO highlights (…) VALUES (…)
ON CONFLICT(book_id, identity_hash) DO NOTHING
RETURNING id
```

`RETURNING` yields no row when the conflict fires, so `insert_highlight` returns
`None`, and `koreader.rs` counts that as `skipped`.

**3. Therefore payload changes made on the device never land.**

Add a note to an existing highlight in KOReader, or recolour it. `ko_datetime`,
`pos0`, `text` are all unchanged → same hash → conflict → `DO NOTHING`. The row
in our database keeps whatever it had. Nothing warns; the import reports it as
`skipped`, which is indistinguishable from "already had it, identical".

Under the corrected axiom this is wrong: KOReader is the **origin** of `note`
and `color`, so on refresh the device should win.

### An unknown that decides which bug we actually have

We don't know whether KOReader bumps an annotation's `datetime` when you edit
its note. That single fact picks the failure mode:

- **`datetime` stays fixed** → hash unchanged → the edit is silently dropped
  (the case above).
- **`datetime` is bumped** → hash changes → the *same highlight imports again as
  a second row*, and now you have a duplicate with an old and a new note.

The second is considerably worse, and no amount of conflict-handling fixes it —
it'd need `pos0`+`text` as the dedup key with `ko_datetime` demoted to payload.
This is precisely why "read the KOReader source, mint real sidecars" is build
item 1 rather than a nicety: it decides the schema.

### Why the existing test doesn't catch it, and what's actually missing

`reimport_is_strictly_idempotent` (`tests/koreader_import.rs:300`) asserts
`s.inserted == 0` and then that every row is byte-for-byte unchanged.

**The test isn't wrong.** It re-imports the *identical fixture*, and re-importing
identical bytes genuinely should change nothing. What it cannot do is
distinguish "nothing changed because nothing should have" from "nothing changed
because we discard device updates" — because in that fixture there is no device
update to discard.

So the missing thing is **a fixture, not a weaker assertion**. The append case
already has one — `variants/Pachinko-Superset.sdr`, deliberately a committed
fixture rather than a string splice. The payload case needs its sibling:
`variants/Pachinko-NoteEdited.sdr`, identical except one existing annotation's
note text.

With that fixture in place the invariant sharpens from **"no row mutates"** to:

> **No field we own mutates. Device-owned fields track the device. Row identity
> is stable.**

Three assertions rather than one:

1. Identical re-import → nothing changes anywhere. *(the existing test, kept)*
2. Device-field-changed re-import → exactly that field changes; `annotation`,
   `created_at` and `source` are untouched.
3. **`highlights.id` is stable across all of it.** This is the one that would
   really hurt: `notes.highlight_id` and `flashcards.highlight_id` are foreign
   keys (`0001_init.sql:48`, `:67`). A delete-and-reinsert refresh would silently
   null your note anchors and cascade away flashcards. It should be asserted, not
   assumed.

### How to implement it without breaking the counters

The obvious fix is to turn `DO NOTHING` into `DO UPDATE SET …`. Don't — at least
not naively, because `RETURNING id` then yields a row on the conflict path too,
so `Some(id)` no longer means "newly inserted" and the `inserted`/`skipped`
counts (which the goldens assert) collapse into each other.

Cleaner: **leave the insert exactly as it is**, and when it returns `None`, run a
targeted update of device-owned columns. Two simple statements, the new-row path
completely untouched, and you get a third counter — `updated` — that makes the
behaviour visible in the report instead of implicit. That new counter is the
golden change.

One subtlety on the update itself. The books upsert uses
`COALESCE(excluded.x, books.x)` no-clobber merging, and **that pattern is right
for providers and wrong for the device.** A provider returns a *partial* record,
so a missing field means "I don't know" and must not erase what we have. A
sidecar is the *complete* state of that annotation — a missing note means the
user **deleted** the note. So device-owned fields take straight assignment, not
`COALESCE`. Copying the books pattern here would make note deletion impossible
to sync, forever.

### Where the column split lands

| Column | Owner | On re-import |
|---|---|---|
| `text`, `pos0`, `pos1`, `ko_datetime` | KOReader | frozen (they *are* the identity) |
| `chapter`, `page`, `color`, `ko_note` | KOReader | assigned from the sidecar |
| `annotation` *(new)* | readingbuddy | never touched |
| `source`, `created_at`, `id` | readingbuddy | never touched |

Adding a `last_seen_ko_note` alongside is the cheap forward-compatibility move
from round 4: with it, a future two-way sync can tell "the user edited this here"
from "the device changed it there". Without it that distinction is unrecoverable,
because we'd only ever have stored the merged result.

**Confirming Q24 means agreeing to:** one new fixture, one new counter in
`BookImportStats`, regenerated goldens, and the idempotency test splitting into
three named assertions.

## The rest of round 4, settled

- **Tauri + Svelte** for the main desktop app. Two consequences worth noting:
  the API crate matters more (the Tauri backend links it directly, no IPC hop
  needed for the bundled case), and **shipping Linux means the device watcher
  needs Linux mount paths** — `/run/media/$USER/…` and `/media/$USER/…` beside
  macOS's `/Volumes/…`. The SwiftUI widget/iOS companions stay Mac-only extras,
  which is consistent with them being display pieces.
- **Reflection / Review** as the two names.
- **Reflections are mid-book and accrete.** Good consequence: a reflection is
  created with the reading, not at the end, so the currently-reading home screen
  has a natural primary action — *open the reflection for what you're reading*.
  That's the daily loop, and it arrives without the orphan-queue inbox framing
  the axiom rejects.
- **Numeric rating scales only** for v1: `min`, `max`, `step`, plus the explicit
  lookup table to Goodreads' integer 0–5.

## Collections — a noun we hadn't named

"Start in collections" for unattributed highlights implies a **Collection**
object, which hasn't appeared anywhere in this document yet. It turns out not to
be optional: **Goodreads CSV carries `Bookshelves` and `Exclusive Shelf`**, so
the history import already in the build order brings user collections with it
whether or not we plan for them. KOReader has its own Collections feature too,
so the concept exists on both sides of the device link.

So it's worth designing as a first-class thing — books in named collections,
one of which may be exclusive (read / reading / to-read, mirroring Goodreads) —
rather than as a parking lot bolted on for orphaned highlights. See Q30, because
your answer was terse enough that I want to check the reading before it goes in
the build order.

## Revised build order

1. **KOReader format work** — source + real sidecars; `summary`,
   `percent_finished`, `stats.md5`; **and settle the `datetime`-on-edit question**,
   which decides item 2's schema.
2. **The highlight ownership seam** — `ko_note`/`annotation` split, targeted
   device-field update with the `updated` counter, `last_seen_ko_note`, the
   `Pachinko-NoteEdited` fixture, and the three-way idempotency test.
3. **`readings` table + progress migration.**
4. **`partialMD5`.**
5. **Reflection + Review**, anchored to a reading; reflection openable mid-book.
6. **Currently-reading home screen**, with "open reflection" as its action.
7. **Collections.**
8. **Backlinks pane.**
9. **Goodreads CSV in/out** — needs 3, 5, 7 to import losslessly.
10. **Wired device watcher** (macOS + Linux mount paths).
11. **`book_files` + owned files + three-level dedup.**
12. **Calibre (i) then (ii).**
13. **API crate + `readingbuddyd`.**
14. **KOReader plugin + wireless push.**
15. **Author/corpus view, excerpt view, graph, orphan queue, shelf, Tauri GUI,
    Mac widget/iOS companions, two-way sync, publishing.**

## Round-5 questions

### Q30. Collections — what did you mean?
Either (a) unattributed highlights park in a staging collection until you assign
them, or (b) collections are a general first-class feature (Goodreads-style
shelves, exclusive + tagged) and unattributed things simply live in one. (b) is
more work now but the Goodreads import needs it regardless.

> **A:** Some book categorization feature is certainly needed, but defer it if
> possible — it is another source of weird merge cases if Goodreads, KOReader
> and readingbuddy can all create their own collections.

### Q31. Does a reading auto-open on import at all?
Given collections: device import with no reading recorded — does it create a
reading from the sidecar's dates *and* file the book into a collection, or stay
strictly hands-off until you act?

> **A:** Pull up a UI that lets the user pull specific books in from the
> reader; also "sync everything from device" and multi-select. Start with the
> simple single-book case — and anything updated must be reflected in that
> interface.

### Q32. Confirm Q24 as scoped above?
One fixture, one counter, regenerated goldens, idempotency test split in three.

> **A:** Confirmed.

---

# Round 6

## The device screen — and the verb it needs that doesn't exist yet

This is the biggest finding in the document so far, so it goes first.

`koreader::import(storage, path, dry_run)` walks a whole tree via
`find_sidecars`, and `dry_run` already exists — so **a device scan is largely
already implemented**. But look at what happens to a book on the device that
isn't in your library (`koreader.rs:392`):

```rust
let Some((book, matched_by)) = match_book(storage, &sidecar_path, &sc).await? else {
    report.unmatched.push(UnmatchedSidecar { path: sidecar_path, title: sc.title });
    continue;
};
```

It is **reported and dropped**. Import only ever *attaches highlights to books
already in the library*. So the primary verb of the screen you just described —
"pull this specific book in from the reader" — does not exist. Today the only
way to get that book in is to go and add it by title or ISBN first, then import.

That's the feature. And the sidecar has what's needed to build it: the `stats`
subtable carries title, authors, page count, series, language and the partial
md5. So **pull-from-device** is:

1. Seed a book from the sidecar's own metadata.
2. Enrich it through the existing provider fan-out on title + author (which is
   already federated, already degrades on failure).
3. Import its highlights against the newly created book.

Step 2 is where the existing `search.rs` ranking earns its keep, and step 1
means the book exists even if you're offline or both providers miss — which
matters on a device screen, where "it's right there on my Kobo" makes a failure
feel absurd.

### What the screen shows

Per book on the device, one of:

| State | Meaning |
|---|---|
| **New** | On the device, not in the library. The pull-in case above |
| **Unchanged** | Imported, sidecar identical since last import |
| **Updated** | Sidecar has changed — *N* new highlights, *M* edited |
| **Unreadable** | Sidecar failed to read or parse (already a `Diagnostic`) |

"Updated" is the one you specified, and note the dependency: **counting edited
highlights is only possible after the Q24 work.** Until `updated` exists as a
counter, a sidecar with a changed note is indistinguishable from an unchanged
one. Item 2 isn't just correctness housekeeping — it's what makes this screen
able to tell the truth.

### One cost to design around

`dry_run` is not free: it reads and evaluates every sidecar's Lua in mlua, then
runs book matching. On a device with several hundred books that's several
hundred VM evaluations *every time the screen opens*.

Cheap fix: record each sidecar's `mtime` + size at import, and skip re-parsing
any sidecar where both match. Unchanged books then cost a `stat` call. Worth
building in from the start rather than discovering it on a full Kobo.

### Build shape

Your ordering — simple single-book case first — maps cleanly onto what exists:

1. **`import_book_from_sidecar`** in the engine: the seed-enrich-import path
   above, for one `.sdr`. Pure engine, offline-testable, and it turns
   `unmatched` from a dead end into an action.
2. **The device screen** listing state per book, single pull.
3. **Multi-select and "sync everything"** — a loop over (1) with one report.

The screen should read as the device's shelf, not a file picker. This is the
first place a stranger's data shows up in your library, and it's a *place*.

## Collections: deferred, but the data is preserved

Agreed, and the reason is right — three systems each minting collections is a
merge problem with no good default. But the Goodreads import shouldn't be lossy
just because the feature is deferred. Two things make that easy:

**1. The two Goodreads columns are not the same kind of thing.**

- **`Exclusive Shelf`** (`read` / `currently-reading` / `to-read`) is not a
  collection at all — it's **reading status**, which the `readings` table
  already models. `read` → a finished reading, `currently-reading` → an open
  one, `to-read` → a book with no reading yet. That maps onto ours cleanly and
  is genuinely ours to own.
- **`Bookshelves`** is free tagging. *That's* the part with the merge problem.

So most of the merge worry evaporates: only free tags are contested.

**2. Preserve, don't interpret.**

Store imported shelf strings as **inert provenance** — raw values, tagged with
their source (`goodreads` / `koreader`), no UI, no merging, no semantics. When
collections are eventually designed, the historical data is already there and
the merge rules can be chosen against real data instead of guessed at. This is
exactly the corrected axiom applied: a Goodreads shelf is Goodreads-owned, so we
record it as theirs rather than absorbing it into a namespace we haven't built.

Cost is one small table and nothing else. Dropping the columns instead means a
painful re-import later.

**Knock-on:** with collections deferred, unattributed highlights don't need a
staging bucket after all — they simply carry `reading_id = NULL` and are reached
from their book. No new noun.

## Revised build order

1. **KOReader format work** — source + real sidecars; `summary`,
   `percent_finished`, `stats.md5`; settle the `datetime`-on-edit question.
2. **The highlight ownership seam** *(Q24, confirmed)* — `ko_note`/`annotation`
   split, targeted device-field update, `updated` counter, `last_seen_ko_note`,
   the `Pachinko-NoteEdited` fixture, three-way idempotency test.
3. **`import_book_from_sidecar`** — seed from sidecar, enrich via providers,
   import highlights. Turns `unmatched` into an action.
4. **`readings` table + progress migration.**
5. **`partialMD5`.**
6. **Device screen** — per-book state incl. Updated, single pull; mtime+size
   pre-filter. Then multi-select and sync-all.
7. **Reflection + Review**, anchored to a reading, openable mid-book.
8. **Currently-reading home screen**, action = open the reflection.
9. **Backlinks pane.**
10. **Goodreads CSV in/out** — `Exclusive Shelf` → readings, `Bookshelves` →
    inert provenance.
11. **Wired device watcher** (macOS + Linux mount paths) feeding the screen.
12. **`book_files` + owned files + three-level dedup.**
13. **Calibre (i) then (ii).**
14. **API crate + `readingbuddyd`.**
15. **KOReader plugin + wireless push.**
16. **Collections (designed with real imported data in hand), author/corpus
    view, excerpt view, graph, orphan queue, shelf, Tauri GUI, Mac widget/iOS
    companions, two-way sync, publishing.**

## Round-6 questions

### Q33. Does pull-from-device wait for the network?
Provider enrichment can be slow or fail. Pull immediately with sidecar metadata
and enrich in the background (book appears instantly, details fill in), or block
on enrichment so the book is never half-formed?

> **A:**

### Q34. What happens to an ambiguous match?
`match_book` uses jaro-winkler ≥ 0.85. Below that a device book is "New" and
pulling it in creates a duplicate of a book you already have under a different
title. Does the screen offer "link to existing book" as an explicit action?

> **A:**

### Q35. Should rounds 1–6 be distilled now?
This document is a conversation record and is getting long. Worth extracting two
short docs: a **decisions list** (what's settled, no reasoning) and an
**engine spec for items 1–3**, leaving this as the archive?

> **A:**

---

# Round 7 — and the distillation

## Q33: enrichment dropped, and that simplifies item 3 a lot

The enrichment material was the existing `providers/` fan-out — OpenLibrary +
Google Books, searched on the sidecar's title and author. Deferring it to v2 is
the better call: **item 3 becomes entirely offline**, which means it's fully
testable under the engine's "no network in tests, ever" rule with no mock
provider needed at all.

Cost: a book pulled from the device has title, authors, page count and language
from the sidecar's `stats` table, and no ISBN, cover or description. Fine — the
existing search+add flow can fill those in later.

**Except it can't, and this is the thing that makes Q34 non-optional.**

`crates/engine/src/storage/books.rs:90` branches the upsert three ways:

```rust
let sql = if book.isbn_10.is_some() {
    format!("{insert} ON CONFLICT(isbn_10) DO UPDATE SET {set_clause} RETURNING id")
} else if book.isbn_13.is_some() {
    format!("{insert} ON CONFLICT(isbn_13) DO UPDATE SET {set_clause} RETURNING id")
} else {
    format!("{insert} RETURNING id")          // ← unconditional insert
};
```

A sidecar-seeded book has **neither ISBN**, so it takes the third branch. Later
searching for that same book and adding it hits the third branch again — plain
insert, new row. You end up with two copies: one holding your highlights, one
holding the cover and ISBN, and nothing joins them.

So deferring enrichment doesn't remove the need for a merge path, it *creates*
it. Q33 and Q34 turn out to need the same feature.

## Q34: link-to-existing, and make the link durable

Agreed — and the answer should be stronger than a one-off UI action, because
guessing again next time is the actual problem.

**Three layers:**

1. **A device→book mapping table**, keyed on the sidecar's `stats.md5`. Once a
   sidecar is linked to a book — automatically or by hand — that decision is
   recorded and never re-guessed. This is the durable fix and it's why
   `partialMD5` should land before the device screen.
2. **A candidate band.** `match_book` auto-links at jaro-winkler ≥ 0.85. Below
   that everything is silently "New". The screen should surface a middle band
   (roughly 0.60–0.85) as *possible matches* with a **Link** action, rather than
   letting a variant title quietly become a duplicate.
3. **Manual link and merge.** Search the library and link; and merge two books
   that are already duplicates, since the ISBN-less insert path above means
   duplicates will exist regardless.

Matching order becomes: **md5 mapping → title ≥ 0.85 → candidate band → New.**

## Distillation

Rounds 1–7 are now the archive. Three documents extracted:

- **`docs/decisions.md`** — everything settled, no reasoning.
- **`docs/spec-engine-01-03.md`** — implementation spec for build items 1–3.
- **`docs/prompts/`** — three ready-to-paste thread prompts, one per item.

This file stays as the record of *why*, which the decisions list deliberately
omits.
