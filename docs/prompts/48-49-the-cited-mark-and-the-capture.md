# Prompt — Items 48 and 49: which passages are cited, and a card can be captured

GUI only, **one thread, both items**. They are two controls in the same band of
the same screen and share `gui/src/lib/book/` outright; `new-wave-item` step 4
forbids running them concurrently, because two agents on one screen produce two
dialects of it.

Your worktree is cut from **finished `main`**, after item 47 merged. `/cards`
exists and `gui/src/lib/card/` may have been reshaped. Do not touch either.

No engine change, no API change, no migration, and **`API_VERSION` stays 2**.
Both requests already exist. If you become convinced one does not, that is a
conversation with the orchestrator and an engine item — never a field added
above the seam.

## Before you write a line

```
git log --oneline -1                    # must be the tip of main, WITH item 47 in it
ls crates/engine/migrations/ | tail -2  # must end at 0018_reading_sort_indexes.sql
ls gui/node_modules >/dev/null && echo deps-ok
```

If the first two are wrong, `git reset --hard main`. If `deps-ok` does not print,
run `pnpm install` in `gui/` **before** anything else: without it `make web-check`
and `make routes` print `SKIPPED:` and you pass unrun.

Read `gui/CLAUDE.md` in full — especially *The book view* — then
`docs/gui/gui-vision.md`, `docs/gui/testing.md`, and `docs/decisions.md` entries
**46** (`CitationsForNotes`), **45** (`CreateFlashcard`), **27** (the book view,
and the three rules it settled) and **7** (highlight ownership, and citing).

---

## Item 48 — which passages are already cited

Small, and mostly a deletion of a prohibition.

A mark on the passages that **some note** already quotes, drawn in the book
view's passages band.

- **One call for the page of notes the route already loads.** The book page has
  `listHighlights` and `listNotes` in the same `Promise.all`; add
  `CitationsForNotes` over the note ids it just got. One call, not one per note.
- **The reply is highlight ids, not rows.** `NoteCitationsDto` is
  `{ note_id, highlight_ids }`, one entry per requested id, **in the order
  asked, empties included**. A note id that does not exist gets an empty entry —
  to this question, *no such note* and *cites nothing* are the same answer, and
  a missing row would leave you unable to zip the reply against the page you
  already hold. **If you find yourself fetching highlight text, you have taken
  the wrong call.**
- **`CitationsFor` (singular) stays and is not redundant.** It feeds the pane
  that shows the passages themselves, where the words are the point, and it is
  what the `Passages` component's existing `cited` prop is built from — that
  prop is *which passages the open note cites*, and it drives the Cite/Uncite
  toggle. The new mark is a **different** fact: *somebody's note quotes this*.
  Do not collapse the two into one visual. A reader must be able to tell
  "I am citing this into the note I have open" from "this is quoted somewhere".

### The two texts you must update

Both carry a standing instruction not to build this, and it is now satisfiable
rather than binding. **A stale prohibition is worse than none, because the next
thread obeys it.** This item is not done until both say it is built:

- `gui/CLAUDE.md`, under *The book view*: *"Marking which passages any note
  cites is one call per note in the book and is a later item — do not build the
  N+1."*
- The module doc at the head of `gui/src/lib/book/Passages.svelte`: *"What is
  deliberately not here is a mark saying which passages are cited by some other
  note: that needs one `CitationsFor` per note in the book, an N+1 with no
  request behind it, and it is recorded as a later item rather than built
  badly."*

Rewrite each to state what shipped and by which call — not to delete the
sentence. The reason the loop was refused is still the reason the one-call shape
is the right one.

---

## Item 49 — a card can be captured

Small. A control that makes a flashcard from a passage.

- **Until item 45 a card could be minted by the KOReader import and by nothing
  else** — `Storage::insert_flashcard`'s only production caller was the import's
  auto-capture of single-word highlights. This is the first way a reader can
  make one.
- `CreateFlashcard { book_id, highlight_id?, word, context? }`. **The pair is
  re-read server-side**: a `highlight_id` belonging to a different book is
  `InvalidInput`, not a card quietly filed beside somebody else's passage. Do
  not pre-validate that in TypeScript; let the refusal live where it lives.
- **The reply is a bool, and the two values are different facts.**
  `true` = created. `false` = *you already had this card* — `UNIQUE(book_id, word)`
  dedupes and the existing card is left exactly as it was. **A frontend that
  renders both as "saved" throws away the whole reason the write answers
  anything.** The confirmation must tell them apart, in the reader's words, and
  "already had" is not an error and must not be styled as one.
