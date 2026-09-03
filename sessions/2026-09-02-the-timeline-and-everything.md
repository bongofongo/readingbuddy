# 2026-09-02 — the timeline, and everything

Worktree session (`.claude/worktrees/gui-minimal`, branch
`worktree-gui-minimal-aesthetic`), based on `e4419e0`. **Frontend and docs
only** — no Rust, no migration, no `API_VERSION` move, no new request in
`crates/api`.

Started as a visual pass over the six surfaces the previous session could not
photograph; became a rework of `/life`. Settled account: `docs/decisions.md`
entry **58**.

## Decisions locked

- **`/life` is two tabs.** *Timeline* (the figures, then months as covers) is
  what the page opens on; *Everything* is the full disclosure. `views.ts` is the
  seam, shaped like `desk.ts` — a closed union plus an ordered registry, so a
  third view is one member and one `{#if}` arm. Selection is component `$state`,
  not the URL: the **period** is this route's subject and the view is not, which
  is the call the book page already makes.
- **The order of the tabs is the permission.** Entry 58 lifts two bans, and what
  makes that safe is that the reader *goes* to the second tab rather than being
  met with it. Draw any of *Everything* on the timeline and the argument is
  gone. This is the load-bearing sentence of the whole item.
- **Ranking is lifted** (authors, subjects, longest books, by size). The prior
  rule lived only in `facets.ts`, which had named its own reversal as "a product
  decision and not a sort order".
- **Self-comparison is lifted** — busiest month, trend, longest run — which
  undoes settled text in entries 23 and 28. It **keeps entry 23's condition**: a
  run is recognised only once it is over. `longestRunOf` refuses any run
  touching today.
- **The word is *busiest*, never *best*.** The first says what happened; the
  second grades it.
- **Ties break alphabetically.** Not cosmetic — `Map` iteration is
  insertion-ordered, so an untied ranking reshuffles when an unrelated row is
  edited. Asserted.
- **The months lost their sentence and gained their covers.** *Finished Hollow
  Weather, Distant Bell…* went; jackets 40px → 56px. A title is a string you
  read, a jacket is a thing you recognise. Nothing became unreachable — each
  jacket links to its book and carries the title as its accessible name, so a
  screen reader gets a list of titles rather than a `joinList` sentence.
