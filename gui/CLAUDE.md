# gui/ — Tauri + Svelte 5

**Nothing is here yet.** The scaffold is spec item 25
(`docs/gui/spec-gui-17-28.md`). This file exists first on purpose: it is what the
thread that scaffolds it should read before choosing anything.

Read first: [`../docs/gui/gui-vision.md`](../docs/gui/gui-vision.md) (what the
product is), [`../docs/gui/spec-gui-17-28.md`](../docs/gui/spec-gui-17-28.md)
(the items), [`../docs/gui/testing.md`](../docs/gui/testing.md) (the four
layers), [`../CLAUDE.md`](../CLAUDE.md) (the repo axiom).

## Layout, decided

```
gui/                     the Svelte frontend
gui/src-tauri/           the Tauri Rust crate — a workspace member,
                         package name `readingbuddy-gui`
```

The package name is not cosmetic: `.claude/hooks/post-edit-check.sh` runs
`cargo check -p readingbuddy-gui` after a Rust edit here, and
`make web-check` keys off `gui/package.json`. Rename either and both go quiet
without failing, which is the worst way for a check to stop working.

**`pnpm`, never npm or yarn.** A second lockfile is a silent divergence.

## The three rules that decide everything else

**1. Every call goes through the API vocabulary.** The Tauri backend links
`readingbuddy-api` **in-process**, behind a swappable client trait — not the
daemon, and not the engine directly. A gap in the API surface must be a compile
error, not a temptation to reach past it. `docs/decisions.md` item 14 closed that
seam and CI's plain `cargo check --workspace` exists to keep it closed.

Before building any feature, run the **`api-surface-auditor`** agent. A missing
request is an engine item; it is never a frontend workaround.

**2. No decisions live in Svelte.** A component is presentation and event
dispatch. Sorting, progress arithmetic, date formatting, author-name parsing,
row-state derivation, and which rows a bulk action may sweep are **derived facts
and belong in the engine** — spec item 17. The TUI needs the same answers, and
the whole point of putting them below both frontends is that work on one never
obliges work on the other.

The rule the engine already follows and that is easy to over-apply: the engine
does no *terminal I/O*, and by extension no *phrasing*. Pluralisation, wording
and layout are yours. The **values** being phrased are not.

**3. Types are generated, never hand-written.** 52 DTOs and 77 request variants
come from Rust via `ts-rs`. A hand-written interface drifts, and the drift is
invisible until a blank panel in a webview. If a field is missing, that is a DTO
change and a regeneration.

## Svelte 5, and the one failure mode to expect

Runes only. **Svelte 4 still compiles**, which is why this is a rule and not a
preference — and why it is the most likely defect in agent-written code here,
since the training mass is Svelte 4.

| never | always |
|---|---|
| `export let foo` | `let { foo } = $props()` |
| `$: x = ...` | `const x = $derived(...)` |
| `$: { ... }` | `$effect(() => { ... })` |
| `writable()` + `$store` for component state | `$state()` |
| `createEventDispatcher` | callback props |
| `<slot />` | `{@render children()}` |

Make eslint fail on these at scaffold time rather than trusting it:

```js
// eslint.config.js
rules: {
  'svelte/no-export-load-in-svelte-module-in-kit-pages': 'error',
  'svelte/valid-compile': 'error',
  'no-restricted-syntax': ['error',
    { selector: "ExportNamedDeclaration > VariableDeclaration[kind='let']",
      message: 'Svelte 4 props. Use $props().' },
    { selector: "LabeledStatement[label.name='$']",
      message: 'Svelte 4 reactive statement. Use $derived or $effect.' },
    { selector: "CallExpression[callee.name='createEventDispatcher']",
      message: 'Svelte 4 events. Use callback props.' },
  ],
}
```

An absent rule reads exactly like a passing one. `web-checker` is told to report
that as a finding.

## The axiom, in markup

`docs/decisions.md` forbids task-completion framing by name. The GUI vision
sharpens it to one testable sentence:

> **The app tells you what you did. It never tells you what you have left.**

Binding, and the `screenshot-reviewer` agent checks all of it:

- **No number on a home surface.** Ever. Counts live on a page the user chose to
  open. The TUI asserts this against its own drawn buffer
  (`the_home_screen_greets_you_with_no_numbers`); until the GUI has an
  equivalent, the agent is the check.
- **No goals, no streak counters, no badges, no inbox.** There are no goals in
  this product — decided against, not deferred.
- **Nothing is a dead end.** Every screen shows its next move.
- **Idle is not blank.** An empty state names the moves that fill it, and never
  apologises.
- **A failure redirects.** Match the repo's refusal-with-a-next-move shape: `ko
  pull` names `--new`; `calibre status` reports absence and prescribes nothing.
- **Abandoning a book is not failure** and is never styled as one.

## The shelf

Home surface, and the product's signature. **Render it in WebGL** — the Rust ray
tracer does not cross. What crosses is `Model`'s derivation of an edition's shape
(four lines of arithmetic), which spec item 19 moves into the engine so a WebGL
shelf and a Unicode-glyph book agree about how fat *Infinite Jest* is.

Keep it as **one self-contained island with a narrow interface**: a list of
books in, an event out. Not a per-frame boundary.

## Working here

| | |
|---|---|
| checks | `make web-check`, or the **`web-checker`** agent |
| how it looks | `make shots`, then the **`screenshot-reviewer`** agent — render it, do not describe what you intended |
| can the API serve this | the **`api-surface-auditor`** agent, *before* building |
| a new component | the **`gui-component`** skill |
| a new numbered item | the **`new-wave-item`** skill |
| E2E | `make e2e` — pre-PR only, never the inner loop |

`tauri-driver` does **not** run on macOS (no WKWebView driver exists). E2E is a
Linux CI job, or `tauri-plugin-wdio-webdriver` locally. See
[`../docs/gui/testing.md`](../docs/gui/testing.md).