- `FlashcardDto` now carries `book_id` and `highlight_id`, so a card can be
  **shown beside its passage** — that is the half of item 45 that makes the
  control worth having rather than a form. `ListFlashcardsForBook` is the call.
  Whether the passages band shows an existing card's mark is yours to decide;
  say what you decided and why.
- **No task-completion framing anywhere near it.** No count of cards not yet
  made, no badge, no *yet*.

### The word, which is the part with a real question in it

A flashcard is a `word` plus optional `context`. A highlight is a passage. The
control has to get a word out of one, and there are two honest shapes:

- **The reader types it**, with the passage as `context`. Truthful — the reader
  picks which word they want — and it is one more field.
- **The selection is the word**, taken from what the reader has selected inside
  the passage, with the whole passage as `context`. Fewer keystrokes, and it is
  the gesture a pointer makes available that a terminal could not — the same
  argument that put the Cite control on the passage.

I lean toward the second **with the first as its fallback** (nothing selected →
an empty box focused). **Push back if you disagree** — six specified points were
overturned by workers last wave and every one of them was right. Say what you
chose in `docs/decisions.md`.

---

## Rules over both items

**The notes band is one pane at three depths** — the list, one note, that note's
links — each replacing the last **in place**. **No dialog anywhere on this
screen.** A page has more room for a modal than a pane does, which is why this
had to be refused here rather than assumed. Neither of your controls may open
one.

**A passage says who wrote what against it.** `ko_note` is the device's and is
rewritten on every import; `annotation` is the reader's and no import touches
it. That labelling is load-bearing and the passages band is the only place the
distinction is visible at all. Do not let two new controls crowd it out.

**No decisions in Svelte.** Sorting, progress arithmetic, date formatting,
author-name parsing and selection predicates are the engine's (item 17).
Pluralisation, wording and layout are yours, and `src/lib/phrasing.ts` is where
the frontend's half lives.

**Do not touch `gui/src/lib/card/`, `gui/src/routes/cards/` or
`gui/src/lib/life/`.** Item 47 just landed there.

**Do not hand-edit `gui/src/lib/api/bindings.ts`.** Nothing here should touch a
DTO at all; if you think you must, that is an engine item and a conversation.

## Svelte 5, runes only

`export let` / `$:` / `writable()` / `createEventDispatcher` / `<slot />` are all
banned and `eslint.config.js` fires on every one of them. The most likely defect
in agent-written code here, because the training mass is Svelte 4.

## The client, and the fake

`gui/src/lib/api/client.ts` has **no** `citationsForNotes` and no
`createFlashcard`. Add each to the `LibraryClient` interface, to `TauriClient`,
and to `FakeClient` in `gui/src/lib/api/fake.ts`.

The fake's books are the hostile set on purpose, declared once in
`crates/corpus/edge-cases.json` and asserted from both sides. **No `as` cast in
`book()`.** Your fake `citationsForNotes` must return one entry per requested id
in the order asked, empties included — a fake that drops empties makes the zip
untestable at the layer it is tested. Your fake `createFlashcard` must return
`false` on a repeat of the same `(book_id, word)`, or the confirmation's two
faces are never rendered.

## Your gate

```
make web-check     # svelte-check + tsc + eslint + vitest + build
make routes        # every route, three viewports, WebKit — fails on a diff
make shots         # then LOOK at the PNGs, and have screenshot-reviewer look
```

The book routes are already in `ROUTES`. Add what would fail: a book where some
passages are cited and some are not (both states in one screenshot), and a book
that already has a flashcard.

**Never read a piped report, only a piped exit code.** `make routes | tail -25`
reports *tail's* status.

Also run `make fmt lint build-check test ts-check`.

## Agents

- **`api-surface-auditor` first**, per item, before a line of Svelte.
- **`gui-component` skill** for any new component.
- **`web-checker`** after touching anything under `gui/`.
- **`screenshot-reviewer`** before calling the screen done, and **not optional**.
  Two new controls in a band that already carries labels, annotations and a cite
  toggle is exactly where a screen becomes unreadable while passing every
  assertion.

## When you are done

Append **two** entries to `docs/decisions.md`, one per item. **Append;
restructure nothing** — the file is in build order, not numeric order, and every
merge conflicts there. If item 47's entry is above yours, leave it alone.

Each entry records **the corrections building it forced**, not a summary of what
was built. Name the word-capture decision and the cited-mark visual decision.

Then report to the orchestrator: what you built, what you overturned, what you
left, and whether anything you did could break another thread. **If you spawned a
`web-checker` or `screenshot-reviewer` and it went quiet, say so** — a subagent
with no `SendMessage` reports to the orchestrator rather than to you, and a
worker can sit completed-but-unfinished with nothing looking wrong.
