# Calibre fixtures

## `recorded/library.json`

`CLAUDE.md` says fixtures are generated, not hand-written. This is the same
documented exception the Goodreads `recorded/` files are, for the same reason:
**`calibredb list --for-machine` output is a recorded artifact of another
system.** Writing one from our own understanding of the format would prove only
that we agree with ourselves.

It was recorded from **calibre 7.26** (`calibredb --with-library … list
--for-machine --fields all`), against a library built by `calibredb add` of the
two epubs in `epubs/`, with `set_metadata` used to reach the shapes a
freshly-added book does not have. Paths and one description were shortened; every
key, type and sentinel is exactly what calibre wrote.

Three of the four shapes here were found by running calibre rather than by
reading about it, and each was a mistake waiting to happen:

| shape | why it is here |
|---|---|
| `"authors": "Min Jin Lee & Deborah Smith"` — a **`&`-joined string** | Every other multi-valued field (`tags`, `languages`, `formats`) comes back as a JSON array. Deserializing `authors` as one is the obvious guess and it fails on the first book, at runtime, in a `serde` error nobody can act on. |
| `"pubdate": "0101-01-01T00:00:00+00:00"` on the undated book | Calibre's `UNDEFINED_DATE` sentinel. Taken at face value it gives every undated book a `publish_year` of **101** — green tests, wrong shelf. |
| row 3, carrying almost nothing | Calibre **omits** a field entirely when it has no value rather than writing null, so `rating`, `series`, `publisher`, `cover`, `formats` and `identifiers` are simply absent. A struct without `#[serde(default)]` on every field refuses the row. |
| `"isbn": ""` beside a populated `identifiers` map | The dedicated column is empty far more often than not; the map is where the ISBN actually is. And the map's `isbn` key holds ASINs and typos as readily as ISBNs, so it goes through `normalize_isbn` like everything else. |

Two further facts about calibre 7.26 have no fixture because they are not about
parsing, and both are asserted in `calibre.rs` instead:

- `calibredb --with-library /a/typo list` **creates a library there** —
  `metadata.db`, `.calnotes/` — and reports `[]` with exit 0. A mistyped path is
  otherwise indistinguishable from an empty library, having scribbled on the
  user's disk on the way past. `library_root` refuses a directory with no
  `metadata.db` before the binary runs.
- `rating` is 0–10 half-stars in `list` output while `set_metadata --field
  rating:N` takes 0–5. Not imported at all: a rating here lives on a *review*,
  which anchors to a *reading*, which calibre knows nothing about.

There is no `generated/` tier. The Goodreads generator earns its place by
covering volume and variety in a format with real quoting hazards; a JSON array
of the same three shapes repeated four hundred times would test `serde_json`.
