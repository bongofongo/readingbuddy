# 2026-07-24 — note anchors + full TUI (parity → in-house editor → menu layout)

Big session, three waves: (1) anchor notes to a location + bring the CLI's whole
feature set into the TUI; (2) replace `$EDITOR` with an in-house editor after the
user's vim turned out broken; (3) rework the book view — header on top, book left,
a section *menu* (not tabs) on the right.

## Decisions locked
- **Note anchor is flexible, per-note, engine-dumb.** Notes gain nullable
  `page` + `location`; `highlight_id` (already existed) covers attach-to-highlight.
  Engine only *stores* what the frontend hands it. CLI auto-defaults page to the
  book's `current_page`; the **TUI asks** for the page after the body (empty /
  invalid / Esc = no page). PDF→page, on-device→highlight, manual→location.
- **In-house editor replaces `$EDITOR` everywhere** (new + edit). User's `$EDITOR`
  unset and the `vi` fallback was a broken Homebrew vim (missing `libperl.dylib`,
  SIGABRT). Notes are 2–3 sentences → a tiny in-pane editor beats suspending the
  TUI. Deleted `editor.rs` (suspend/restore) entirely.
- **Editor keys:** Enter saves, Esc cancels, newline via Shift/Alt+Enter or Ctrl-J.
- **Book view = header + book-left + menu-right.** Dropped the tab strip (names
  truncated on narrow panes) for a single-column section menu (Info/Notes/
  Highlights/Cards). ↑↓ move menu, Enter/→ opens a section into the right pane,
  Esc/← backs out. Title+progress pinned on top, book keeps spinning on the left.
- **Manual book rotation removed** — arrows now drive the menu/list. `space`
  toggles spin, `r` resets pose.
- **Split layout down to small squares.** `book_layout` → `{Split, Bare}`; splits
  whenever `width≥26 && height≥8` (was `width≥80 && landscape`). Panel width
  `(w*2/5).clamp(16,34)`, object keeps the rest. Bare (too small) = object only.
- **Search runs interactively in the event loop** (single-line query input), not
  via `$EDITOR`. Global actions (search/add, add-isbn, ko-import, settings,
  remove) live on the menu; config API-key editing stays CLI-only (600-mode file).

## Technical gotchas
- **Parallel tests shared one on-disk vault → wiped each other.** `test_app()`
  keyed the tmp dir by pid only; tests run in-process in parallel, and each did
  `remove_dir_all` at start. Symptom: a note file vanished mid-test
  (`NotFound`). Fix: per-invocation dir via a `static AtomicUsize` counter. The
  in-memory DB was already isolated; only the vault leaked.
- **Same-second `created_at` makes note ordering ambiguous.** `list_notes` is
  `ORDER BY created_at DESC` (unix *seconds*); two notes in the same second tie
  and SQLite returns them by rowid. Don't assert "newest = index 0" in tests —
  assert `notes.iter().any(|n| n.page == …)` instead.
- **`frontmatter_and_body` byte arithmetic.** Preserving the raw header on a body
  rewrite needs exact offsets: `after_close = "---\n".len() + rest.find("\n---") +
  "\n---".len()`, then skip trailing `\n`s. `parse_frontmatter` now delegates to it
  and filters out the two `---` marker lines (the old code sliced *between* them).
- **Editor mode routing order matters.** `dispatch_key` checks `note_editor` →
  `input` → `confirm` → `map_key`. Same shape as the existing input mode. `tick()`
  must also pause the spin while `note_editor.is_some()` (like `input`), else the
  book animates under the modal.
- **`NotePage` input commits even when empty.** Its handler runs *before* the
  generic empty-text early-return in `commit_input`, and Esc is special-cased to
  save-without-page (don't lose the written body).
- **Shift+Enter often isn't delivered.** Terminals only send it under the kitty
  keyboard protocol; hence the Alt+Enter / Ctrl-J newline fallbacks.
- **A failing editor must not kill the TUI.** (Pre-in-house) the `$EDITOR` error
  propagated up through `run()` and crashed the app — now moot since it's gone,
  but the lesson stuck: note actions report errors to the status line.
- **Panel border side flips with the split order.** Object-left/panel-right → the
  rule is `Borders::LEFT` on the panel (was `RIGHT` when the panel was on the left).

## Verification
- `cargo test --workspace` — 44 engine + 59 tui + doctests, 1 ignored
  (`print_layout` dev aid). New: anchor-in-frontmatter+DB, `frontmatter_and_body`
  split, `list_flashcards_for_book`, textedit edit-ops, page-prompt (filled +
  empty), menu→section nav, spin pause/resume, edit-preserves-frontmatter.
- `every_screen_draws_at_every_size` now also renders each section both as menu
  and entered, plus the input/confirm/editor overlays — the layout-panic guard.
- clippy `--workspace --all-targets` clean.
- CLI smoke: `note --location intro --page 5` → frontmatter has `page:`/`location:`,
  `notes` lists `@p.5, intro`.
- `--dump-frame` + `print_layout` (44×26): title+progress on top, book left,
  section menu right, new key-bar hints.

## Deferred / unverified
- **Live TUI feel** — interactive editing, the page prompt, small-square resize,
  and the menu→section flow only verified via TestBackend + `print_layout`. User
  to eyeball `cargo run --release -p readingbuddy-tui`.
- **Search + KOReader import** need network / real sidecars — engine paths reused,
  TUI wiring untested against live data.
- **API-key editing in the TUI** intentionally out of scope (Settings is read-only;
  CLI `config set` owns the 600-mode file).