- **The entrance dropped *Latest passage* / *Latest note*** for two icon doors
  (reading mode, the book's page), on the user's ask. `.act` and not `.door` —
  two accent doors per preview times however many books are open would spend the
  accent budget on the calmest surface in the app.
- **Fourteen charts on *Everything*, to be culled.** Built breadth-first on the
  explicit understanding that the user selects what survives.

## Bugs found

- **`/life` repeated the device-absence line on every month** — five on screen,
  all ~14 in the span. Entry 57's `No passage from this read` × 24 in another
  costume, and a *third* statement after the summary's two `not measured` and
  the `rb ko stats` paragraph. Now gated on a `someMeasured` derived over the
  whole span: the line only distinguishes something when the span is **mixed**.
- **`/devices` contradicted itself.** *"Nothing is plugged in."* sat directly
  above *"Every reader readingbuddy has been introduced to is plugged in right
  now."* — `away.length === 0` is true both when every paired reader is present
  and when nothing has ever been paired. Added a `list.length === 0` branch.
- **The year rail's CSS repainted the new tabs.** Its rules were written as bare
  `nav button` when it was the page's only `<nav>`; the tab strip is the second,
  so `.choices` was being overridden by the rail's inset dialect. All year-rail
  selectors are scoped to `.years` now.
- **The trend ran backwards.** `yearsOf` returns years **newest first** with
  months ascending inside each, so a bare flatten gave 2026's months then
  2025's — an axis that runs forwards then jumps back, drawn as a line through
  time. `shownMonths` sorts explicitly.
- **A sticky rail painted over the periphery.** The panels spanned both grid
  columns, so the rail's own column ran underneath them, and a sticky item does
  not reliably stop at the end of its grid row. The periphery moved out of the
  grid entirely.
- **`longestRunOf` read `sorted[-1]`** at `i === 0` and handed `isNextDay` an
  undefined date → `RangeError: Invalid time value`. Loop starts at 1.
- **"over the 79 books of 79 that state a length"** — a caveat about nothing.
  The denominator only appears when it differs.
- **Axis ticks collided** (`Jan 25Mar 25May 25`). Every-nth thinning is not
  enough when labels are wider than their band. Two modes now: *dense* labels
  every band (≤14 short labels), *sparse* labels first/middle/last.
- **The calendar wrapped into a tall texture.** `flex-wrap` turned three years
  into a column of week-strips; a year is a horizontal thing. Scrolls sideways
  now.

## Technical gotchas

- **This machine can drive the GUI; the Linux box could not.** `run-readingbuddy`
  is written for Hyprland and its "Nothing can click the GUI" gotcha does **not**
  hold on macOS. What works: window geometry from
  `System Events → get {position, size} of window 1`, then
  `screencapture -x -o -R"x,y,w,h"` — deterministic, unlike `grim`'s coin flip.
  `System Events → click at {x, y}` really navigates. The skill needs a macOS
  path; a helper lived in the scratchpad this session.
- **Screen coordinates from a screenshot**: `screen = displayed × 0.75 + origin`
  for a 1500-wide window captured at 2× and shown at 2000px. Get it wrong and
  clicks land on the wrong control silently.
- **A stale prebuilt binary looks exactly like a bug.** `/devices` rendered a
  wall of ~100 API variant names — `unknown variant 'paired_devices'`. That was
  the 24-Aug `target/debug/readingbuddy-gui`, which predates item 55. Rebuild
  before believing a serde error on a page.
- **`CARGO_TARGET_DIR=<main checkout>/target` makes a worktree build cheap.**
  A fresh worktree has no target dir; the branch changed no Rust, so pointing at
  the warm one is a cache hit instead of a from-scratch build of vendored Lua.
- **`make` must run from the worktree root.** From `gui/` it fails with
  `No rule to make target 'web-check'` and **exit 2** — which reads exactly like
  a check failure. Cost a wrong conclusion once.
- **eslint catches unused CSS that svelte-check does not.** `.measured strong`
  and ten dead selectors from replaced panels were invisible to
  `svelte-check --threshold error` and hard errors under `svelte/valid-compile`.
- **`axiom.test.ts` enforces exactly one word: `/\byet\b/i`**, over `.svelte`
  markup only, with `<script>`, `<style>` and comments stripped first. *streak*,
  *goal*, *target*, *unread*, *remaining* lost their test when the Playwright
  suite went and are **review rules only** — grep new markup by hand. It caught
  a real violation this session (a `yet` in new `/devices` copy, in a file that
  carries a comment naming the rule).
- **`:focus-visible` fires on AppleScript synthetic clicks** in WebKit, so a
  selector member renders with a focus ring after a driven click. An artifact of
  driving, not a defect — do not "fix" it.
- **`ts-rs` types are `T | undefined` on index access.** `by[i] += x` on a
  `new Array(12).fill(0)` is a tsc error; `by[i] = by[i]! + x`.
- **`href` cannot be both a prop name and an attribute** in Svelte —
  `{href}={to}` is a parse error. The prop became `link`.
- **`books` cannot be summed across months** (item 42) and it bit the
  seasonality chart: two Januaries can hold the same book. Built on
  `activity_days`, which *is* summable because a day belongs to one month.
  Asserted, with the wrong answer named in the test.
- **`BookSummaries` returns counts, not titles** (`{book_id, highlights, notes,
  files}`), so `/notes` showing which book a row belongs to is still one
  `getBook` per row. Real engine gap.
- **`ActivityByDay` already existed in `crates/api`** and was simply absent from
  `LibraryClient` — a frontend-only addition, not an engine item. Worth checking
  the protocol before filing a gap.
- **`SearchMarks` still has no `kind` filter** (only `source`). The GUI comments
  claiming so are accurate.
- **The api-surface-auditor agent stalled** (600s, no progress) on a broad
  "inventory the whole surface" prompt. Reading `protocol.rs` and `dto.rs`
  directly took minutes. Prefer narrow questions to that agent.

## Verification

- `make web-check` — exit 0: svelte-check 452 files / 0 errors / **0 warnings**,
  tsc, eslint, **361 vitest**, build.
- New unit tests: `facets.test.ts` (29) and `graphs.test.ts` (18). The ones that
  matter are negative — a run touching today is refused, an unrated reading gets
  no bar, a zero `page_count` is absent not zero, an unmeasured month is dropped
  not flattened, the calendar never invents a day.
- Looked at, against `dev-data/`: every surface the previous session could not —
  `/notes`, `/cards/history`, `/reading`, `/devices`, `/life`, the editor's
  `Connections` — plus both new `/life` tabs and all fourteen charts.
- The banned vocabulary was grepped by hand out of rendered markup (script,
  style and comments stripped) for *goal / target / pace / behind / remaining /
  streak / in a row / best* — clean.

## Deferred

- **The user is culling the fourteen charts.** `docs/decisions.md` entry 58 and
  `gui/CLAUDE.md` still describe the *panels*, not the charts — both need a pass
  once the set is settled.
- **`/notes` is untouched**: centring and denser rows were asked for and are
  blocked on the batch book-title gap above.
- **`facets.ts` / `graphs.ts` derive above the seam and should not.** Every
  figure is bounded by `READINGS_PAGE` (500) and cannot tell when it was
  truncated; all are over closed readings only. An engine aggregate would have
  neither limit. Recorded in both module headers, like `latest.ts`' N+1.
- **The calendar is the panel to cut first.** Nothing computes a run, labels one
  or draws today, but the contribution-graph *form* carries a suggestion the
  numbers do not. Its own header says so.
- **Three design calls raised and not taken**: the editor's eleven boxed rating
  buttons (the most shallow interface left in the app), `/cards`' fixed 3-column
  grid stretching six cards to ~570px of mostly air, and the book page's measure
  — content capped at 44rem while `Write` / `Cards →` justify across 1400px.
- **The entrance's N+1 now buys ordering alone.** `Preview` stopped drawing the
  mark; `latestMark` is still fetched because it is `ordered`'s sort key. That
  makes the missing request worth *more*, not less.
- A macOS path for `run-readingbuddy`.
