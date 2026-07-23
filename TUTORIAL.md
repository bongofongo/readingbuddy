# readingbuddy — CLI Tutorial

A walk through every feature, in the order you'd naturally meet them.

```
cargo build --workspace
alias rb='cargo run -q -p readingbuddy-cli --'   # or install: cargo install --path crates/cli
```

All examples below use `rb` for brevity.

---

## 0. Where your data lives

By default everything is created relative to the current directory:

| Path | What |
|---|---|
| `database/app.db` | SQLite database (created + migrated automatically) |
| `database/images/` | Downloaded / extracted cover images |
| `vault/` | Your notes as Markdown — open this folder as an Obsidian vault |

Override the root with `--data-dir <path>` on any command, or set it once:

```
export READINGBUDDY_DATA_DIR=~/reading      # everything lands under ~/reading
```

Tip: pin `READINGBUDDY_DATA_DIR` in your shell profile so the library is the same no matter where you run `rb` from.

### Google Books API key (recommended)

Google Books works keyless but rate-limits aggressively (you'll see 429 warnings). A free key fixes that: create one at [console.cloud.google.com](https://console.cloud.google.com) (enable the **Books API**), then store it:

```
rb config set google-api-key            # hidden prompt — key stays out of shell history
rb config set google-api-key --verify   # same, but live-checks the key before saving
rb config get                           # shows it masked: AIza…f3Qk (from config file)
rb config path                          # ~/.config/readingbuddy/config.toml (mode 600)
rb config unset google-api-key
```

Precedence when several are set: `--google-api-key` flag > `GOOGLE_BOOKS_API_KEY` env > config file. The key is redacted (`key=REDACTED`) from any warning or error output, so it never leaks into logs or terminal scrollback.

---

## 1. Finding books — `search`

Federated search: OpenLibrary and Google Books are queried concurrently, results deduped and ranked. If one provider is down or rate-limited you get a warning and the other's results — never a dead search.

```
rb search "left hand of darkness"                 # free text
rb search --title pachinko --author "min jin lee" # fielded
rb search --author "ursula k le guin" --lang en --year 1969
rb search --publisher "new directions" --translator "charlotte mandell"
rb search --isbn 9781455563937                    # exact-ISBN match ranks first, always
```

Output is a ranked list:

```
  0. Pachinko — Min Jin Lee (2017)  isbn:9781455563937  [openlibrary+googlebooks, 79.4]
  1. ...
save which? (number, Enter to skip):
```

- Type a number to save that book to your library (cover downloads automatically).
- `Enter` skips — search doubles as a browsing tool.
- Flags: `--no-save` (browse only), `--pick 0` (save without the prompt — good for scripts), `--no-cover`, `--limit 30`.

The `[openlibrary+googlebooks]` tag tells you both providers agreed on the edition — a good confidence signal. The number is the relevance score.

## 2. Adding without searching

```
rb add 9781455563937            # direct ISBN lookup (both providers, fields merged)
rb add 9781455563937 --no-cover
```

Hyphens/spaces in ISBNs are fine; ISBN-10 with an X check digit works too. Invalid checksums are rejected up front.

## 3. Importing an epub — `epub`

```
rb epub "epubs/Station Eleven (Emily St. John Mandel).epub"
```

What happens: the epub's metadata is scanned for a *valid* ISBN (UUID identifiers are ignored) → providers enrich it → the embedded cover is extracted into `database/images/`. If the epub has no usable ISBN or you're offline, the epub's own title/author metadata is used as a fallback — you still get a library entry.

## 4. Browsing your library — `list`, `show`

```
rb list                         # most recently touched first
rb list --sort title --limit 50
rb list --sort progress         # closest-to-finished first
rb show pachinko                # selector = title fragment...
rb show 9781455563937           # ...or ISBN...
rb show 1                       # ...or the #id from `list`
```

Every command that takes a book accepts that same **selector** (id / ISBN / title fragment). If a fragment is ambiguous you'll get the candidate list — use the `#id`.

`show` prints all metadata plus note/highlight counts.

## 5. Reading progress — `progress`

```
rb progress station 120         # I'm on page 120 (date_started stamps itself the first time)
rb progress station --finished  # done!
```

Finishing prints a small celebration and nudges you toward final thoughts:

```
🎉 finished Station Eleven! congratulations.
capture your final thoughts:  readingbuddy note --book 2 --kind final
```

Re-importing book metadata later will never un-finish a book.

## 6. Notes — `note`, `notes`

The heart of the tmux workflow. Notes are **Markdown files in `vault/`**; the database only indexes them.

```
rb note "Sunja's dignity under pressure" --book pachinko
rb note --book pachinko                       # no text -> opens $EDITOR
rb note "loose thought, no book attached"     # bookless notes land in vault/unsorted/
rb note "read ch 4-6, slow burn" --book 1 --kind session
rb note --book 1 --kind final                 # kinds: note | session | final
rb note "..." --title "On dignity"            # otherwise title = first six words
```

**Wikilinks are live.** Write `[[Han]]` in any note body and readingbuddy records the link. If a note titled "Han" doesn't exist yet, the link is kept as a dangling reference and resolves automatically the moment you create it (zettelkasten forward references). Obsidian sees the same `[[links]]` natively — just open `vault/` as a vault.

Reading them back:

```
rb notes                        # all notes, newest first
rb notes pachinko               # one book's notes
rb notes --search "dignity"     # full-text search (FTS5) with snippets:
#1    Sunja's dignity under pressure  (pachinko/20260723...md)
      Sunja's >>dignity<< under pressure
```

Heads-up: if you edit a note file externally (Obsidian/vim), the full-text index does **not** pick up the change yet — the engine has the refresh hook (`refresh_note_from_disk`) but no `notes sync` CLI command exposes it so far. The file itself is always the source of truth.

### tmux binding

The one-shot shape is designed for a popup. In `.tmux.conf`:

```
bind-key N display-popup -E -w 60% "cd ~/reading && readingbuddy note --book \"$(readingbuddy list --limit 1 | head -1 | sed 's/#\\([0-9]*\\).*/\\1/')\""
```

(simplest version: `bind-key N display-popup -E "readingbuddy note"` — opens $EDITOR, saves, closes.)

## 7. KOReader highlights — `ko import`

Point it at anything: a whole KOReader library, one `.sdr` folder, or a single `metadata.epub.lua`:

```
rb ko import /Volumes/KOBOereader/Books --dry-run   # see what would happen
rb ko import /Volumes/KOBOereader/Books
rb ko import "Station Eleven.sdr"
```

- Sidecars are matched to your library by the sibling epub's ISBN first, then by fuzzy title match against `doc_props`.
- **Unmatched sidecars are reported, not imported** — add the book (`rb search` / `rb epub`), re-run, done.
- Imports are **idempotent**: run it after every reading session; already-known highlights count as "already known", nothing duplicates. Both modern (2024+) and legacy KOReader sidecar formats parse.
- Highlights carry their KOReader note, chapter, and page.

```
rb highlights station
  p.31 [Section 1] “Survival is insufficient.”
      ↳ The Star Trek line as thesis.
  p.88 [Section 2] “prophet”
```

## 8. Flashcards — `cards`

Any **single-word highlight** you make in KOReader (the classic "highlight a word you don't know") automatically becomes a flashcard candidate on import — word, context (your note or the chapter), source book.

```
rb cards list                   # pending candidates
rb cards list --all             # include already-exported
rb cards export --out anki.tsv  # Anki-ready TSV (File > Import in Anki)
rb cards export --all           # re-export everything
```

Exported cards are marked so the next `export` only ships new ones.

## 9. Interactive mode — `repl`

```
rb repl
```

Menu-driven loop (`s` search, `r` epub, `d` library, `n` note, `k` koreader import, `rd` remove, `e` exit) — the descendant of the original BookBuddy loop, for when you'd rather sit in the program than fire one-shots.

## 10. Removing — `rm`

```
rb rm "station"        # asks for confirmation, deletes cover image too
rb rm 2 --yes          # no prompt
```

Deleting a book cascades to its highlights and flashcards; its notes stay on disk and in the index, just unlinked from the book.

---

## A full session, end to end

```
rb search --title pachinko --author "min jin lee"   # -> save #0
rb progress pachinko 50
rb note "History has failed us — [[opening lines]]" --book pachinko --kind session
rb ko import ~/koreader-library                     # after reading on the ereader
rb highlights pachinko
rb cards export --out ~/anki.tsv
rb progress pachinko --finished
rb note --book pachinko --kind final                # $EDITOR, write the retrospective
rb notes --search "failed"                          # find it all again later
```
