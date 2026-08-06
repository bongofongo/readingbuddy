---
title: Item 37 — a real PDF sidecar, and a `Diagnostic` instead of silence
date: 2026-08-06
branch: feat/engine-pdf-sidecar
---

# Session log

Item 37 of the 2026-08-06 non-GUI wave, built alone in a worktree. No migration
— `0015`/`0016` belong to two sibling threads running at the same time, and
nothing here wanted one. Engine, tier-1 corpus, docs, and the one DTO line the
compiler insisted on.

The subject is a comment that was true when it was written and false everywhere
else. `entry_to_highlight` said:

```rust
// Modern `annotations` mixes highlights and plain bookmarks; a real
// highlight always carries a pos0 xpointer.
let pos0 = get_str(item, "pos0")?;
```

That holds on EPUB. On PDF, KOReader anchors a highlight to a page plus
coordinates — a scanned page has no text stream to point into — so `pos0` is a
**table**, `get_str` returns `None`, `?` returns `None`, and the entry left no
trace at all. Not a count, not a diagnostic, nothing in the report. A user with a
PDF library imported zero highlights and was given no reason, which is
indistinguishable from a book nobody had highlighted.

## The decision the prompt asked for: skipped, with a diagnostic

The prompt offered two readings and asked which. **Skipped-with-a-diagnostic**,
and imported-with-a-different-anchor is a real item that this one deliberately
did not open. Three reasons, and the second is the one that settles it.

`identity_hash = sha256(book_id | ko_datetime | pos0 | text)` is what makes
import idempotent, so serialising a coordinate table into the `pos0` column fixes
the identity of every PDF highlight to whichever serialisation is picked on day
one, permanently.

And coordinates are the *drifting* half of a sidecar. `docs/koreader-format.md`
§3.2 already records `pageno` moving 29→30, 30→31, 42→43 on a re-render **with no
user action** — that is why the identity hash excludes it. An anchor built out of
numbers the device rewrites is an anchor that re-inserts the same highlight after
every re-render, which is precisely the failure mode the tier-2 layout split
exists to make observable. `DeviceDigest` and `DEVICE_FIELDS_DIFFER` would both
have to agree about it as well.

Third: it cannot be designed here. The anchor's *shape* is settled and its
*contents* are unobserved, so that item needs a real PDF sidecar before it needs
code.

## The decision the prompt warned about: one diagnostic per file

The prompt flagged it — *a 300-highlight PDF that emits 300 diagnostics has
replaced silence with noise, which is not better* — and it is right. **One
`SidecarAnchorsUnsupported` per file, carrying the count.** `KoSidecar` grew a
`usize` and not a `Vec`, so the shape of the type is the decision rather than a
convention a later caller can drift from.

It is emitted in `import_into`, beside `UnknownDeviceStatus`, on that variant's
precedent: both are facts derived from the parse alone, and `import_into` is the
one place `import` and `import_book_from_sidecar` both reach, so the two paths
cannot grow separate opinions about how a degradation is reported.

## The pushback: no `ErrorClass`

The prompt said "`ErrorClass` gets a new variant if one is warranted". It is not,
and the reason is worth writing down because the instruction reads like a
requirement.

`ErrorClass` is `From<&EngineError>`. It exists to classify something that
**failed**. Nothing failed here: the file read, the chunk evaluated, the entry is
well formed, and we simply cannot represent it. There is no error to classify, so
a class on this variant would be a field with no source — and `ErrorClass::from`
would have to be handed something to produce it, which would mean inventing an
`EngineError` for a path that has none.

The engine's rule is "a partial failure returns a typed `Diagnostic`", not "every
`Diagnostic` carries an `ErrorClass`". Half the existing variants already carry
none: `NoSidecarsFound`, `SidecarUnparsable`, `SidecarNotIdentified`,
`UnknownDeviceStatus`, every Goodreads variant, three of the five statistics
ones.

## The pushback the prompt nominated: is the fixture a fiction?

The prompt's own words: *a synthetic PDF sidecar may encode a `pos0` table shape
that a real device does not produce — in which case the fixture is a fiction that
the goldens then enshrine, and saying so is worth more than the fixture.*

Half right, and the half that is right is fixable by construction rather than by
abandoning the fixture.

What is **settled**: on a paging document `pos0`/`pos1` are tables and not
strings. That is source-derived, from the same `readerrolling`/`readerpaging`
split that gives one a cre xpointer and the other a page number, and
`docs/koreader-format.md` has recorded it since item 1.

What is **not settled**: the keys inside that table. `Gen-Pdf-Anchors.sdr` writes
`page`/`x`/`y`/`zoom`/`rotation` with a sibling `pboxes` array, and nobody here
has ever read the file that models. Those key names are a reconstruction.

The resolution is that **nothing reads them**. `koreader::anchor` branches on the
value being a Lua table and on nothing inside it, so no golden can bless a key
name, and if a real device turns out to write `pos` or `rect` the fixture and the
engine both stand unchanged. The fixture asserts one fact and illustrates the
rest, and the file says so in its own comment.

`docs/koreader-format.md` §6 now separates the two in those words, because the
alternative was a synthetic fixture quietly promoting itself to an observation —
which is the exact thing that section exists to prevent.

