# migrations/

`sqlx::migrate!` runs every file here on connect, in numeric order.

## The rules, and they are hard

- **Never edit an applied migration.** A new schema change is a new numbered
  file. CI enforces this: the `migrations` job on every PR refuses a migration
  that was modified, deleted or renamed (`git diff --diff-filter=MDR` against
  the merge base).
- **Pre-allocate the number before parallel threads start.** Parallel branches
  are how two threads both claim `0008`, and a duplicate version is *not* a git
  conflict — the filenames differ past the number.
  `migration_versions_are_contiguous_from_one` in `tests/migrations.rs` is what
  catches it.
- **A destructive migration back-fills first, then drops.** `0005` is the first
  one in the repo and does back-fill → `DROP COLUMN` ×4 in that order;
  `tests/migrations.rs` applies the set in two halves precisely to exercise the
  back-fill, which a fully-migrated database can never reach again.
- An **indexes-only** migration is judged on `EXPLAIN QUERY PLAN` before and
  after — "the index is used" is the only claim it can make. See `0008` and
  `the_note_link_indexes_are_the_plan_the_planner_picks`.
- Collation is part of an index. `idx_note_links_target` is `COLLATE NOCASE`
  because back-resolution compares that way, and SQLite silently ignores an
  index whose collation differs.

## What each one did

The reasoning behind every migration lives beside the module that uses it:
`0001`–`0002` and `0007`–`0008` in [`../CLAUDE.md`](../CLAUDE.md) under
"notes.rs"; `0003`/`0006` under "device.rs"; `0004` under "koreader.rs"; `0005`
and `0011`–`0012` in [`../src/storage/CLAUDE.md`](../src/storage/CLAUDE.md);
`0009` under "goodreads.rs"; `0010` under "files.rs"; `0013`, `0014`, `0015` and `0016`
in [`../src/storage/CLAUDE.md`](../src/storage/CLAUDE.md) and, for what `0013`
deliberately did *not* add, under "epub.rs" in [`../CLAUDE.md`](../CLAUDE.md);
`0017` under "moments.rs" and `0018` under "Readings" in
[`../src/storage/CLAUDE.md`](../src/storage/CLAUDE.md).

`0019` and `0020` are the only migrations here that describe **a piece of
hardware** rather than a book or the app's own bookkeeping, and they split one
subject on purpose: `0019` is the *relationship* (a reader we wrote a plugin to,
identified by a uuid we minted — never by `last_mount_path`, which moves), and
`0020` is *what has come off it*. They are the file to read before adding a
fifth timestamp to `paired_devices`, because it already carries three that are
easy to conflate: `installed_at` is when the pairing began and a reinstall must
not move it, `last_seen_at` is when the reader was last in hand (item 55 made
`plugin_status` stamp it, so it finally means that), and `last_synced_at` is when
*everything* on it was last brought across. Reading either of the first two as
the third is what tells somebody who plugged a Kobo in to charge it that their
highlights are here.

`0020` is also the repo's **fourth deliberate non-back-fill**, and the cleanest
case of the four: `0012` had signals that recorded who was *consulted* rather
than who supplied a field, and `0014`/`0016` could not back-fill because SQLite
cannot decode a PNG or parse a name. This one has nothing to reason from at all
— `sync_device` has never recorded which mount its paths came from, so no row in
`device_books` or `sidecar_seen` could attribute a past import to a device id.
`NULL` therefore means *not since we started recording*, which is a claim about
our records, and every caller has to word it that way.

`0018` is the **second indexes-only migration on a sort key**, and it is the one
to read before writing a range filter over a timestamp. `0016`'s collation
lesson has a twin here: SQLite declines an index whose *column is wrapped in a
function* as silently as one whose collation differs, so
`date(finished_at, 'unixepoch') BETWEEN ? AND ?` — which is what
`activity_summary` had asked since `0011` — could never have used
`idx_readings_finished_at`. The conversion to raw unix bounds happens once, in
`DayRange::unix_bounds`, and the file's own control is a *test* rather than an
unindexable sort: `0016` had `BookSort::Progress` to prove its assertions could
fail, and every arm of `ReadingSort` is indexed, so the control is the year
filter written the old way and watched to lose the index.

`0017` is the one migration here that adds **no fact about a book** — both its
tables are what the app knows about *itself*. That is also why it carries the
repo's first `strftime('%s','now')`: "when did this database learn about
moments" is a thing a migration knows and no back-fill can reconstruct.

`0015` is the **first migration with triggers**, and the argument for them is in
the file: it is the inverse of the one that keeps `notes_fts` trigger-free. A
note's body is not in the database, so a trigger has nothing to read;
`highlights.text` is a column, so a trigger can read it and an
*external-content* table can index it without a second copy. Read it before
adding any FTS index — it also records the two things that were **measured**
rather than assumed (a foreign-key cascade *does* fire the delete trigger, and
`AFTER UPDATE OF <columns>` is what keeps `attribute_highlights` and
`merge_books` from paying a reindex per row for columns the index does not
hold), and it back-fills with `'rebuild'`, which is the one-statement back-fill
an external-content index gets for free.

`0014` is the repo's second **deliberate non-back-fill**, and unlike `0012` it
could not have had one at all: `cover_width` is the result of decoding a PNG
and SQLite cannot decode one. The back-fill is a *command*
(`Engine::measure_stored_covers`) writing through the same `Storage::set_cover`
the download path uses, so a back-filled row and a fresh one are the same row.
It is also the migration to read before adding a fifth cover column: it argues
why the four it adds are **not** `MERGE_RULES` rows and why `Rule::pair` is not
the fix.

`0016` is the **third** deliberate non-back-fill and it is `0014`'s case one
layer over: `cover_width` is the result of decoding a PNG and SQLite cannot
decode one; `sort_author` is the result of parsing a human name and SQLite cannot
parse one. So the back-fill is again a command (`Engine::rebuild_sort_keys`,
behind `rb sort-keys`, which `make dev-db` runs) writing through the same
`Storage::refresh_sort_keys` every live write goes through. It is also the
migration to read before adding a sixth index: it argues why
`idx_books_sort_title` is on an **expression** (`COALESCE(sort_title, title)`,
which is what let it ship with no SQL back-fill of `sort_title` and keep the
article-stripping rule in one dialect), why that index is `COLLATE NOCASE` and
`idx_books_sort_author` deliberately is not, and why `sort_author` is nullable
rather than defaulted. `0008`'s collation lesson is asserted twice here —
`a_sort_title_index_that_nearly_matches_is_not_used` pins that dropping the
collation *or* indexing the bare column loses the index silently.

`0012` is the repo's first **deliberate non-back-fill**, and the argument is in
the file itself: every signal that might attribute an existing row
(`openlibrary_key`, `googlebooks_id`, `external_ids`, `device_books`) records who
was *consulted*, not who supplied the field beside it, and there is no honest
reading of `created_at`/`last_modified` as a `fetched_at`. A migration that
guesses is worse than a table that says "unattributed" — which is exactly what
an absent row means, and what every caller has to handle anyway.
