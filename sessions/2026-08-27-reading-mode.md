---
title: Reading mode — the book, with the window to itself
date: 2026-08-27
scope: item 54, a route group, `update_progress` reaching the frontend, `docs/decisions.md` entry 54
---

# Reading mode

One session, no wave. The user asked for two things — a reading mode with a
minimal aesthetic, and the GUI recut as a left-column menu — plus a question
about whether the second could be user-configurable the way Firefox's vertical
tabs are. Reading mode was built; the sidebar was scoped, argued and deferred to
the user's call.

## Decisions locked

- **Reading mode is a *place*, not a mode.** A full-window surface that covers
  the app is what the axiom's *nothing is modal-by-default* is normally about.
  Four counts make this one legitimate and they are written into entry 54 as the
  test to apply before adding to it: a **URL** that survives reload; state that is
  the **engine's** (`currently_reading`, not a flag the route invented); **both
  exits on screen in every state**; **no count and no target** anywhere.
- **One panel at a time**, which is the opposite of the book page and not in
  tension with it. The desk's rails are permanent *so that* nothing is modal;
  this covers the book on purpose. A desk shows its instruments, a book does not.
- **`$lib/reading/mode.ts` is deliberately not merged with `$lib/book/desk.ts`.**
  The two values have the same shape (`Panel` / `Centre`) and opposite
  arguments. One shared type would make that difference invisible the day either
  surface grew a second open panel.
- **A route group, not a conditional in the shell.** `(shell)/` is the app with
  its header; `reading/` is the app without one. The conditional was rejected for
  putting knowledge of reading mode into the chrome reading mode exists to
  escape — and because it would have grown a second arm the first time any other
  surface wanted the window.
- **The door lives in the *Reading now* band, not the header.** It is only a
  place to go when something is open, and that band already renders nothing when
  nothing is. A permanent nav entry would link to an empty state most of the time.
- **A note written here is a different object from the composer's** — it carries
  `reading_id` and `page` — and has **no title field**. A note you sit down to
  write has a name you mean; a thought you had at page 214 does not, and asking
  for one before the thought is how the thought is lost.
- **Three things reading mode deliberately cannot do**: start a read, close a
  read, annotate a passage. All three have a home on the book's page and none
  belongs one keystroke from a surface you leave open while reading.
- **Sidebar: deferred, with the cost stated.** `+layout.svelte`'s own header,
  written one commit earlier, argues *a rail there means you are working, never
  you are navigating* — the book page already has two rails, so a third column in
  the shell makes the reader learn which left column is which. Offered as a
  toggle (not a Firefox-style drag-to-customize toolbar, which buys little over
  four links). Not built; awaiting the user.

## Bugs found

- **Two links named *The library* on one screen.** `Verbs` and `SwitchPanel`
  both had one, which Playwright caught as a strict-mode violation before a human
  could. Fixed by naming the destination instead — *on the shelf*, the library
  page's own word for its second band. Two links with one name to one place reads
  as two destinations.
- **The whole composition pinned to the bottom edge**, under a field of nothing.
  `margin-top: auto` on the book with every other block answering with an auto
  margin of its own. **Auto margins in a flex column do not compose**;
  `justify-content` was the property being approximated. Found only by looking at
  the PNG — every test was green.
- **The page box said one fact twice.** *The record says p. 500 of 1408 · 35%*,
  three lines under the identical string in the accent. On a surface whose claim
  is that it shows you only what you need, the redundancy *is* the defect. The
  line now renders only in the case the row above cannot: a book with no page
  recorded.
- **Stale facts in `docs/handoff-orchestrator-gui-53-plus.md`** — it claimed the
  next free item was 53 (now 55) and that the repo "has no remote it uses"
  (`origin` exists and `main` tracks it). Both corrected.

## Technical gotchas

- **`update_progress` had been on the wire since the beginning with no client
  method above it.** This is the shape the API auditor keeps finding from the
  other direction: not a missing request, but a request nothing above the seam
  could reach. Worth checking `protocol.rs` against `LibraryClient` before
  concluding a feature needs an engine item — no engine change, no migration and
  no `API_VERSION` move were needed here.
- **`update_progress` answers with the book re-read**, which is why the client
  method returns `StoredBook | null` rather than `void`. The percentage that comes
  back is the engine's integer division over the page just stored, and it differs
  from anything the frontend could compute for the two fixture books whose
  `page_count` is `0` or `NULL`. A panel that echoed its own input would pass
  every test written against an ordinary book.
- **`FakeClient` had to reproduce that arithmetic to be worth having**, and had
  to reproduce the *right* one: `Math.floor((page * 100) / of)`, not
  `Math.floor(fraction * 100)`. The two agree on almost every input, which is
  exactly what would have let the wrong one survive.
- **`page_count: 0` is absence by the time it reaches a DTO.** `of` is `null`, so
  no percentage and no track. The fake's overlay follows the same rule rather
  than a softer one.
- **A SvelteKit route group is the way out of a root layout.** There is no way to
  skip `src/routes/+layout.svelte` — `+page@.svelte` resets *to* it, not past it —
  so the root became almost nothing (the stylesheet, and children) and the header
  moved into `(shell)/`. `git mv` of five entries; **the URLs do not change**,
  because a group's directory name is not a path segment.
- **`$state(props.foo)` warns `state_referenced_locally`.** The repo's existing
  answer is the book page's: a plain function that *seeds* state, named `seed()`,
  with a comment saying seeding is the intent. Deriving would put the record back
  into the box under a half-typed number.
- **Playwright: `locator.type()` is deprecated** — `pressSequentially()`. And a
  `for (const [a, b] of [[...]])` literal infers `string[]`, not a tuple, which
  `svelte-check` fails on at the `toHaveAttribute` call site; annotate the array
  as `[string, string][]`.
- **A keystroke handler on a reading surface needs two rejections before the
  map**: modifiers belong to the platform (`Cmd-P` prints), and a keypress inside
  a field is text — or the note box cannot contain the letter *n*. **Escape is
  exempt from the second**, or a panel is a keyboard trap.

## Verification

- `make ci` exit 0: fmt, clippy `-D warnings`, plain `cargo check --workspace`,
  `ts-check`, whole-workspace tests, `web-check`, and **156** Playwright routes on
  WebKit (144 before this session; 129 before the wave).
- vitest **267** passing, of which 10 are new (`src/lib/reading/mode.test.ts`).
- Every screenshot **looked at**, not just diffed — which is how all three
  defects above were found. Both states are now covered: the resting surface is in
  `ROUTES`, and the four panels are shot at three widths each. It shipped its
  first draft with half its design rendered in no test.

## Deferred

- **The left-column menu.** Scoped and argued, not built. If taken, it wants a
  `decisions.md` entry either way, and the persistence belongs in `localStorage`
  rather than the engine — there is no preferences table and no settings request,
  and chrome orientation is presentation, not a derived fact the TUI will ever
  want. Cost to know: a layout toggle doubles the visual surface unless the route
  suite pins one orientation and covers the other with a single dedicated spec.
- **Closing a read from reading mode.** `finished` is on the client method
  because the request has it; `FakeClient` states in as many words that its arm is
  unimplemented rather than quietly pretending. The day a screen closes a read,
  that is the line that grows.
- **Dark-theme screenshots** are still absent everywhere, reading mode included —
  backlog item 1 in the orchestrator handoff, unchanged by this session.