One thing found on the way: **`Gen-Pdf-Sidecar` was never a PDF sidecar.** It has
`metadata.pdf.lua` as its filename and cre xpointers inside, so it covers what
`is_sidecar_file` accepts and not the format. Its name has read for months as
though the case were covered. It is untouched — changing it would have moved a
committed highlight count — and it now carries a comment and a manifest note
saying which half it is.

## `has_warnings` became a list of names, and that was not tidying

The golden shape carried `"has_warnings": bool`. That boolean was `true` for a
sidecar with an unknown device status and `true` for one whose highlights could
not be anchored, so no golden could tell one degradation from another. A fixture
asserting "something warned" is green when the wrong thing warns, and green when
the right thing is replaced by a different wrong thing.

The prompt's "done when" is that the goldens show the skipped entries **as
diagnostics rather than as an absence** — and a bare `true` beside an empty
highlight list is an absence with a flag on it. So `warnings` is now a sorted
list of variant names, with the count folded into this one's:

```json
  "warnings": [
    "SidecarAnchorsUnsupported(3)"
  ]
```

`warning_name`'s catch-all **panics** rather than bucketing into "other". A new
koreader diagnostic has to be taught to that function on purpose; silently
folding it into a catch-all is how the next silence gets committed.

That is a one-line change in 22 existing goldens and no other line moved.

## The change is scoped to one Lua value type

`anchor` treats a **table** as unstorable and sends every other value through the
same `get_str` as before. A numeric `pos0` still coerces to its digits (Lua
coercion, which `mlua`'s `FromLua for String` performs), an empty one is still
filtered, a missing one is still a bookmark.

That matters because the alternative — testing for `Value::String` and rejecting
everything else — would have changed what a numeric `pos0` does, on no evidence
about what a numeric `pos0` means. Narrowing it to one value type makes "no
reflowable sidecar imports differently" a property of the code rather than a
claim about which fixtures happen to be committed.
`only_a_table_anchor_changed_behaviour` pins it.

## The golden diff, in numbers

Imported highlights per fixture, before → after. Twenty-two fixtures, all
unchanged; one row added.

| fixture | before | after |
|---|---|---|
| Empty | 0 | 0 |
| Gen-Both-Layouts | 1 | 1 |
| Gen-Duplicate-Entries | 1 | 1 |
| Gen-Holey-Annotations | 3 | 3 |
| Gen-Incomplete-Entries | 1 | 1 |
| Gen-Isbn-Match | 1 | 1 |
| Gen-No-Datetime | 2 | 2 |
| Gen-No-Doc-Props | 0 | 0 |
| Gen-Not-A-Table | 0 | 0 |
| Gen-Not-Utf8 | 0 | 0 |
| **Gen-Pdf-Anchors** | *(new)* | **0**, + `SidecarAnchorsUnsupported(3)` |
| Gen-Pdf-Sidecar | 1 | 1 |
| Gen-Runaway-Loop | 0 | 0 |
| Gen-Stats | 2 | 2 |
| Gen-Summary | 1 | 1 |
| Gen-Summary-Legacy | 1 | 1 |
| Gen-Summary-Unknown-Status | 1 | 1 |
| Malformed | 0 | 0 |
| Multi-Chapter | 4 | 4 |
| Pachinko | 2 | 2 |
| The-Trial | 2 | 2 |
| Unicode | 2 | 2 |
| Unmatched | 0 | 0 |

Three of `Gen-Pdf-Anchors`' four entries are counted. The fourth is a plain
bookmark with no `pos0` of any shape, and it is deliberately **not** in the
count: a bookmark is not a highlight that went missing, and a diagnostic that
inflated its number would be a worse lie than the silence it replaced. An entry
with no `text` is excluded for the same reason — it would not have imported
however good its anchor was — which is why `text` is still tested first.

## What crossed the seam, and why

The prompt said "no API". One line crossed anyway, and it was the compiler's
call rather than a design choice: `DiagnosticKindDto` mirrors `DiagnosticKind`
**in full** (`crates/api/CLAUDE.md` states that deliberately), so the exhaustive
`From` match does not compile without the new variant. `make ts` regenerated
`bindings.ts`; the diff is that one union member and nothing else. Nothing in
`gui/src` reads `DiagnosticKind`, so no frontend switch broke.

`crates/corpus/Cargo.toml` is unchanged and still does not depend on
`readingbuddy`.

## Gate

`make fmt lint build-check test ts-check`, plus `make synthetic` and
`make golden`. All green. `make synthetic` run twice produces a byte-identical
tree, so the generator is deterministic. `make ci` was not run — a fresh worktree
has no `gui/node_modules`, so `web-check` and `routes` would print `SKIPPED:` and
pass without running. `make corpus` was not run: tier 2 needs gutenberg.org.

The two `ts-rs failed to parse this attribute` warnings are pre-existing —
`#[serde(other)]` on `ErrorCode::Internal`, documented in `crates/api/CLAUDE.md`
— and confirmed present on a clean tree.

## Left for later

- **Storing a PDF anchor rather than counting it.** Its own item, argued above.
  It needs a real PDF sidecar with annotations in it first; until then the
  contents of that table are a reconstruction and any column shape derived from
  them is a guess with a schema attached.
- **`device.rs` does not report this.** `scan_device` shows a PDF book as `New`
  or `Unchanged` with no highlights and says nothing about the anchors — the
  `DeviceState` vocabulary has four names and none of them is "readable, and
  partly unstorable". Out of scope here (this item's surface is the import
  report) but it is the same silence one layer over, and it is written down
  rather than discovered later.
