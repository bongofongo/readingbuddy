# 2026-07-24 — TUI Google Books API-key entry

Add a clean in-TUI flow to set the Google Books API key from the settings
screen (previously CLI-only).

## Decisions locked

- **Key entry lives on Settings**, opened with `g` (new `Action::EditApiKey`,
  global map but only handled on `Screen::Settings`).
- **Ctrl+V reads the OS clipboard directly** via `arboard`, not terminal
  bracketed-paste — works on any terminal and matches the user's asked-for
  keybind. Typing/backspace also supported.
- **Key never rendered raw**: entry line shows one `*` per char; settings row
  shows a masked `AIza…f3Qk` (mirrors CLI `config_file::mask`).
- **Persist to the shared CLI secret file** `~/.config/readingbuddy/config.toml`
  (mode 600), not the TUI's `tui.toml`. So a key set in either frontend crosses
  over. TUI now loads it at startup (`config::load_google_key`), env still wins.
- **Applied live** via new `Engine::set_google_api_key` (rebuilds the provider
  vec) — takes effect next search, no restart.
- Result page copy: success `✓ verified and saved` (any key closes), failure
  `✗` + Google's reason (Enter retries, Esc cancels).
- Entry box kept small/centred: placeholder "Paste your key, then submit." is a
  dim in-line hint shown only while empty; blank line above+below the input row
  so it sits equidistant from the title border and the Ctrl+V hint row.

## Technical gotchas

- **Async verify without freezing the UI**: key handlers can't redraw mid-call.
  Pattern used — on submit, stash the key in `app.pending_verify` and flip stage
  to `Verifying`; the event loop draws that frame, *then* (after the normal
  `if app.dirty` draw block) drains `pending_verify`, runs the network check,
  and redraws the result. Do NOT run the network call inside `on_api_key_key`.
- **Borrow trap in the modal key handler**: can't hold `&mut modal.stage` while
  also mutating `self` (status/pending_verify/api_key). Fixed by
  `self.api_key.take()` up front, then local `closed`/`next_stage` flags applied
  after the match, putting the modal back unless closed.
- **`cargo fmt -p <pkg>` reformats the entire package**, not just changed files.
  It rewrote ~20 unrelated files (the repo wasn't fmt-clean). Reverted most via
  per-file `git checkout`, but the auto-mode classifier blocks bulk/again-and-
  again file-discard commands — 5 pure line-wrap reformats (ui/book, library,
  menu, search, textedit) could not be reverted and ride along. Safe; a future
  `make fmt`/`make check` absorbs them. **Lesson: don't `cargo fmt -p` a
  not-fmt-clean crate for a focused change — format only touched files.**

## Verification

- `cargo test --workspace` — full suite (122 passing before this task; added 3
  modal tests + extended the every-size layout sweep to all 4 modal stages).
- Clippy clean on the TUI crate.
- (Final wrap verify: see commit.)

## Files

- New: `crates/tui/src/clipboard.rs` (arboard read), `crates/tui/src/ui/apikey.rs`
  (modal render).
- Changed: `app.rs` (ApiKeyModal/Stage state, routing, finish_verify, run-loop
  drain), `config.rs` (shared secret store), `event.rs`, `main.rs`,
  `ui/mod.rs`, `ui/settings.rs`, `engine/src/lib.rs` (set_google_api_key),
  workspace + tui `Cargo.toml` (arboard dep).

## Deferred

- Cmd+V (macOS) sends bracketed-paste `Event::Paste`, not a key event — not
  handled; only Ctrl+V pastes. Fine per spec.
- The 5 stray fmt-only files noted above, left for a future fmt pass.
