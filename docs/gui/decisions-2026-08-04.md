---
name: gui-phase-decisions
description: Decisions settled 2026-08-04 for the readingbuddy GUI phase — transport, reward model, TUI/GUI independence, what was deferred. Read before proposing GUI work.
type: project
---

Settled 2026-08-04 while building `docs/gui-vision.md` and `docs/spec-gui-17-28.md`.
Those two files are the full argument; this is the index so a later session does
not re-litigate them.

**Transport: the GUI links `readingbuddy-api` in-process, behind a swappable
client trait. Not the daemon.**
Why: the daemon does not solve the two-writer problem while the TUI keeps its
direct engine link; covers and paths cross as filesystem strings so it buys no
remoteness; it has no push channel; and iOS forces the in-process path to exist
regardless. It arrives with item 15's plugin listener.
How to apply: every GUI call goes through the API vocabulary even in-process, so
an API gap is a compile error rather than a temptation to reach past it.

**Reward model: tracking is total, presentation is retrospective.**
The rule: *the app tells you what you did, it never tells you what you have
left.* No goals — decided against, not deferred. No live streak counters. No
number on the home surface. Past-tense statements of fact are fine. Three
registers: finishing, thinking, returning. The chain is **moment → card →
shelf**, and the card is per *reading*, not per book, so a reread mints a second.
How to apply: hold any proposed feature against the one-sentence rule.

**TUI and GUI are peers developed independently.**
Working on one must not necessitate working on the other. This is a constraint
on the *engine* — shared logic moves below both (spec item 17). The TUI is not
required to migrate to the engine's versions on any schedule.

**A settled decision was overturned: "Shelf view" leaves *Out of scope for now***
(`docs/decisions.md:230`). The original ruling was against a shelf that grouped
by collection; collections are still deferred and this shelf groups by nothing.
"Author/corpus view" partially moves with it. "Graph view" and "Orphan queue"
stay out. **`docs/decisions.md:230` still needs editing** — as of this writing
the reversal lives only in `gui-vision.md`.

**New scope: a local-reading source.** Attach a PDF, type your own progress,
take notes against it. It is the first *reading state* readingbuddy originates
(it already originates the vault). No embedded PDF viewer — explicitly out of
scope. The one real new engine job is PDF page-count/title extraction.

**Deferred with reasons:** KOReader `statistics.sqlite3` lands with item 15's
plugin work, not the GUI phase — but `reading_events` (spec item 21) is built now
as a source-agnostic log so it arrives as one more filler and changes nothing
downstream. Also deferred: note tags, graph view, collections, new importers,
better export (named as the weakest part of the system and the wave after this).
