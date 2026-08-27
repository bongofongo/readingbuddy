---
title: LLM and AI — the v2 subsystem, deferred
date: 2026-08-21
source: docs/decisions.md (the settled part); this file is the argument and the
        open questions
---

# LLM and AI

**Nothing here is scheduled.** `docs/decisions.md` records the settled half: v2,
separate, no seam stubbed. This file is the other half — why, and what is still
undecided — so the next thread starts from the four problems rather than from the
idea.

## Why a subsystem, and why not now

The question that started it was one feature: an LLM-backed recommender grounded
in the user's own reading, aiming to beat Storygraph. It widened in the same
conversation to a quiz / book-club surface and an MCP server.

That widening is the whole argument. One recommender is a feature. Three surfaces
sharing a model client, a consent posture, a storage story and a prompt-versioning
story is a subsystem — and a subsystem does not belong inside a v1 still finishing
its acquired-data layer (items 29–32) and its GUI wave.

**Deferring costs nothing, and it compounds.** The taste signal a recommender
needs is already in the database and gets richer every time v1 lands an item:
`readings`, `ratings`, `subjects_series` (0013), `reading_events` (0011),
`moments` (0017), highlights and annotations, notes/reflections/reviews,
`field_provenance` (0012). v2 gets strictly better the longer v1 runs.

## Two directions, not one feature

**Inbound — readingbuddy is the tool a model calls.** An MCP server over the
existing `crates/api` DTOs. This is transport, not engine work: `readingbuddyd`
already demonstrates the shape — one crate, `Api::call` per line, branching on no
method name — and an MCP server is a second transport with the same property.
Point a chat client at the library and both recommendations *and* book-club
discussion work with no prompt hardcoded anywhere.

Almost certainly the right first v2 item, for a reason beyond cheapness: it shows
what a good prompt over this data actually looks like *before* one is baked into
the engine.

**Outbound — the engine calls a model.** An `LlmProvider` trait beside
`providers/`, held to that module's rules exactly: failures degrade to a typed
`Diagnostic` and never abort, a mock impl in tests, no network in tests. One
OpenAI-compatible impl covers ollama, LM Studio, openrouter and vLLM; `reqwest`
is already a dependency, so no new crate and no `deny.toml` licence gate. Whether
it lives in `engine/src/llm/` or a separate crate is undecided and should be
decided by the first real consumer, not in advance.

## The four problems, all open

### 1. Hallucination is the whole game

A raw model invents titles, authors and ISBNs. Storygraph wins because its rows
are real books, not because its taste is better.

The pipeline that would beat it: the model proposes `title + author + why` → each
row resolves through the existing OpenLibrary/GoogleBooks fan-out → a resolved row
carries a real ISBN, page count and cover and can *become* a book → an unresolved
row is dropped and counted in a `Diagnostic`.

**Open:** what happens to a genuinely good obscure pick that no provider knows.
Dropping it silently is wrong; showing it unverified next to real rows makes the
whole shelf untrustworthy.

### 2. Stored output argues with `0017_moments.sql`

That migration sets a house rule and states it at length: a moment is derived on
every ask, never accumulated, **specifically** so nothing can be counted or
badged — which is the design axiom's "no task-completion framing" expressed as
schema.

Model output cannot be re-derived. It is non-deterministic and it costs a call, so
suggestions must be stored. That is the first genuine argument between this
subsystem and the codebase's own style.

**Open:** how to store them without the stored rows becoming a number that greets
the user. The constraint is not "don't store" — it is that a suggestion shelf must
be a place you go, carrying each row's reason and the books that seeded it, and
never a count of things you have not read.

### 3. Private reading leaving the machine is a consent boundary

Root `CLAUDE.md` already holds the floor: highlight text, note bodies and search
queries are the user's private reading, never above `trace!`. Shipping them to a
third-party endpoint is a decision of the same kind, and each surface needs a
different amount:

| surface | material it needs |
|---|---|
| recommendations | bibliographic + subjects/series + ratings + finish dates. No highlight text needed. |
| quiz / discussion | highlights, annotations, notes, the reflection — that *is* the material |
| MCP | whatever tools are enabled; the exposure is the tool list |

**Open, but leaning:** per-surface tiers with the tier recorded alongside each
stored result, rather than one global "AI on" switch. A single toggle is far
simpler and leaves no way to use recommendations at low exposure and no record
afterwards of what went where.

A third option exists and was not dismissed: bibliographic data to any endpoint,
private material only to a local one detected by `base_url`. Strongest guarantee;
makes the flagship discussion feature unavailable to anyone without local
inference.

### 4. "Quiz" was never pinned down

Three readings, three different builds:

- **Book-club discussion prompts.** Open questions about a finished reading —
  themes, what the author is arguing, where you disagreed — seeded by highlights
  and the reflection. Unscored. The response is a natural reflection entry, so it
  lands in an object that already exists.
- **Graded academic recall.** Closed questions, expected answers, the model marks
  the response. Needs scoring, per-question answer records and a schedule — the
  largest of the three, and the one sitting closest to the task-completion framing
  the axiom forbids.
- **Socratic conversation over one book.** Not a question set but a running
  dialogue held to the text, with the book's highlights, notes and reflection in
  context. Nearest to "book club" — and possibly needs no engine feature at all
  once MCP exists, which is itself an argument for doing MCP first.

## Naming

`flashcards.rs` is vocabulary word → Anki TSV. Items 45–48 call a cited passage a
card. **"Card" is spent.** Keep *suggestion*, *quiz* and *discussion* free.
