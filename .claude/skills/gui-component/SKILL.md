---
name: gui-component
description: Write a new Svelte component or route for readingbuddy's GUI to this project's conventions. Use when adding any component, screen or route under gui/, or when a component needs data from the engine. Keeps twelve sessions producing one dialect instead of twelve.
---

# gui-component

The GUI is **Tauri + Svelte 5**, linking `readingbuddy-api` in-process behind a
client trait. Read `docs/gui/gui-vision.md` for what the product is and
`gui/CLAUDE.md` for the frontend rules; this skill is the procedure.

## 1. Decide where the logic goes — before writing the component

This is the step that is skipped and the one that matters.

**A Svelte component holds presentation and event dispatch. Nothing else.** If
you are about to write sorting, progress arithmetic, date formatting, name
parsing, row-state derivation, or a rule about which rows a bulk action may
touch — **stop**. That is a derived fact and it belongs in the engine, because
the TUI needs the same answer and the two frontends must never have to be edited
together. This is spec item 17, and there are already three divergent spellings
of "how far into this book am I" in this repo from the last time it was skipped.

Launch the `api-surface-auditor` agent with the feature description. It answers
whether the API can serve it and names the gap if not. **A gap is an engine
item, never a frontend workaround** — do not read the database, do not shell
out, do not cache-and-recompute.

## 2. Reach data through the client, never `invoke`

Exactly one module in `gui/` knows the word `invoke`. Everything else takes a
typed client. That is what makes a component testable in `jsdom` with no Tauri
loaded, and it is why a renamed command is a `tsc` error rather than a blank
panel.

Types are **generated** from the Rust DTOs — never hand-written, never
hand-edited. If a field you need is absent, that is a DTO change plus a
regeneration, not a local interface.

## 3. Svelte 5 runes only

The legacy dialect still compiles, which is exactly why it must be a rule rather
than a preference:

| never | always |
|---|---|
| `export let foo` | `let { foo } = $props()` |
| `$: x = ...` | `const x = $derived(...)` |
| `$: { ... }` | `$effect(() => { ... })` |
| `writable()` + `$store` for component state | `$state()` |
| `createEventDispatcher` | callback props |
| `<slot />` | `{@render children()}` |

## 4. Honour the axiom in the markup

`docs/decisions.md` forbids task-completion framing by name, and the GUI vision
gives the one-line test: **the app tells you what you did, it never tells you
what you have left.**

- **No count on a home surface.** Ever. Counts live on a page the user chose to
  open.
- **No dead ends.** Every screen shows its next move.
- **An empty state names the moves that fill it** — it is never blank and never
  an apology.
- **A failure redirects.** Match this codebase's refusal-with-a-next-move shape:
  say what was refused and name the thing that would work.
- **Abandoning a book is not failure** and must not be styled as one.

## 5. Test it at the right layer

- **Component logic** → Vitest + `@testing-library/svelte`, with a fake client
  injected. Milliseconds, no Tauri. This is where you write tests.
- **How it looks** → a Playwright screenshot, reviewed by the
  `screenshot-reviewer` agent. Do not describe what you intended; render it and
  look.
- **Never** reach for `make e2e` here. That is a seam check, not a feature test.

Render at a narrow, a normal and a wide width. The terminal sibling's
`every_screen_draws_at_every_size` exists because the layout bug is always at an
extreme.

## 6. Finish

Run the `web-checker` agent. Green means svelte-check, tsc, eslint, vitest and
the production build — not just the one you were watching.

## Push back rather than comply

If the component being asked for wants a number on the home screen, a goal, a
streak counter, or a badge counting undone work, say so and stop. Four of five
threads in the last engine wave pushed back and each time they were right.
