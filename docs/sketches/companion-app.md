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

## Round 2 — no decay, pixel art, and the Tauri GUI first

### What your answers settled
- **No decay, and now it has a precise reason rather than a vibe.** In
  operant-conditioning terms, tamagotchi decay/neglect is **negative
  punishment** — removing something good (happiness, health) because the
  wanted behavior didn't happen. That's punishment wearing a cute sprite, and
  it's out, categorically. The companion's state economy is
  **positive-reinforcement-only**: things are added or unlocked, nothing is
  ever taken away or allowed to wilt for inaction.
- **Aesthetic: retro pixel art.** This settles open question 4 from round 1 —
  the companion is **not** built on `render3d/`'s ray-traced-cuboid
  machinery. It's a separate, much simpler pipeline: sprite sheets / pixel
  art frames, not a camera and a raster. Good news for scope: this is cheap
  compared to the book renderer, and it means companion work never risks the
  book renderer's hard-won performance budget (`render3d/`'s whole design
  discipline — no images while animating, motion quantization, the kitty
  wire-format traps — is scoped to the book object and stays untouched).
- **Build order: the Tauri GUI, with its embedded companion, comes first** —
  ahead of the KOReader plugin, iOS, and everything else in round 1's
  cross-surface list. Those are still the eventual shape, just not what gets
  built first.
- **Platforms: Mac, Linux, and Windows, in that priority order.** Mac is
  primary because that's the native dev machine — its design takes
  precedent when a choice has to be made. Linux is second (a real machine
  exists to test on). Windows is the stretch goal — no local machine to
  develop against, so it's the one that will actually test whether "Tauri
  gives you all three for free" holds up in practice. This isn't a new
  architectural question — it's the same reasoning `docs/decisions.md`
  already used to pick Tauri over SwiftUI (**"Linux for free"**, which
  SwiftUI never gives) — this just confirms the payoff was the point, not
  an incidental benefit.

### Open thread: reward schedule
Positive-reinforcement-only still leaves a real design choice — *how*
rewards land:
- **Fixed/deterministic** — do X, get Y, every time. Predictable, feels
  earned, matches "just for fun" without pressure.
- **Variable/intermittent** — unpredictable timing or magnitude. The
  strongest schedule for sustained engagement in the literature, and also
  the mechanism behind slot machines and loot boxes — worth naming plainly
  since this app's ethos elsewhere (no streaks, no nagging, self-hosted, no
  dark patterns) is pointedly anti-manipulation. A mostly-fixed economy with
  occasional ungated surprises (a bonus after a milestone, never behind
  repeated grinding) gets the delight without the compulsion loop.

> **A:**

### Open questions for round 3
7. Does the companion coexist with the 3D book in the Tauri GUI, or replace
   it as the centerpiece on some screens? (E.g.: book stays the library
   centrepiece; the pixel-art companion is a separate persistent
   character/corner presence — a "little writer desktop" pet sitting beside
   the desk rather than swapping out the desk's own furniture.)
8. Pixel art at what fidelity/scale — Game Boy-era 1-bit-feeling sprites (which
   would also travel cleanly to the KOReader e-ink surface later), or fuller
   16/32-bit color pixel art (matches "little writer desktop" more, but is a
   worse fit for e-ink if that surface still matters)?
9. First concrete unlock content: what are the *first few* things worth
   earning, concretely — cosmetic accessories, poses, room/background
   decorations, companion "forms"? Naming 3-4 real examples now makes the
   schema (round-4-style: what table holds an "unlock") much easier to sketch
   honestly than designing the abstraction first.
