---
title: The GUI wave opens — item 25's scaffold, the type seam, and a library worth rendering
date: 2026-08-05
follows: sessions/2026-08-05-surfacing-the-engine-wave.md
---

# Session log

The handoff said path A (the GUI wave) was all that remained. The user asked for
a functioning GUI and a working Claude workflow by the end of the thread, and
chose to **invert the spec's order**: item 25's scaffold first, as a thin vertical
slice, rather than six engine items before a pixel.

## Decisions locked

- **Item 25 before item 17, deliberately against the written order.** Three
  reasons, and the third is the one that decided it. The workflow tooling the user
  wanted (`ts-rs`, `make shots`, `screenshot-reviewer`) *cannot be validated
  without a scaffold*. Item 25's own spec says to expect API gaps, and finding
  them costs one screen rather than six items. And the spec's closing rule says
  item 17 is "a design made from an audit rather than from writing the code" — so
  item 17 designed against a frontend that exists is a better item 17.
  `docs/prompts/17-derived-facts.md` is the payoff: every derivation the slice was
  refused, recorded at the point where the temptation was.
- **The slice is a plain list and a detail page, explicitly not the shelf.** Item
  26 is the WebGL shelf and a half-built one in the wrong dialect would be worse
  than none.
- **`ts-rs`, one committed `bindings.ts`, and `make ts-check` as the gate.** 77
  types in one file rather than 77 files, because `export_to` collapses them and
  ts-rs then emits no imports. Output committed so a thread that cannot run cargo
  reads current types; CI regenerates into a temp dir and diffs.
- **`bigint` is widened to `number` in the generator, with a guard.** ts-rs maps
  `i64` to `bigint`, which is correct for a transport that can carry one and wrong
  for this one: Tauri IPC is JSON, so an `i64` arrives from `JSON.parse` as a
  `number` — and **`JSON.stringify(3n)` throws**, making a `bigint` id on an
  outgoing request a runtime failure that tsc calls correct. Every `i64` here is a
  row id, a page count, a unix second or a minute total, all far under 2^53. The
  guard fails if any `bigint` survives, and the type count is derived from the
  crate's own attribute count rather than hardcoded.
- **`make dev-db` emits data, never a schema** — a deviation from the plan as
  stated, and the better one. The user picked "corpus writes the DB via rusqlite
  after applying the migrations as data files"; that needs corpus to reimplement
  sqlx's `_sqlx_migrations` ledger, and it needs `rusqlite`, whose `bundled`
  feature would unify onto the engine's `libsqlite3-sys` and change how the
  *shipped* engine links SQLite. So the real `rb` binary creates and migrates the
  database and `seed.sql` fills it: corpus keeps the no-`readingbuddy` rule
  literally, gains no dependency, and never owns a second copy of the schema.
  Precedent was already in the crate — `gen-kostats` emits SQL text for the same
  class of reason.
- **G1 landed: `Book::reading_status`, beside `finished` and not replacing it.**
  The `api-surface-auditor` found it before a line of Svelte was written.
  `abandon_reading` deliberately leaves the reading open, so *reading*,
  *abandoned* and *never opened* are all `finished: false` with a `current_page`.
  A `String`, not an enum, because an importer can write a status this build does
  not know and a parse that refused one would turn a foreign device's vocabulary
  into an error on the read path.
- **G3 (the cover collision) assigned to item 20, not fixed here.** It is a live
  bug on `main` — see below — but item 20 rewrites cover storage and
  content-addressing the filename is that item's natural shape. Recorded loudly in
  `docs/prompts/17-derived-facts.md` rather than half-fixed in a file item 20 is
  about to rewrite.
- **`Api::open(data_dir)` is new, and it is the point of the seam.** `Api::new`
  needs an `Arc<Engine>`, so every caller depends on `readingbuddy` — harmless for
  the daemon, which names no method, and fatal for a semantic client whose whole
  discipline is that a missing request must be a compile error rather than one
  `use` away. `gui/src-tauri/Cargo.toml` lists the API crate and not the engine.
- **One Tauri command, taking `Call` and returning `Reply`.** A command per facade
  method would be a third hand-written copy of the surface after the DTOs and the
  TypeScript. `Api::call` rather than `Api::dispatch` — the auditor's correction:
  `call` never returns `Err` and stamps `api_version`, so the in-process arm and a
  future socket arm produce the same value instead of one wrapping and one not.
