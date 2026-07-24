# 2026-07-24 — TUI keybindings, editor fixes, note deletion

Small-feature session on the ratatui TUI: a back keybind, tab/shift-enter in the
note editor, a cursor-position bug, and note deletion with confirmation.

## Decisions locked

- `b` is an additional "back" (alongside `Esc`/`←`) — plain alias in `event.rs`,
  reaches `map_key` only when no editor/input/confirm is open, so text entry is
  unaffected.
- Tab inserts a **real `\t`** into the note buffer (saved notes get a genuine
  tab), not soft spaces — expansion is a render-time concern only.
- Deleting a note **and** discarding a written-but-unsaved draft both go through
  the existing confirm overlay (`y` = yes, anything else = no). A *blank* draft
  still discards silently (no prompt).
- `d` deletes a note only in the **open Notes section** (`in_section && Notes`);
  inert on the section menu and other tabs. Mirrors the library's `d`.

## Bugs found / fixed

- **Literal `\t` corrupts the whole pane.** Writing a raw tab into a ratatui
  cell makes the terminal jump to *its* next tab stop while ratatui thinks it
  advanced one cell → the cell diff desyncs, text smears across panes, the note
  popup bleeds the TUI behind it. Fix: keep `\t` in the model, expand to spaces
  at render time so no raw tab ever reaches a cell. Also expand tabs in the
  Notes-list title span (`book.rs`), since a saved note's title line can carry a
  tab now.
- **Cursor floated one row below the text on an empty line.** An empty line
  carries only the reversed cursor cell = a whitespace-only line, and ratatui's
  `WordWrapper` (`Wrap { trim: false }`) emits a *phantom blank row* before a
  whitespace-only line. Leading-whitespace *content* lines are fine; only
  whitespace-only ones trip it. Fix: dropped `Wrap` entirely and character-wrap
  the editor manually in `wrapped_lines` (expand tabs → cells, place the reversed
  cursor at its exact display column, wrap to next row at the edge, scroll
  vertically to keep the cursor visible). Cursor's visual row is now exact.

## Technical gotchas

- **Shift+Enter needs the Kitty keyboard protocol.** Plain terminals report
  Shift+Enter as a bare Enter (no modifier), so the `SHIFT|ALT` newline chord
  never fires. Enabled `PushKeyboardEnhancementFlags(DISAMBIGUATE_ESCAPE_CODES)`
  in `setup_terminal`, gated on `supports_keyboard_enhancement()`, popped on
  restore (pop is safe to send unconditionally). **Caveat: tmux usually doesn't
  pass this through**, so inside tmux Shift+Enter still degrades to bare Enter —
  `Alt+Enter` / `Ctrl+J` remain the reliable newline chords there.
- **FTS5 doesn't cascade.** `notes_fts` is a virtual table, so a foreign key on
  `notes` can't cascade into it. `delete_note` must `DELETE FROM notes_fts WHERE
  rowid = ?` explicitly (same transaction as the `notes` row delete). `note_links`
  *does* cascade via FK (from_note ON DELETE CASCADE; to_note ON DELETE SET NULL —
  links pointing at the deleted note degrade to dangling text, which is the
  intended zettelkasten behavior).
- Engine `delete_note` removes the vault file too; a missing file (`NotFound`) is
  swallowed so the DB row still goes.
- `resolve_confirm` needed restructuring: the old generic `if !yes { "kept."; return }`
  early-return can't express "declining a discard restores the editor". Now each
  variant owns its yes/no branch; the draft is held in a new `pending_discard`
  field while the confirm is open.

## Verification

- `cargo test --workspace` + `cargo clippy --workspace --all-targets`: green
  (119 tests at the point of the note-delete change; clippy clean).
- Editor render bugs reproduced first with a throwaway `TestBackend` dump test
  (removed after), then locked with assertions: empty-trailing-line, fresh-note,
  typed, and long-line-wrap cursor rows.

## Deferred

- Nothing outstanding from this session. Manual wrapping is character-wrap (not
  word-wrap) — a deliberate simplification, fine for 2–3 sentence notes.
