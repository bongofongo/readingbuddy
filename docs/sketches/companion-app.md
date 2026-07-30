---
title: Companion & Multi-Surface Sketches
status: exploratory — not decisions.md, nothing here is binding until it graduates
date: 2026-07-30
---

# Companion & Multi-Surface Sketches

Freeform workshopping space for app-shape ideas beyond the current build order —
the Tauri desktop GUI, a gamified companion, and where it shows up across
devices. Written in the same rounds format as `docs/ux-positioning.md`, but
this file is a scratchpad, not a decision record: nothing here binds until it's
promoted into `docs/decisions.md`.

## Round 1 — what you brought, and where it lands against what's already decided

### The raw ideas
- Desktop GUI in Tauri, full control, same as the TUI.
- A widget that's a *separate* surface from the main app, with presence not
  just on Mac but inside KOReader itself, plus iOS (paid, one-time purchase)
  and Android (unscoped, brainstormed only).
- A companion — tamagotchi + "little writer desktop" inspired — that you grow
  and upgrade, gamified, "just for fun."

### Against what's already on the books
- Tauri + Svelte for the desktop app is already the settled call
  (`docs/decisions.md` → Frontends). Nothing to relitigate.
- A Mac widget + iOS companion, read-only, reading a snapshot rather than
  touching the DB, was already decided — the reasoning holds up for a
  companion too (see below).
- Android was explicitly out of scope before; you've said it's not a
  commitment now either, so it stays parked — noted here so it doesn't
  quietly become one by default.
- iOS as a one-time purchase, still self-hosted / no accounts, doesn't force
  the settled architecture to change. It does mean a real App Store listing,
  and — per the existing widget design — an iCloud-Drive-synced snapshot
  rather than a live connection, since there's still no server and none is
  planned.

### The one real tension, and how your answer resolves it
`docs/decisions.md`'s axiom bans task-completion framing on the app's own
screens — no badges, no counters, nothing that greets you with a number. A
tamagotchi is normally exactly that: a meter that goes up when you behave.

Your answer splits this cleanly: the *desk* (TUI, main Tauri app) keeps the
existing rule untouched — ambient layer stays mood-only, no meters, nothing
changes there. The *gamified* layer lives specifically in the companion/widget
space, a different surface with different rules. That's a clean seam: the
axiom doesn't bend, it just doesn't apply to a screen that was never claiming
to be "a place, not a tool" in the first place.

**Working principle for `decisions.md`, once this graduates:** the companion
is exempt from the no-task-completion rule by living outside the desk, not by
an exception carved into it.

### Reading "koreader presence" as more than "also on Mac"
Initial read: "the widget, on Mac / iOS / Android," with KOReader as one more
platform in the list. Re-reading, the more interesting version is that the
companion has presence **inside KOReader itself** — on the device, via the
KOReader plugin already in the build order (`docs/decisions.md` item 15).
That's a different idea from "port the widget to four platforms": it's "the
companion follows you to the surface where the reading actually happens,"
which is the one place none of the existing frontends reach. Flagging this
reading explicitly rather than assuming it.

### Open questions for round 2
1. **What feeds growth/unlocks?** Minutes read, highlights made, notes /
   reflections written, books finished, cross-links created (novel to this
   app specifically)? A companion that rewards *synthesis* (reflections,
   links) fits the app's whole thesis better than one that rewards raw
   reading volume, which KOReader/Kindle already reward implicitly.
2. **Decay, or pure accretion?** A real tamagotchi punishes neglect — hungry,
   can die. That's in direct conflict with "a place, not a tool" and with
   never nagging. Leaning toward no decay, ever, only forward progress at
   whatever pace you show up — but worth deciding on purpose.
3. **Cosmetic-only upgrades, or functional too?** Just how it looks, or things
   it (or the widget) can *do*? Cosmetic-only keeps "just for fun" honest;
   functional unlocks start to feel like a progression system with stakes.
4. **What is the companion, visually — same machinery as the book renderer,
   or something new?** A literal creature (separate asset pipeline,
   sprite/pixel-art or a second 3D object) vs. some evolving treatment of the
   existing book object. `render3d/` already builds a lit cuboid with a real
   raster/glyph pipeline — a companion built on that is nearly free; an
   unrelated pixel pet is a new pipeline end to end.
5. **Where does state live?** New engine tables (`companion_state`,
   `unlocks`) fits "readingbuddy is the home of its data" — but the
   widget/iOS side still can't touch the DB directly per the existing rule,
   so it reads growth state the same way it reads everything else: a
   computed snapshot the main app writes.
6. **The KOReader plugin specifically.** E-ink is monochrome and slow to
   refresh, and the plugin's cardinal rule is "fails closed, never blocks or
   slows the reader UI." What would companion presence even look like there —
   a status glyph in a menu, an overlay on book-close, nothing during actual
   reading? This is the surface most worth designing carefully: it's the one
   no competitor has, and the one easiest to wreck (KOReader users are
   protective of a fast, quiet reader).