- **The GUI is a workspace member so `cargo check --workspace` covers it.** That
  is the build where the engine's `internals` feature is off, which makes reaching
  past the API a CI failure rather than a decision somebody makes at 11pm.
- **`LibraryClient` is one interface with two impls, injected** — `testing.md` had
  already decided this and the first cut of `client.ts` (free functions) did not
  allow it. `mockIPC` matches on a command-name *string*, so a renamed command
  breaks the app while every test mocking it keeps passing.
- **`TauriClient` never falls back to the fake.** A GUI that quietly rendered
  fixture data when the engine failed to open would look like a working app
  showing somebody else's library.
- **The Svelte 4 ban is tested, not configured.** `eslint.config.js` exports
  `svelte4Bans` and `dialect.test.ts` asserts all five selectors fire plus six
  shapes they must not. `gui/CLAUDE.md` already warned that an absent rule reads
  exactly like a passing one; this was the one place that failure was invisible.
- **Two axiom rules moved from review to assertion.** `the library surface greets
  you with no numbers` (the TUI's home-screen test, ported) and `an abandoned book
  is not styled as a failure`.
- **The accent is inherited, not invented.** `crates/tui/src/theme.rs`'s brass and
  its seven presets became CSS custom properties. Beyond that the visual design is
  deliberately unfinished: `claude-code-plan.md` says not to let an agent decide
  the shelf's feel.

## Bugs found

- **Google Books covers all collide, on `main`, today.**
  `images::filename_from_url` names the file after the URL's last path segment,
  and a GB thumbnail is `.../books/content?id=…` — so **every** GB-sourced cover
  writes `images_dir/content` and the last import wins. Two books render each
  other's cover. Epub extraction (`slugify(title)`) collides on two editions of
  one title. Found by the `api-surface-auditor`, invisible in a single-provider
  library, and `make dev-db` generates its own covers so nothing here catches it.
  Assigned to item 20.
- **`scripts/gen-ts.sh` deleted the committed bindings before it knew the build
  worked.** ts-rs appends, so the old file has to go — but deleting first means a
  compile error leaves no bindings at all and `make ts` destroys the committed
  copy on its way to failing. Which is exactly what happened the first time a DTO
  field was added and a test's struct literal did not compile. It now builds with
  `--no-run` first.
- **`vitest` 2.1.9 bundles vite 5's types** and the project is on vite 6, so
  `defineConfig` from `vitest/config` rejected the config with a wall of
  variance errors. Bumped to vitest 3 rather than papering over it — and the first
  attempt (`defineConfig` from `vite`) was wrong in the other direction, since
  plain vite's has no `test` key at all.
- **`_sqlx_migrations.checksum` was verified, not assumed.** It is SHA-384 over
  each migration's raw bytes and the description is the filename stem with `_` →
  space, both measured against a freshly migrated database. Worth knowing even
  though the design chose not to depend on it.

## Technical gotchas

- **`#[serde(other)]` does not survive ts-rs.** `ErrorCode::Internal` carries it,
  which is what makes an unknown code from a newer build degrade rather than fail
  to parse — so the generated TS union is exhaustive over *today's* codes while
  the wire is not. A frontend must keep a default arm tsc believes is unreachable.
  The four `failed to parse serde attribute` warnings on every `make ts` are this,
  and are deliberately **not** silenced with ts-rs's `no-serde-warnings`: they name
  the one place the two disagree.
- **`cover_path` is a whole path, not a name relative to `images_dir`.** So
  `convertFileSrc(book.cover_path)` is right and joining the two doubles the
  prefix. It is absolute only if the engine was rooted at an absolute path, which
  is why `InProcess::open` absolutizes — a webview has no working directory.
- **The asset-protocol scope is set at runtime, not in `tauri.conf.json`**, because
  the directory is not known until the engine is open. And
  `core:asset:allow-asset-protocol` is **not** a capability permission in Tauri 2 —
  the protocol is gated by `app.security.assetProtocol` alone, and naming it in
  `capabilities/default.json` fails the build with a 400-line list of what it
  expected instead.
- **Tauri forces a `Result` on any async command with a reference input.** So
  `State<'_, _>` and a bare `Reply` cannot coexist; the command takes an owned
  `AppHandle` and reaches for the state itself, which keeps the protocol's single
  error channel rather than adding a second differently-shaped one beside it.
- **`notes_fts` has no triggers** — the engine writes it from application code, so
  a seeded note without its FTS row is a note `SearchNotes` cannot find. `seed.sql`
  writes it. This is the one thing item 27's search box exists to do.
- **`reading_events` is not seeded**; `make dev-db` runs `rb activity --refill`, so
  the log comes from the engine's own fillers (835 rows). A generator writing that
  table would be asserting item 21's arithmetic rather than exercising it.
- **`gen-ts.sh` pins plain `cargo test`, not nextest.** All 77 types append to one
  file; cargo runs them as threads in one process where ts-rs is the only writer,
  while nextest runs each test in its own process. Not measured — nextest was
  absent on this machine — so it is pinned as a precaution and the derived type
  count is the backstop either way.
- **An empty inline `<span>` generates no line box**, so the whitespace-only
  `.where` span on a highlight with no chapter and no page was *not* the visible
  extra line it looked like it would be. The conditional is a cleanliness change,
  not a rendering fix; `make routes` passing on the old shots is what proved it.
- **`svelte-check` wants `line-clamp` beside `-webkit-line-clamp`**, and the
  clamp is load-bearing: it is the only thing that clips by rendered line rather
  than by character count, which is what makes it correct for the CJK title.
- **`pnpm` ignores esbuild's build scripts by default** (pnpm 10), which breaks
  vite silently-ish. `pnpm.onlyBuiltDependencies` in `package.json` rather than the
  interactive `pnpm approve-builds`.
- **`bindings.ts` is in `.prettierignore` and eslint's ignores.** Formatting it
  would make `make ts-check` fail on a clean tree, since the generator does not
  produce prettier's output.

## Verification

- **`make ci` → exit 0**, and `ci` is now wider: fmt + clippy + `cargo check
  --workspace --locked` + **`ts-check`** + whole-workspace test + **`web-check`** +
  **`routes`**.
- Engine lib **324** (was 323) — `the_projection_tells_abandoned_from_reading_from_never_opened`.
- `readingbuddy-gui` **2** — `one_call_reaches_sqlite_and_comes_back_stamped`
  (a `Call` in, a `Reply` out, over a real database the engine migrated itself:
  the seam check `testing.md` describes, and the thing E2E cannot do on macOS) and
  `the_images_dir_is_absolute_whatever_the_data_dir_was`.
- Frontend **21** vitest (11 dialect, 10 phrasing) and **30** Playwright across
  three WebKit viewports, 24 committed shots.
- `make dev-db` → 220 books, 15 abandoned / 64 finished / 18 reading, 21 notes in
  `notes_fts`, 835 `reading_events` derived by the real filler. Verified by reading
  it back through `rb list` and by SQL.
- **The app was run**, `pnpm tauri dev` against `dev-data`, and the library screen
  and a book page were read as PNGs rather than described. Two things the images
  settled that no assertion would have: the RTL title lays out right-aligned
  inside its tile without dragging the grid, and the 220-character title clips at
  two lines with the whole thing on the detail page.

## Deferred

- **Items 26, 27, 28 — the shelf, the notes, the chain.** The current library
  screen is a plain grid on purpose.
- **The two fixtures can diverge.** `corpus gen-devdb` builds the app's library
  and `gui/src/lib/api/fake.ts` the frontend's; layer 2 runs in a bare browser and
  cannot reach the first. The shapes are named after the manifest's entries so the
  drift is visible, but nothing asserts they agree. A generator emitting both is
  the fix and nobody has asked for it.
- **The fake serves no covers**, so layer 2 exercises only the *no cover* path.
  Covers are checked in the real app against `make dev-db`. Cover layout therefore
  has no headless regression test.
- **The detail screen makes four calls for one book** and there is no request that
  returns a book with its children. Fine for one book, wrong for any list — an
  item 18 line, recorded in the route file rather than worked around.
- **No push channel**, so anything background would be a poll. Nothing polls yet.
- **G2** — `ReadingDto.status`/`source` and `NoteDto.kind` cross as bare `String`s
  while `NoteKindDto` and `KoStatusDto` are exported enums the DTOs do not use.
- **G4** — `highlights.color` has existed since migration `0001` and the importer
  writes it, but it reaches no frontend. Item 27's, named now so the highlight list
  is not built twice.
- **`docs/gui/claude-code-plan.md` items 1, 5 and 8 were already done** before this
  thread (the CLAUDE.md split, the four agents, the three skills and both hooks).
  Items 2, 3, 4 and 6-for-item-17 landed here. Item 6's remaining seven prompt
  files are still deliberately unwritten — speculative until 17 lands.
