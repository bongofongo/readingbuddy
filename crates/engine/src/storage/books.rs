use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use time::OffsetDateTime;

use super::field_provenance::{self, Source};
use super::highlights::identity_hash_of;
use super::{Storage, now_unix};
use crate::book::{Book, series_index_text};
use crate::error::{EngineError, Result};

/// What [`Storage::list_books`] orders by.
///
/// **`limit` selects along the sort key, in every arm alike.** `list_books(200,
/// Title)` is the first two hundred books alphabetically, not two hundred
/// arbitrary books shown alphabetically — which is what `LIMIT` means and what
/// a paginated shelf needs. The TUI wants the *other* thing, deliberately: it
/// fetches one page by recency and reorders that same page in Rust, so pressing
/// `s` reorders the list rather than swapping its contents
/// (`crates/tui/src/app.rs`). That is a frontend policy about a fixed page and
/// it stays there; the two are not in tension as long as nobody mistakes one for
/// the other, which is what this paragraph is for.
///
/// [`BookSort::Author`] is the one key SQL cannot express — see its own note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookSort {
    LastModified,
    Title,
    Progress,
    /// Alphabetically by the first author's **last name**, authorless books
    /// last.
    ///
    /// The only arm not ordered by `ORDER BY`, and it cannot be: "last name" is
    /// a parse of a human name ([`crate::names`]) and SQLite has nothing to
    /// parse one with. So this arm reads the library, sorts in Rust and *then*
    /// truncates — which keeps the contract above (the first N by this key) at
    /// the cost of a whole-table read.
    ///
    /// A `sort_author` column computed on write is the obvious follow-on and is
    /// deliberately not done here: it is a migration, and item 17 has none. Do
    /// the simple thing, then measure — a library where this hurts is a library
    /// large enough to have said so.
    Author,
    /// Newest first, undated last.
    ///
    /// Descending like every other list in this app. An ascending year sort
    /// would be the one place the newest thing is at the bottom.
    Year,
}

/// What [`Storage::merge_books`] actually moved.
///
/// The dropped counts are the interesting ones: they are rows that existed on
/// both sides and are gone from the count of what survived, which is the only
/// way a caller can tell "merged cleanly" from "merged and lost duplicates".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// False when `src` was already gone — the merge had nothing to do. Every
    /// other field is then zero, which is what makes a repeat merge a no-op
    /// rather than an error.
    pub src_existed: bool,
    pub highlights_moved: usize,
    /// Highlights that collided on identity with one already on `dst`. Dropped,
    /// not duplicated; anything anchored to the dropped copy was repointed at
    /// the survivor first.
    pub highlights_dropped: usize,
    pub notes_moved: usize,
    /// Readings repointed at `dst`. Both books may have had an open reading;
    /// the older one is closed as `abandoned` rather than deleted, so it is
    /// counted here like any other.
    pub readings_moved: usize,
    pub flashcards_moved: usize,
    /// `flashcards` is `UNIQUE(book_id, word)`, so a word both books captured
    /// cannot move.
    pub flashcards_dropped: usize,
    pub device_links_moved: usize,
    /// Owned files repointed at `dst`. No `dropped` twin: `book_files` is keyed
    /// on the sha256 alone, so the same content cannot already be on both sides
    /// and a collision is not representable.
    pub files_moved: usize,
    /// `src`'s cover file, when `dst` already had one of its own and therefore
    /// kept it. The file is now unreferenced; the caller deletes it, the same
    /// contract [`Storage::delete_book`] has.
    pub orphaned_cover: Option<String>,
}

/// The `books` columns plus the reading-state **projections**.
///
/// Six of them now. `current_page`, `finished`, `date_started` and
/// `date_finished` left `books`
/// with migration `0005` and now come off the current reading; `reading_status`
/// (item 25) and `ko_percent` (item 17) joined them, both because a value a
/// frontend needs *per row* is a value a list cannot afford one query each for.
/// Keeping them on
/// [`Book`] as read-only projections is what left every consumer — the CLI's
/// `render.rs`, the TUI's `progress_tag` and `progress_text`, the note page
/// auto-anchor — untouched by that move, and [`row_to_book`] unchanged with it.
///
/// Every `books` column is qualified: `readings` carries `id`, `created_at` and
/// `last_modified` of its own, and an unqualified name would be ambiguous.
pub(super) const BOOK_COLUMNS: &str = "books.id, books.title, books.sort_title, books.authors, \
     books.translators, books.publisher, books.publish_year, books.language, books.isbn_10, \
     books.isbn_13, books.openlibrary_key, books.googlebooks_id, books.cover_url, \
     books.cover_path, books.page_count, books.description, books.first_sentence, \
     books.subjects, books.series, books.series_index, \
     cur.current_page AS current_page, \
     CASE WHEN cur.status = 'finished' THEN 1 ELSE 0 END AS finished, \
     cur.status AS reading_status, cur.ko_percent AS ko_percent, \
     cur.started_at AS date_started, cur.finished_at AS date_finished, \
     books.created_at, books.last_modified";

/// The join that resolves **the current reading**: the open one if there is
/// one, else the most recent.
///
/// One join rather than four correlated subqueries, and one definition of
/// "current" rather than a different one per column — a book whose `finished`
/// came from its last reading while its `current_page` came from nowhere would
/// render as a contradiction.
pub(super) const BOOK_FROM: &str = "FROM books LEFT JOIN readings cur ON cur.id = (
         SELECT r.id FROM readings r WHERE r.book_id = books.id
          ORDER BY (r.finished_at IS NULL) DESC,
                   COALESCE(r.started_at, r.created_at) DESC, r.id DESC
          LIMIT 1)";

pub(super) fn row_to_book(row: &SqliteRow) -> Result<Book> {
    let authors: String = row.try_get("authors")?;
    let translators: String = row.try_get("translators")?;
    let subjects: String = row.try_get("subjects")?;
    let created: i64 = row.try_get("created_at")?;
    let modified: i64 = row.try_get("last_modified")?;
    Ok(Book {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        sort_title: row.try_get("sort_title")?,
        authors: serde_json::from_str(&authors).unwrap_or_default(),
        translators: serde_json::from_str(&translators).unwrap_or_default(),
        publisher: row.try_get("publisher")?,
        publish_year: row.try_get("publish_year")?,
        language: row.try_get("language")?,
        isbn_10: row.try_get("isbn_10")?,
        isbn_13: row.try_get("isbn_13")?,
        openlibrary_key: row.try_get("openlibrary_key")?,
        googlebooks_id: row.try_get("googlebooks_id")?,
        cover_url: row.try_get("cover_url")?,
        cover_path: row.try_get("cover_path")?,
        page_count: row.try_get("page_count")?,
        description: row.try_get("description")?,
        first_sentence: row.try_get("first_sentence")?,
        subjects: serde_json::from_str(&subjects).unwrap_or_default(),
        series: row.try_get("series")?,
        series_index: row.try_get("series_index")?,
        current_page: row.try_get("current_page")?,
        finished: row.try_get::<i64, _>("finished")? != 0,
        reading_status: row.try_get("reading_status")?,
        ko_percent: row.try_get("ko_percent")?,
        date_started: row.try_get("date_started")?,
        date_finished: row.try_get("date_finished")?,
        created_at: OffsetDateTime::from_unix_timestamp(created).ok(),
        last_modified: OffsetDateTime::from_unix_timestamp(modified).ok(),
    })
}

/// How one column merges when a **partial** record arrives.
///
/// The three shapes are not stylistic. `title` and the two JSON list columns are
/// NOT NULL with a sentinel empty value (`''`, `'[]'`), so `COALESCE` would
/// happily overwrite a real title with an empty string; the rest are nullable
/// and `NULL` genuinely means "this record does not say".
#[derive(Clone, Copy)]
enum Merge {
    /// NOT NULL text: keep what is there unless the new value is non-empty.
    NonEmptyText,
    /// NOT NULL JSON array: keep what is there unless the new value is not `[]`.
    NonEmptyList,
    /// Nullable: `COALESCE`, so a missing field never clobbers a known one.
    Coalesce,
}

/// Copy one column across, `src` → `dst`, unconditionally. The *whether* is
/// [`Federated`]'s; this is only the *how*.
type Take = fn(&mut Book, &Book);

/// How the **federated search** merges this column, before anything is stored.
///
/// The fourth consumer of this table, and until now the one that was not
/// generated from it: `search::merge_into` spelled the column list a second
/// time, so a column added here compiled cleanly and was **silently dropped
/// from every search result**. Only `every_claimed_field_is_a_merge_column`
/// noticed, and it noticed item 32 doing exactly that.
///
/// It could not simply reuse [`Merge`], which is a different question asked at
/// a different layer: `Merge` is about a *partial record meeting a stored row*
/// and compiles to SQL, while this is about *two providers meeting each other*
/// in memory, with a per-column preference between them that has no equivalent
/// on the storage side. Two questions, one table, one row each.
///
/// Every variant but [`Federated::Local`] carries its own [`Take`], so a column
/// that is never merged has no setter to leave dead or to call by mistake.
#[derive(Clone, Copy)]
enum Federated {
    /// Ours, never a provider's — a derived sort key, a path on this machine.
    /// There is nothing to merge and nothing to attribute.
    Local,
    /// The first provider to speak owns it; a later one fills only a gap.
    Fill(Take),
    /// This source outranks a value already merged in. OpenLibrary is the
    /// authority on editions (the ISBNs, the page count), Google Books on prose
    /// (the description, the language tag).
    Prefer(Source, Take),
    /// Moves with [`Rule::pair`] and is **never taken on its own** — the rule
    /// `series_index` needs, since an index taken from a record whose series
    /// name lost is how a row ends up saying *Dune #2* about its neighbour.
    WithPair(Take),
}

/// One column's entry in [`MERGE_RULES`].
///
/// `says` is the third thing the table has to answer, and it is not a
/// convenience: provenance is stamped for the fields a record actually
/// *supplied*, which is the same question `merge` answers in SQL — "is there a
/// value here, by this column's own definition of empty". Keeping it beside the
/// rule is what stops the two definitions of empty drifting apart, which for
/// `authors` (`[]`) and `title` (`''`) is a real risk and not a hypothetical.
struct Rule {
    col: &'static str,
    merge: Merge,
    /// Does this record say anything about this column?
    says: fn(&Book) -> bool,
    /// What this record says about this column, as text a report can print.
    ///
    /// The fifth thing the table generates, and it is here for the same reason
    /// `says` is. Item 30's `EnrichReport` has to name the value a provider
    /// *offered* for a field it was not allowed to write — a held-back field
    /// reported by name alone is exactly the "silently not updated" state the
    /// report exists to remove — and a `Book`-to-text projection written beside
    /// the table rather than in it is the second column list all of this is
    /// arranged to make unrepresentable.
    ///
    /// **`Some` exactly when `says` is true**, asserted by
    /// `show_and_says_agree_on_every_column`: they are two readings of one
    /// question, and a column where they disagree is a field that is claimed
    /// and unprintable or printable and unclaimed.
    show: fn(&Book) -> Option<String>,
    /// The column this one is only meaningful *beside*, if any — and therefore
    /// the column whose user-ownership also protects this one.
    ///
    /// **Per-field provenance cannot protect a field pair**, which item 30 hit
    /// for real: a user-owned `isbn_13` was held while the unowned `isbn_10`
    /// landed from a *different edition*, leaving one row carrying two ISBNs of
    /// two books. `series`/`series_index` has exactly that shape — owning
    /// "Dune" without owning "#2" is not a coherent state, and a provider that
    /// knows a different series would write its index under the user's name.
    ///
    /// The fix is a guard, not a new kind of claim: **owning either half holds
    /// both**. That is strictly weaker than what the ISBN case was said to need
    /// (a claim on an *edition*, i.e. a claim with no value) and it is enough
    /// for both instances, because in both the incoherence is only ever
    /// introduced by a write to the unowned half. It costs the user nothing
    /// they can't undo: `Engine::set_book_fields` writes both halves and the
    /// user outranks the guard.
    ///
    /// Symmetric by construction — `pairs_are_symmetric` asserts that a
    /// column's pair names it back, since a one-way pairing protects one
    /// direction of exactly the write it exists to stop.
    pair: Option<&'static str>,
    /// How the federated search merges this column, and — where it merges it at
    /// all — how to copy it. The sixth thing the table generates, and the one
    /// that closes the last hand-written column list. See [`Federated`].
    federated: Federated,
}

/// **The provider no-clobber merge, defined once.**
///
/// Four statements need it — `ON CONFLICT DO UPDATE` in
/// [`Storage::upsert_book`], a plain `UPDATE … WHERE id = ?` in
/// [`Storage::enrich_book`], the `dst`-wins fill in [`Storage::merge_books`],
/// and the `field_provenance` stamp all three carry — and they must not be able
/// to disagree about what "merge a partial record" means. That is the same rule
/// `DEVICE_FIELDS_DIFFER` and `identity_hash_of` follow: one formula, every side
/// through it. A field name that exists in one list and not another is the bug
/// this arrangement makes unrepresentable, and it is the bug that would
/// otherwise arrive with item 32, which adds three more columns.
///
/// It is emphatically **not** the device merge. A sidecar is the device's
/// complete state, so a missing note there means the user deleted it and
/// straight assignment is correct. A provider record — and a `calibredb list`
/// row, which carries no page count at all — is partial, and missing means
/// "don't know". `docs/decisions.md`: do not copy one pattern to the other.
const MERGE_RULES: [Rule; 19] = [
    Rule {
        col: "title",
        merge: Merge::NonEmptyText,
        says: |b| b.title.as_deref().is_some_and(|t| !t.is_empty()),
        show: |b| b.title.clone().filter(|t| !t.is_empty()),
        pair: None,
        federated: Federated::Fill(|d, s| d.title = s.title.clone()),
    },
    Rule {
        col: "sort_title",
        merge: Merge::Coalesce,
        says: |b| b.sort_title.is_some(),
        show: |b| b.sort_title.clone(),
        pair: None,
        // Ours: derived from the title for shelf ordering. No provider
        // publishes one, so there is nothing to merge or attribute.
        federated: Federated::Local,
    },
    Rule {
        col: "authors",
        merge: Merge::NonEmptyList,
        says: |b| !b.authors.is_empty(),
        show: |b| (!b.authors.is_empty()).then(|| b.authors.join(", ")),
        pair: None,
        federated: Federated::Fill(|d, s| d.authors = s.authors.clone()),
    },
    Rule {
        col: "translators",
        merge: Merge::NonEmptyList,
        says: |b| !b.translators.is_empty(),
        show: |b| (!b.translators.is_empty()).then(|| b.translators.join(", ")),
        pair: None,
        federated: Federated::Fill(|d, s| d.translators = s.translators.clone()),
    },
    Rule {
        col: "publisher",
        merge: Merge::Coalesce,
        says: |b| b.publisher.is_some(),
        show: |b| b.publisher.clone(),
        pair: None,
        federated: Federated::Fill(|d, s| d.publisher = s.publisher.clone()),
    },
    Rule {
        col: "publish_year",
        merge: Merge::Coalesce,
        says: |b| b.publish_year.is_some(),
        show: |b| b.publish_year.map(|y| y.to_string()),
        pair: None,
        federated: Federated::Fill(|d, s| d.publish_year = s.publish_year),
    },
    Rule {
        col: "language",
        merge: Merge::Coalesce,
        says: |b| b.language.is_some(),
        show: |b| b.language.clone(),
        pair: None,
        // Google publishes a BCP-47 tag; OpenLibrary a MARC name, often
        // absent. Prose is Google's.
        federated: Federated::Prefer(Source::GoogleBooks, |d, s| d.language = s.language.clone()),
    },
    Rule {
        col: "isbn_10",
        merge: Merge::Coalesce,
        says: |b| b.isbn_10.is_some(),
        show: |b| b.isbn_10.clone(),
        // Two ISBNs of two editions is the row item 30 recorded and could not
        // patch. It is the same shape as `series`, so it takes the same guard.
        pair: Some("isbn_13"),
        // Editions are OpenLibrary's — Google's volume records routinely
        // carry another printing's ISBN.
        federated: Federated::Prefer(Source::OpenLibrary, |d, s| d.isbn_10 = s.isbn_10.clone()),
    },
    Rule {
        col: "isbn_13",
        merge: Merge::Coalesce,
        says: |b| b.isbn_13.is_some(),
        show: |b| b.isbn_13.clone(),
        pair: Some("isbn_10"),
        federated: Federated::Prefer(Source::OpenLibrary, |d, s| d.isbn_13 = s.isbn_13.clone()),
    },
    Rule {
        col: "openlibrary_key",
        merge: Merge::Coalesce,
        says: |b| b.openlibrary_key.is_some(),
        show: |b| b.openlibrary_key.clone(),
        pair: None,
        federated: Federated::Fill(|d, s| d.openlibrary_key = s.openlibrary_key.clone()),
    },
    Rule {
        col: "googlebooks_id",
        merge: Merge::Coalesce,
        says: |b| b.googlebooks_id.is_some(),
        show: |b| b.googlebooks_id.clone(),
        pair: None,
        federated: Federated::Fill(|d, s| d.googlebooks_id = s.googlebooks_id.clone()),
    },
    Rule {
        col: "cover_url",
        merge: Merge::Coalesce,
        says: |b| b.cover_url.is_some(),
        show: |b| b.cover_url.clone(),
        pair: None,
        federated: Federated::Fill(|d, s| d.cover_url = s.cover_url.clone()),
    },
    Rule {
        col: "cover_path",
        merge: Merge::Coalesce,
        says: |b| b.cover_path.is_some(),
        show: |b| b.cover_path.clone(),
        pair: None,
        // Ours: where the bytes landed on this machine, written by
        // `download_cover` long after any merge.
        federated: Federated::Local,
    },
    Rule {
        col: "page_count",
        merge: Merge::Coalesce,
        says: |b| b.page_count.is_some(),
        show: |b| b.page_count.map(|n| n.to_string()),
        pair: None,
        federated: Federated::Prefer(Source::OpenLibrary, |d, s| d.page_count = s.page_count),
    },
    Rule {
        col: "description",
        merge: Merge::Coalesce,
        says: |b| b.description.is_some(),
        show: |b| b.description.clone(),
        pair: None,
        federated: Federated::Prefer(Source::GoogleBooks, |d, s| {
            d.description = s.description.clone()
        }),
    },
    Rule {
        col: "first_sentence",
        merge: Merge::Coalesce,
        says: |b| b.first_sentence.is_some(),
        show: |b| b.first_sentence.clone(),
        pair: None,
        federated: Federated::Fill(|d, s| d.first_sentence = s.first_sentence.clone()),
    },
    // ---- migration 0013 ----------------------------------------------------
    Rule {
        // A **set**, and `MERGE_RULES` had no vocabulary for one — so it got the
        // vocabulary `authors` already had rather than a new one. `NonEmptyList`
        // means a provider with subjects replaces the stored set whole, and a
        // provider without them says nothing: `[]` is "this record does not
        // say", never "this book has no subjects".
        //
        // **Not a union**, and that is the decision this column forced. A union
        // reads as generous and is dishonest twice over: Google's categories are
        // BISAC paths and OpenLibrary's are free-text headings, so the result is
        // a two-dialect pile no source vouches for; and `field_provenance` holds
        // exactly one `source` per field, so a unioned value would have to name
        // one of two origins. The set that survives is the set one provider
        // actually published.
        col: "subjects",
        merge: Merge::NonEmptyList,
        says: |b| !b.subjects.is_empty(),
        show: |b| (!b.subjects.is_empty()).then(|| b.subjects.join(", ")),
        pair: None,
        // Filled whole or not at all, never unioned — the argument is on
        // `merge` above and applies identically here.
        federated: Federated::Fill(|d, s| d.subjects = s.subjects.clone()),
    },
    Rule {
        col: "series",
        merge: Merge::Coalesce,
        says: |b| b.series.is_some(),
        show: |b| b.series.clone(),
        pair: Some("series_index"),
        federated: Federated::Fill(|d, s| d.series = s.series.clone()),
    },
    Rule {
        // Fractional, because novellas are 0.5. `show` prints it the way a
        // series index is written rather than as a float: `Dune #2`, not `2`.
        col: "series_index",
        merge: Merge::Coalesce,
        says: |b| b.series_index.is_some(),
        show: |b| b.series_index.map(|n| format!("#{}", series_index_text(n))),
        pair: Some("series"),
        // Comes from whoever supplied the name, **including when that
        // provider had no index at all** — so the assignment is
        // unconditional and only a value present is claimed.
        federated: Federated::WithPair(|d, s| d.series_index = s.series_index),
    },
];

/// Every column the merge table governs, in its own order.
///
/// `pub(crate)` so that a caller assembling per-field attribution — item 30's
/// enrichment, which merges provider by provider — names columns this table
/// knows about rather than strings that merely look like column names.
pub(crate) fn merge_columns() -> impl Iterator<Item = &'static str> {
    MERGE_RULES.iter().map(|r| r.col)
}

/// What a record says about one column, as printable text.
///
/// `None` both when the column is not in [`MERGE_RULES`] and when the record is
/// silent about it — a caller asking for a field's value has no use for the
/// difference, and every caller here has already been through `merge_columns`.
pub(crate) fn field_value(book: &Book, col: &str) -> Option<String> {
    MERGE_RULES
        .iter()
        .find(|r| r.col == col)
        .and_then(|r| (r.show)(book))
}

/// Merge one provider's record into a partially-merged one, and name the
/// columns it actually supplied.
///
/// **This is `search::merge_into`'s body, generated from [`MERGE_RULES`].** It
/// lives here rather than in `search.rs` because the alternative is what was
/// there before: a hand-written column list, which is the fourth spelling of
/// this table and the one place a new column could be added, compile, pass
/// clippy and then be silently dropped from every search result. Now a new
/// `Rule` does not compile without saying how it federates.
///
/// It writes nothing and stamps nothing — at this point in a search there is no
/// row yet. The returned columns are the attribution, in table order, for the
/// caller to carry (`search::FieldClaims`); `save_book` then stamps `None`,
/// because by the time a merged record reaches it the map has been discarded.
///
/// `provider` is a [`Source`] rather than a `ProviderId` so that the preference
/// table can name calibre or the epub reader the day either one merges here;
/// `ProviderId` already converts.
pub(crate) fn merge_provider_record(
    dst: &mut Book,
    src: &Book,
    provider: Source,
) -> Vec<&'static str> {
    let mut claimed: Vec<&'static str> = Vec::new();
    for rule in MERGE_RULES.iter() {
        // `WithPair` columns are taken below, beside the column they belong to.
        // `Local` ones are never a provider's to supply.
        let (take, outranks) = match rule.federated {
            Federated::Local | Federated::WithPair(_) => continue,
            Federated::Fill(take) => (take, false),
            Federated::Prefer(wins, take) => (take, wins == provider),
        };
        if !(rule.says)(src) || (!outranks && (rule.says)(dst)) {
            continue;
        }
        take(dst, src);
        claimed.push(rule.col);

        // The pair moves with it — see [`Federated::WithPair`]. Assigned
        // unconditionally (a provider that named the series and no index clears
        // a stale index that belonged to a different series) but claimed only
        // where there is a value, since a source is not the origin of a NULL.
        if let Some(pair) = MERGE_RULES.iter().find(|r| Some(r.col) == rule.pair)
            && let Federated::WithPair(take_pair) = pair.federated
        {
            take_pair(dst, src);
            if (pair.says)(src) {
                claimed.push(pair.col);
            }
        }
    }
    claimed
}

/// The column whose user-ownership also protects `col` — see [`Rule::pair`].
///
/// `pub(crate)` for `enrich.rs`, whose *report* has to ask the same question the
/// merge clause asks: a field held back because the user owns its partner is a
/// field silently not updated, which is the state `EnrichReport::held` exists to
/// remove. Asked of the table rather than re-derived, for the reason every other
/// consumer is.
pub(crate) fn field_pair(col: &str) -> Option<&'static str> {
    MERGE_RULES
        .iter()
        .find(|r| r.col == col)
        .and_then(|r| r.pair)
}

/// Which side of a merge wins where both have something to say.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Winner {
    /// The record being merged in. `upsert_book` and `enrich_book`: the new
    /// data is the authority and the stored row only survives where the record
    /// is silent.
    Incoming,
    /// The stored row. `merge_books`: `dst` is the row the user chose to keep,
    /// so `src` fills gaps and nothing else.
    Stored,
}

/// The SET clause for [`MERGE_RULES`]. `src` names where the incoming value
/// comes from, per column: `excluded.title` inside an upsert, `?1` inside an
/// update. `last_modified` is appended by the caller, since not every statement
/// binds it positionally.
///
/// `guard_user` wraps every column in the one rule that makes this item worth
/// building: **a field the user owns is not overwritten by a merge.** It lives
/// here, in the generated SQL, rather than in each caller — a caller-side check
/// is a rule that has to be remembered once per writer, and item 30 is a new
/// writer. `merge_books` turns it off, and that is not an exception to the rule:
/// `dst` wins there anyway, so a `user` field is only ever *filled* when `dst`
/// has no value for the user to have corrected.
fn merge_set(winner: Winner, guard_user: bool, src: impl Fn(usize, &str) -> String) -> String {
    MERGE_RULES
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            let col = rule.col;
            let new = src(i, col);
            let stored = format!("books.{col}");
            let (a, b) = match winner {
                Winner::Incoming => (&new, &stored),
                Winner::Stored => (&stored, &new),
            };
            let value = match rule.merge {
                Merge::NonEmptyText => format!("CASE WHEN {a} != '' THEN {a} ELSE {b} END"),
                Merge::NonEmptyList => format!("CASE WHEN {a} != '[]' THEN {a} ELSE {b} END"),
                Merge::Coalesce => format!("COALESCE({a}, {b})"),
            };
            if guard_user {
                // A paired column is guarded by *either* half's claim — see
                // `Rule::pair`. Without it, owning one half of a pair invites
                // the other half to be filled from a record about something
                // else, which is a row that contradicts itself and that nothing
                // downstream can tell from a correct one.
                let owned = match rule.pair {
                    Some(other) => format!(
                        "({} OR {})",
                        field_provenance::user_owns("books.id", col),
                        field_provenance::user_owns("books.id", other)
                    ),
                    None => field_provenance::user_owns("books.id", col),
                };
                format!("{col} = CASE WHEN {owned} THEN {stored} ELSE {value} END")
            } else {
                format!("{col} = {value}")
            }
        })
        .collect::<Vec<_>>()
        .join(",\n            ")
}

/// Does a write from `source` have to respect fields the user owns?
///
/// Everyone except the user, and stated once because "except the user" is
/// exactly the clause that gets remembered in two writers out of three. An
/// unattributed write (`None`) is guarded like any other: not knowing who is
/// writing is not a reason to let them past a correction.
fn guards_user(source: Option<Source>) -> bool {
    source != Some(Source::User)
}

/// The columns this record has something to say about — which is exactly the
/// set that gets a `field_provenance` row, since a source can only be the origin
/// of a value it supplied.
pub(crate) fn fields_said(book: &Book) -> Vec<&'static str> {
    MERGE_RULES
        .iter()
        .filter(|r| (r.says)(book))
        .map(|r| r.col)
        .collect()
}

/// The fields a guarded write is allowed to *claim*, which is not always the
/// set it supplied.
///
/// [`field_provenance::stamp`]'s own `WHERE` refuses to overwrite a `user` claim
/// on the same field, and that covers every unpaired column: value held, claim
/// held, one predicate. A **paired** column is held by the *other* half's claim
/// and has no row of its own, so the stamp would sail past a value the merge
/// clause had just refused to write — a field claiming a provider that did not
/// supply it, which is the exact half-protected state the guard exists to
/// prevent. Same rule as `merge_set`'s, read off the same `Rule::pair`.
async fn claimable(
    conn: &mut sqlx::SqliteConnection,
    book_id: i64,
    fields: &[&'static str],
    guard_user: bool,
) -> Result<Vec<&'static str>> {
    if !guard_user {
        return Ok(fields.to_vec());
    }
    let mut out = Vec::with_capacity(fields.len());
    for field in fields {
        let held = match MERGE_RULES
            .iter()
            .find(|r| r.col == *field)
            .and_then(|r| r.pair)
        {
            Some(other) => field_provenance::user_owns_now(conn, book_id, other).await?,
            None => false,
        };
        if !held {
            out.push(*field);
        }
    }
    Ok(out)
}

/// The columns `stored` is silent about and `incoming` is not — the only ones a
/// `Winner::Stored` merge actually takes from the incoming record, and therefore
/// the only ones whose attribution may travel with it.
fn gap_fields(stored: &Book, incoming: &Book) -> Vec<&'static str> {
    MERGE_RULES
        .iter()
        .filter(|r| !(r.says)(stored) && (r.says)(incoming))
        .map(|r| r.col)
        .collect()
}

/// Bind the [`MERGE_RULES`] columns, in order, to a query.
///
/// Shared for the same reason the clause is: the SQL and the binds are one
/// thing, and a column added to the list without a bind beside it is a runtime
/// error in a statement nobody reads.
fn bind_merge_columns<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    book: &Book,
) -> Result<sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>> {
    Ok(q.bind(book.title.clone().unwrap_or_default())
        .bind(book.sort_title.clone())
        .bind(serde_json::to_string(&book.authors)?)
        .bind(serde_json::to_string(&book.translators)?)
        .bind(book.publisher.clone())
        .bind(book.publish_year)
        .bind(book.language.clone())
        .bind(book.isbn_10.clone())
        .bind(book.isbn_13.clone())
        .bind(book.openlibrary_key.clone())
        .bind(book.googlebooks_id.clone())
        .bind(book.cover_url.clone())
        .bind(book.cover_path.clone())
        .bind(book.page_count)
        .bind(book.description.clone())
        .bind(book.first_sentence.clone())
        .bind(serde_json::to_string(&book.subjects)?)
        .bind(book.series.clone())
        .bind(book.series_index))
}

impl Storage {
    /// Insert-or-merge keyed on isbn_10, else isbn_13, else plain insert.
    /// COALESCE(excluded.x, books.x) keeps NULL fields from clobbering
    /// existing data. Returns the row id.
    ///
    /// **Reading state is not written here, and `Book`'s four progress fields
    /// are ignored.** They are projections of the current reading
    /// (`BOOK_COLUMNS`); writing them belongs to
    /// [`Storage::update_progress`]. This is also why the old
    /// `finished = MAX(excluded.finished, books.finished)` clause is gone rather
    /// than moved: it only ever existed to stop a metadata refresh from
    /// un-finishing a book, and a provider upsert now has no reach into reading
    /// state at all. Everything else keeps its `COALESCE` no-clobber — that
    /// pattern is still right for providers, which return partial records.
    ///
    /// **`source` is who supplied this record**, and every field the record
    /// speaks to gets a `field_provenance` row stamped with it, in the same
    /// transaction as the write. `None` is the honest answer for a writer that
    /// cannot say — `Engine::save_book` holds a record already merged across
    /// providers by `search::merge_provider_books`, which keeps the winning
    /// value and discards which provider supplied it — and it stamps nothing
    /// rather than picking one. It is a parameter and not a default precisely so
    /// that every writer has to answer; a writer that does not stamp is a field
    /// that goes on silently claiming whatever it claimed last.
    pub async fn upsert_book(&self, book: &Book, source: Option<Source>) -> Result<i64> {
        let set_clause = format!(
            "{},\n            last_modified = excluded.last_modified",
            merge_set(Winner::Incoming, guards_user(source), |_, col| format!(
                "excluded.{col}"
            ))
        );

        let insert = r#"INSERT INTO books (
                title, sort_title, authors, translators, publisher, publish_year, language,
                isbn_10, isbn_13, openlibrary_key, googlebooks_id, cover_url, cover_path,
                page_count, description, first_sentence, subjects, series, series_index,
                created_at, last_modified
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#;

        let sql = if book.isbn_10.is_some() {
            format!("{insert} ON CONFLICT(isbn_10) DO UPDATE SET {set_clause} RETURNING id")
        } else if book.isbn_13.is_some() {
            format!("{insert} ON CONFLICT(isbn_13) DO UPDATE SET {set_clause} RETURNING id")
        } else {
            format!("{insert} RETURNING id")
        };

        let now = now_unix();
        // One transaction, for the same reason `merge_books` is one: a row whose
        // `page_count` landed and whose provenance did not is indistinguishable
        // from history, and it is indistinguishable *forever* — nothing later
        // re-derives where a field came from.
        let mut tx = self.pool().begin().await?;
        let row = bind_merge_columns(sqlx::query(&sql), book)?
            .bind(now)
            .bind(now)
            .fetch_one(&mut *tx)
            .await?;
        let id: i64 = row.try_get("id")?;
        if let Some(source) = source {
            let fields =
                claimable(&mut tx, id, &fields_said(book), guards_user(Some(source))).await?;
            field_provenance::stamp(&mut tx, id, &fields, source).await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    /// Merge a partial record into a book **we have already identified**.
    ///
    /// [`Storage::upsert_book`] cannot do this job, and the way it fails is
    /// silent: its third branch — no `isbn_10`, no `isbn_13` — is a *plain
    /// unconditional insert*, so handing it a `Book` whose `id` is set creates a
    /// second row rather than updating the first. That is the same trap
    /// `import_book_from_sidecar` had to key on `device_books` to avoid, and it
    /// is exactly the shape a calibre import meets: a book matched by uuid or by
    /// file hash, carrying no ISBN at all.
    ///
    /// Same rules as the upsert, from [`MERGE_RULES`] — the caller has a partial
    /// record either way, and which statement runs is about *how the book was
    /// found*, never about what a missing field means.
    ///
    /// Same `source` contract as [`Storage::upsert_book`] — and this is also the
    /// door a user edit comes through: `Some(Source::User)` both writes the
    /// value and claims the field, after which no provider merge can overwrite
    /// it. There is no user-edit path in the engine yet, so nothing but a test
    /// passes that today; the rule is built now because item 30's first run is
    /// where its absence would be discovered, by silently overwriting a hand
    /// correction.
    pub async fn enrich_book(
        &self,
        book_id: i64,
        book: &Book,
        source: Option<Source>,
    ) -> Result<()> {
        let claims: Vec<(&'static str, Source)> = match source {
            Some(source) => fields_said(book).into_iter().map(|f| (f, source)).collect(),
            // Not "claims nothing by accident": `None` is a writer that cannot
            // honestly name a source, and an empty claim list is exactly that.
            None => Vec::new(),
        };
        self.enrich_book_attributed(book_id, book, &claims).await
    }

    /// [`Storage::enrich_book`] where the record's fields came from **different
    /// sources**, each named.
    ///
    /// This is the write item 29 left open. `Engine::save_book` stamps `None`
    /// because `search::merge_provider_books` keeps the winning value per field
    /// and discards which provider supplied it; item 30 merges provider by
    /// provider, so it holds that map and can answer honestly. Rather than one
    /// `UPDATE` per provider — which would make whichever provider ran first win
    /// every field it spoke to, a *second* dialect of the federated merge's
    /// tuned per-field preference — the merged record is written once and the
    /// attribution is stamped per field.
    ///
    /// A claim for a field the record does not actually supply is dropped: a
    /// source can only be the origin of a value it gave us, and the same
    /// `fields_said` that decides what the SQL moves decides what may be
    /// claimed.
    ///
    /// **The user guard is on** unless every claim is the user's, which is the
    /// one-source case [`guards_user`] already describes — reached through that
    /// predicate rather than re-spelled, since `merge_set`'s value guard and
    /// `field_provenance::stamp`'s stamp guard have to keep agreeing about which
    /// rows are protected. A mixed set containing `Source::User` guards like any
    /// other, which is the safe direction and is not a set any caller here
    /// produces: a user edit names one source and comes through
    /// [`Storage::enrich_book`].
    pub async fn enrich_book_attributed(
        &self,
        book_id: i64,
        book: &Book,
        claims: &[(&'static str, Source)],
    ) -> Result<()> {
        let guard_user = claims.iter().all(|(_, s)| guards_user(Some(*s)));
        // ?1..?19 are the merge columns, ?20 is last_modified, ?21 the id.
        let sql = format!(
            "UPDATE books SET {}, last_modified = ?20 WHERE id = ?21",
            merge_set(Winner::Incoming, guard_user, |i, _| format!("?{}", i + 1))
        );
        let mut tx = self.pool().begin().await?;
        bind_merge_columns(sqlx::query(&sql), book)?
            .bind(now_unix())
            .bind(book_id)
            .execute(&mut *tx)
            .await?;

        let said = fields_said(book);
        for source in Source::ALL {
            let fields: Vec<&'static str> = claims
                .iter()
                .filter(|(field, s)| *s == source && said.contains(field))
                .map(|(field, _)| *field)
                .collect();
            let fields = claimable(&mut tx, book_id, &fields, guard_user).await?;
            if !fields.is_empty() {
                field_provenance::stamp(&mut tx, book_id, &fields, source).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Merge a **fallback** record: the stored row wins, and this fills only
    /// what it is silent about.
    ///
    /// The mirror of [`Storage::enrich_book`], and the difference is which
    /// record is the authority rather than how a missing field is read. It
    /// exists for `Engine::import_epub`, where the file's own metadata is
    /// deliberately second to whatever a provider said about the same ISBN —
    /// that used to be a hand-written `if book.title.is_none()` chain folding
    /// the two into one `Book` before a single write, which is `MERGE_RULES`
    /// spelled a second time *and* a record with two origins and no way to say
    /// which supplied what. Two writes, two sources, one merge table.
    ///
    /// Provenance is stamped for the fields it actually filled, never for the
    /// ones the stored row already answered — a source is only the origin of a
    /// value that survived.
    pub async fn fill_book(&self, book_id: i64, book: &Book, source: Option<Source>) -> Result<()> {
        // ?1..?19 are the merge columns, ?20 is last_modified, ?21 the id.
        let sql = format!(
            "UPDATE books SET {}, last_modified = ?20 WHERE id = ?21",
            merge_set(Winner::Stored, guards_user(source), |i, _| format!(
                "?{}",
                i + 1
            ))
        );
        let mut tx = self.pool().begin().await?;
        // Read before the write, or the gaps this filled are already closed and
        // it would look as though it had supplied nothing.
        let select = format!("SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.id = ?");
        let stored = sqlx::query(&select)
            .bind(book_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {book_id}")))?;
        let gaps = gap_fields(&row_to_book(&stored)?, book);

        bind_merge_columns(sqlx::query(&sql), book)?
            .bind(now_unix())
            .bind(book_id)
            .execute(&mut *tx)
            .await?;
        if let Some(source) = source {
            let gaps = claimable(&mut tx, book_id, &gaps, guards_user(Some(source))).await?;
            field_provenance::stamp(&mut tx, book_id, &gaps, source).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_book(&self, id: i64) -> Result<Option<Book>> {
        let sql = format!("SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_book).transpose()
    }

    /// Lookup by a normalized ISBN (either column).
    pub async fn find_book_by_isbn(&self, isbn: &str) -> Result<Option<Book>> {
        let sql = format!(
            "SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.isbn_10 = ?1 OR books.isbn_13 = ?1"
        );
        let row = sqlx::query(&sql)
            .bind(isbn)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_book).transpose()
    }

    pub async fn find_books_by_title(&self, fragment: &str) -> Result<Vec<Book>> {
        let sql = format!(
            "SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.title LIKE ?
             ORDER BY books.last_modified DESC"
        );
        let rows = sqlx::query(&sql)
            .bind(format!("%{fragment}%"))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_book).collect()
    }

    /// The first `limit` books by `sort`. See [`BookSort`] for what that means
    /// about membership — it is a contract, not an accident of `LIMIT`.
    pub async fn list_books(&self, limit: i64, sort: BookSort) -> Result<Vec<Book>> {
        let order = match sort {
            BookSort::LastModified => "books.last_modified DESC",
            BookSort::Title => "books.title COLLATE NOCASE ASC",
            // The joined reading's page, not a `books` column any more.
            BookSort::Progress => {
                "CAST(cur.current_page AS REAL) / NULLIF(books.page_count, 0) DESC NULLS LAST"
            }
            // Undated last, so the `NULL` arm must sit outside the reversal.
            BookSort::Year => "books.publish_year DESC NULLS LAST",
            // SQL cannot answer this one. Read, sort, truncate — see the
            // variant's own note for why that is the whole design and not a
            // stopgap hiding a missing column.
            BookSort::Author => return self.list_books_by_author(limit).await,
        };
        let sql = format!("SELECT {BOOK_COLUMNS} {BOOK_FROM} ORDER BY {order} LIMIT ?");
        let rows = sqlx::query(&sql).bind(limit).fetch_all(self.pool()).await?;
        rows.iter().map(row_to_book).collect()
    }

    /// [`BookSort::Author`]'s arm: the whole library, sorted in Rust, truncated.
    ///
    /// The base order is recency and it is load-bearing rather than arbitrary:
    /// `sort_by_key` is **stable**, so two books by one author keep the order
    /// the library screen would otherwise have shown them in. That is a better
    /// tie-break than any second key, and it costs nothing.
    async fn list_books_by_author(&self, limit: i64) -> Result<Vec<Book>> {
        let sql = format!("SELECT {BOOK_COLUMNS} {BOOK_FROM} ORDER BY books.last_modified DESC");
        let rows = sqlx::query(&sql).fetch_all(self.pool()).await?;
        let mut books: Vec<Book> = rows.iter().map(row_to_book).collect::<Result<_>>()?;
        books.sort_by_key(|b| crate::names::sort_key(&b.authors));
        books.truncate(limit.max(0) as usize);
        Ok(books)
    }

    /// Fold `src` into `dst`: move highlights, notes, flashcards, readings,
    /// device links and owned files, fill `dst`'s empty fields from `src`,
    /// delete `src`.
    ///
    /// This exists because the ISBN-less insert path guarantees duplicates
    /// regardless of how good matching gets: `upsert_book` branches
    /// isbn_10 → isbn_13 → *plain insert*, and a book created from a sidecar
    /// has neither ISBN. Matching can only shrink the rate, never reach zero,
    /// so the library needs a way to put two rows back together.
    ///
    /// **One transaction.** A half-merged library — highlights moved, notes
    /// not — is worse than the duplicate it was trying to fix, and nothing
    /// would tell the user it had happened.
    ///
    /// **The identity collision is handled, not avoided.** `book_id` is an
    /// input to a highlight's `identity_hash`, so every moved row's hash has to
    /// be recomputed against `dst`; one that then collides with a highlight
    /// already there is the *same annotation*, and is dropped rather than
    /// duplicated. Its note anchors and flashcards are repointed at the
    /// survivor first — dropping them would quietly undo exactly the guarantee
    /// migration `0004` went to the trouble of keeping.
    ///
    /// Field merge is `COALESCE`-style with **`dst` winning**: `dst` is the row
    /// the user chose to keep, so `src` only fills what `dst` does not have.
    ///
    /// Idempotent: a repeat merge finds `src` already gone and returns a report
    /// with `src_existed == false` and every count zero. A missing `dst` is a
    /// real error — there is nothing to merge *into*.
    pub async fn merge_books(&self, src: i64, dst: i64) -> Result<MergeReport> {
        let mut report = MergeReport::default();
        if src == dst {
            // Not an error: "make these two one" is already true. Proceeding
            // would delete the book.
            report.src_existed = self.get_book(src).await?.is_some();
            return Ok(report);
        }

        // One connection for the whole merge. Nothing below may call back
        // through `self` — an in-memory Storage caps the pool at one
        // connection, so a nested acquire would deadlock rather than fail.
        let mut tx = self.pool().begin().await?;

        // The whole row, not just its cover: which fields `dst` is silent about
        // is what decides both the fill below and whose provenance travels with
        // it, and that has to be read *before* the fill changes the answer.
        let sql = format!("SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.id = ?");
        let dst_row = sqlx::query(&sql).bind(dst).fetch_optional(&mut *tx).await?;
        let Some(dst_row) = dst_row else {
            return Err(EngineError::NotFound(format!("book id {dst}")));
        };
        let dst_book = row_to_book(&dst_row)?;

        let Some(src_row) = sqlx::query(&sql).bind(src).fetch_optional(&mut *tx).await? else {
            // Already merged (or never existed). Returning an empty report
            // rather than an error is what makes merging twice equal merging
            // once, which the caller — a retry, a re-run, a device screen
            // firing the same action — has no other way to be sure of.
            return Ok(report);
        };
        let src_book = row_to_book(&src_row)?;
        report.src_existed = true;

        // ---- highlights: recompute identity against dst, drop collisions ----
        let rows =
            sqlx::query("SELECT id, text, pos0, ko_datetime FROM highlights WHERE book_id = ?")
                .bind(src)
                .fetch_all(&mut *tx)
                .await?;
        for r in &rows {
            let id: i64 = r.try_get("id")?;
            let text: String = r.try_get("text")?;
            let pos0: Option<String> = r.try_get("pos0")?;
            let ko_datetime: Option<String> = r.try_get("ko_datetime")?;
            let hash = identity_hash_of(dst, ko_datetime.as_deref(), pos0.as_deref(), &text);

            let survivor: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM highlights WHERE book_id = ? AND identity_hash = ?",
            )
            .bind(dst)
            .bind(&hash)
            .fetch_optional(&mut *tx)
            .await?;
            match survivor {
                Some(keep) => {
                    // The same annotation under two book ids. Move what hangs
                    // off the copy we are about to drop before dropping it:
                    // `flashcards.highlight_id` cascades on delete and
                    // `notes.highlight_id` nulls, so doing this in the other
                    // order loses note anchors silently.
                    sqlx::query("UPDATE notes SET highlight_id = ? WHERE highlight_id = ?")
                        .bind(keep)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("UPDATE flashcards SET highlight_id = ? WHERE highlight_id = ?")
                        .bind(keep)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("DELETE FROM highlights WHERE id = ?")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    report.highlights_dropped += 1;
                }
                None => {
                    sqlx::query(
                        "UPDATE highlights SET book_id = ?, identity_hash = ? WHERE id = ?",
                    )
                    .bind(dst)
                    .bind(&hash)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    report.highlights_moved += 1;
                }
            }
        }

        // ---- notes -------------------------------------------------------
        report.notes_moved = sqlx::query("UPDATE notes SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // ---- flashcards: UNIQUE(book_id, word) forbids a straight move ----
        report.flashcards_dropped = sqlx::query(
            "DELETE FROM flashcards WHERE book_id = ?1
             AND word IN (SELECT word FROM flashcards WHERE book_id = ?2)",
        )
        .bind(src)
        .bind(dst)
        .execute(&mut *tx)
        .await?
        .rows_affected() as usize;
        report.flashcards_moved = sqlx::query("UPDATE flashcards SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // ---- readings ------------------------------------------------------
        // Two open readings would violate `idx_readings_one_open`, so the older
        // one is closed first — `abandoned`, at its own `last_modified`, because
        // deleting it would lose a reading that really happened. This runs
        // before the move (and before `src` is deleted, which would cascade its
        // readings away entirely).
        sqlx::query(
            r#"UPDATE readings SET finished_at = last_modified, status = 'abandoned'
               WHERE id = (
                   SELECT id FROM readings
                    WHERE book_id IN (?1, ?2) AND finished_at IS NULL
                    ORDER BY COALESCE(started_at, created_at) ASC, id ASC
                    LIMIT 1)
                 AND (SELECT count(*) FROM readings
                       WHERE book_id IN (?1, ?2) AND finished_at IS NULL) > 1"#,
        )
        .bind(src)
        .bind(dst)
        .execute(&mut *tx)
        .await?;
        report.readings_moved = sqlx::query("UPDATE readings SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // ---- device links --------------------------------------------------
        report.device_links_moved =
            sqlx::query("UPDATE device_books SET book_id = ? WHERE book_id = ?")
                .bind(dst)
                .bind(src)
                .execute(&mut *tx)
                .await?
                .rows_affected() as usize;

        // ---- owned files ---------------------------------------------------
        // Same reason as the provenance below: `book_files` cascades on `books`,
        // so a merge that left it alone would *delete* the rows with `src` — and
        // the bytes they name would stay on disk, owned by nothing, findable by
        // nothing, with the book that was folded in showing no files at all.
        //
        // A plain `UPDATE`, unlike `book_tags`: the primary key is the sha256,
        // so `src` and `dst` cannot both hold one file and there is nothing to
        // ignore.
        report.files_moved = sqlx::query("UPDATE book_files SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // ---- provenance ----------------------------------------------------
        // `external_ids` and `book_tags` both cascade on `books`, so a merge
        // that left them alone would *delete* them with `src` — and losing
        // `external_ids` is the expensive one: the next Goodreads import would
        // no longer recognise the row and would recreate the duplicate this
        // merge just folded in.
        //
        // `UPDATE OR IGNORE` for the tags, because their primary key is
        // `(book_id, tag, source)` and both books being on the same shelf is
        // ordinary; the losers are dropped by the cascade a moment later.
        sqlx::query("UPDATE external_ids SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE OR IGNORE book_tags SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?;

        // Field provenance cascades too, and moving it wholesale would be worse
        // than losing it: the fill below is `dst`-wins, so `src`'s claim on a
        // field `dst` already has a value for would attribute `dst`'s value to
        // the source of a value that was thrown away. Only the gaps travel.
        field_provenance::move_claims(&mut tx, src, dst, &gap_fields(&dst_book, &src_book)).await?;

        // `src` goes before `dst` is updated: isbn_10 and isbn_13 are UNIQUE, so
        // handing `dst` an ISBN `src` still holds would fail the constraint.
        sqlx::query("DELETE FROM books WHERE id = ?")
            .bind(src)
            .execute(&mut *tx)
            .await?;

        if dst_book.cover_path.is_some() {
            report.orphaned_cover = src_book.cover_path.clone();
        }

        // dst wins: `src` fills only what `dst` does not already have. The
        // inverse of `upsert_book`'s clause, where the incoming record wins —
        // there the new data is the authority, here the kept row is.
        //
        // Generated from `MERGE_RULES` like the other three, and it did not use
        // to be: this was sixteen hand-written columns, i.e. a second list that
        // item 32 would have had to remember to extend, in the one statement
        // where forgetting a column loses data outright rather than merely
        // failing to merge it. The user guard is **off** — see `merge_set`; both
        // books are ours, and `dst` winning already means a corrected field can
        // only ever be filled when there was nothing to correct.
        let fill = format!(
            "UPDATE books SET {}, last_modified = ?20 WHERE id = ?21",
            merge_set(Winner::Stored, false, |i, _| format!("?{}", i + 1))
        );
        bind_merge_columns(sqlx::query(&fill), &src_book)?
            .bind(now_unix())
            .bind(dst)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(report)
    }

    /// Backdate a book's `created_at`.
    ///
    /// Narrow on purpose: it is for an importer that has *just created* a book
    /// and knows when the user really added it — Goodreads' `Date Added`. For a
    /// book we already had, when it arrived here is ours to know and no CSV has
    /// standing to rewrite it, which is why the caller checks and this function
    /// does not.
    pub async fn set_book_created_at(&self, book_id: i64, created_at: i64) -> Result<()> {
        sqlx::query("UPDATE books SET created_at = ? WHERE id = ?")
            .bind(created_at)
            .bind(book_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Delete a book; returns its cover_path (if any) so the caller can
    /// clean up the image file.
    pub async fn delete_book(&self, id: i64) -> Result<Option<String>> {
        let cover: Option<Option<String>> =
            sqlx::query_scalar("DELETE FROM books WHERE id = ? RETURNING cover_path")
                .bind(id)
                .fetch_optional(self.pool())
                .await?;
        match cover {
            None => Err(EngineError::NotFound(format!("book id {id}"))),
            Some(path) => Ok(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Book {
        Book {
            title: Some("Pachinko".into()),
            authors: vec!["Min Jin Lee".into()],
            isbn_13: Some("9781455563937".into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn upsert_merges_without_clobbering() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let mut b = sample();
        b.description = Some("A sweeping saga.".into());
        let id1 = s.upsert_book(&b, None).await.unwrap();

        // Re-upsert same ISBN with missing description but new page count:
        // description must survive, page count must land, id must be stable.
        let mut b2 = sample();
        b2.page_count = Some(490);
        let id2 = s.upsert_book(&b2, None).await.unwrap();
        assert_eq!(id1, id2);

        let got = s.get_book(id1).await.unwrap().unwrap();
        assert_eq!(got.description.as_deref(), Some("A sweeping saga."));
        assert_eq!(got.page_count, Some(490));
        assert_eq!(got.authors, vec!["Min Jin Lee".to_string()]);
    }

    /// The two statements built from [`MERGE_RULES`] must not be able to
    /// disagree about what merging a partial record means.
    ///
    /// The clause is generated once and rendered twice — `excluded.x` inside the
    /// upsert, `?n` inside the update — so this is the assertion that the second
    /// rendering is the same rule and not merely similar SQL. Run against the
    /// same starting row and the same partial update, both must land in exactly
    /// the same state.
    #[tokio::test]
    async fn enrich_merges_a_partial_record_exactly_as_the_upsert_does() {
        let mut full = sample();
        full.description = Some("A sweeping saga.".into());
        full.publisher = Some("Grand Central".into());
        full.page_count = Some(490);

        // Partial: a new publish year, an empty title, no authors, and nothing
        // to say about the three fields already set.
        let partial = Book {
            title: None,
            isbn_13: full.isbn_13.clone(),
            publish_year: Some(2017),
            language: Some("en".into()),
            ..Default::default()
        };

        let (by_upsert, prov_upsert) = {
            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let id = s
                .upsert_book(&full, Some(Source::GoogleBooks))
                .await
                .unwrap();
            // Same ISBN, so this takes the ON CONFLICT branch.
            assert_eq!(
                s.upsert_book(&partial, Some(Source::OpenLibrary))
                    .await
                    .unwrap(),
                id
            );
            (
                s.get_book(id).await.unwrap().unwrap(),
                sources(&s, id).await,
            )
        };
        let (by_enrich, prov_enrich) = {
            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let id = s
                .upsert_book(&full, Some(Source::GoogleBooks))
                .await
                .unwrap();
            s.enrich_book(id, &partial, Some(Source::OpenLibrary))
                .await
                .unwrap();
            (
                s.get_book(id).await.unwrap().unwrap(),
                sources(&s, id).await,
            )
        };

        for (name, a, b) in [
            ("title", by_upsert.title.clone(), by_enrich.title.clone()),
            (
                "publisher",
                by_upsert.publisher.clone(),
                by_enrich.publisher.clone(),
            ),
            (
                "description",
                by_upsert.description.clone(),
                by_enrich.description.clone(),
            ),
            (
                "language",
                by_upsert.language.clone(),
                by_enrich.language.clone(),
            ),
        ] {
            assert_eq!(a, b, "{name} merged differently");
        }
        assert_eq!(by_upsert.authors, by_enrich.authors);
        assert_eq!(by_upsert.page_count, by_enrich.page_count);
        assert_eq!(by_upsert.publish_year, by_enrich.publish_year);
        // …and both actually merged rather than both blanking everything, which
        // is the way a same-vs-same assertion passes for the wrong reason.
        assert_eq!(by_enrich.title.as_deref(), Some("Pachinko"));
        assert_eq!(by_enrich.authors, vec!["Min Jin Lee".to_string()]);
        assert_eq!(by_enrich.page_count, Some(490));
        assert_eq!(by_enrich.publish_year, Some(2017));

        // …and the third consumer of the same table agrees with both. Two
        // statements that merge identically while attributing differently is
        // exactly the drift this arrangement is built to make impossible.
        assert_eq!(prov_upsert, prov_enrich, "provenance stamped differently");
        assert!(
            prov_enrich.contains(&("publish_year".into(), "openlibrary".into())),
            "the field the partial record supplied is claimed by it: {prov_enrich:?}"
        );
        assert!(
            prov_enrich.contains(&("description".into(), "googlebooks".into())),
            "a field the partial record is silent about keeps its claim: {prov_enrich:?}"
        );
    }

    /// The reason `enrich_book` exists at all: `upsert_book`'s third branch is a
    /// plain unconditional insert, so a `Book` with no ISBN cannot be updated
    /// through it however its `id` is set.
    #[tokio::test]
    async fn enrich_updates_an_isbn_less_book_where_upsert_would_duplicate_it() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some("Untitled Draft".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        s.enrich_book(
            id,
            &Book {
                publisher: Some("Self".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(s.list_books(10, BookSort::Title).await.unwrap().len(), 1);
        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.publisher.as_deref(), Some("Self"));
        assert_eq!(got.title.as_deref(), Some("Untitled Draft"));

        // The contrast, so the claim above is observed rather than asserted:
        // the same partial record through `upsert_book` makes a second row.
        s.upsert_book(
            &Book {
                id: Some(id),
                publisher: Some("Self".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(s.list_books(10, BookSort::Title).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn upsert_branches_on_isbn10() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let b = Book {
            title: Some("X".into()),
            isbn_10: Some("0306406152".into()),
            ..Default::default()
        };
        let id1 = s.upsert_book(&b, None).await.unwrap();
        let id2 = s.upsert_book(&b, None).await.unwrap();
        assert_eq!(id1, id2);
        let found = s.find_book_by_isbn("0306406152").await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn progress_and_finish_flow() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&sample(), None).await.unwrap();
        let b = s.update_progress(id, Some(100), None).await.unwrap();
        assert_eq!(b.current_page, Some(100));
        assert!(!b.finished);
        assert!(b.date_started.is_some());

        let b = s.update_progress(id, None, Some(true)).await.unwrap();
        assert!(b.finished);
        assert!(b.date_finished.is_some());
        assert_eq!(b.current_page, Some(100));
    }

    /// The *absence* of the retired `finished = MAX(excluded.finished,
    /// books.finished)` clause, pinned.
    ///
    /// That clause existed only because reading state lived on `books`, and
    /// without a test naming it the behaviour comes straight back the next time
    /// someone "fixes" the upsert to carry a `Book`'s progress fields. A
    /// provider record cannot know what page you are on: the fields are ignored,
    /// in both directions — an incoming `finished: true` must not finish a book,
    /// and an incoming `finished: false` must not un-finish one.
    #[tokio::test]
    async fn a_provider_upsert_never_touches_reading_state() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&sample(), None).await.unwrap();
        s.update_progress(id, Some(100), Some(true)).await.unwrap();
        let before = s.list_readings(id).await.unwrap();

        // A metadata refresh arriving with the opposite of everything.
        let refreshed = Book {
            page_count: Some(490),
            current_page: Some(1),
            finished: false,
            date_started: Some(1),
            date_finished: Some(2),
            ..sample()
        };
        assert_eq!(s.upsert_book(&refreshed, None).await.unwrap(), id);

        let b = s.get_book(id).await.unwrap().unwrap();
        assert!(b.finished, "a provider upsert must not un-finish a book");
        assert_eq!(b.current_page, Some(100));
        assert_eq!(b.page_count, Some(490), "metadata still lands");
        assert_eq!(s.list_readings(id).await.unwrap(), before);

        // And the other direction: it must not *start* a reading either.
        let fresh = s
            .upsert_book(
                &Book {
                    title: Some("Never opened".into()),
                    current_page: Some(42),
                    finished: true,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert!(s.list_readings(fresh).await.unwrap().is_empty());
        let b = s.get_book(fresh).await.unwrap().unwrap();
        assert_eq!(b.current_page, None);
        assert!(!b.finished);
    }

    // ---- merge ------------------------------------------------------------

    use crate::storage::{LinkedBy, NewHighlight, Reading};

    fn hl(text: &str, datetime: &str) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: None,
            page: Some(1),
            pos0: Some(format!("/body/p[1]/text().{}", text.len())),
            pos1: None,
            ko_datetime: Some(datetime.into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    async fn book(s: &Storage, title: &str) -> i64 {
        s.upsert_book(
            &Book {
                title: Some(title.into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap()
    }

    async fn count(s: &Storage, sql: &str, id: i64) -> i64 {
        sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(s.pool())
            .await
            .unwrap()
    }

    /// The whole contract in one pass: everything moves, the duplicate is
    /// dropped rather than doubled, and `src` is gone.
    #[tokio::test]
    async fn merge_moves_everything_and_drops_the_identity_collision() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;

        // Same annotation on both sides. Its `identity_hash` differs today only
        // because `book_id` is one of the hash's inputs — recompute against
        // `dst` and it is plainly the same row.
        let shared = hl(
            "History has failed us, but no matter.",
            "2026-01-05 21:14:08",
        );
        let only_on_src = hl("pachinko", "2026-01-06 08:02:11");
        s.insert_highlight(dst, &shared).await.unwrap();
        let src_shared = s.insert_highlight(src, &shared).await.unwrap().unwrap();
        s.insert_highlight(src, &only_on_src).await.unwrap();

        // Dependents of the copy that is about to be dropped.
        s.insert_flashcard(src, Some(src_shared), "pachinko", None)
            .await
            .unwrap();
        s.link_device_book("abc", src, LinkedBy::Auto)
            .await
            .unwrap();

        let r = s.merge_books(src, dst).await.unwrap();
        assert!(r.src_existed);
        assert_eq!(r.highlights_moved, 1);
        assert_eq!(r.highlights_dropped, 1, "the collision must not duplicate");
        assert_eq!(r.flashcards_moved, 1);
        assert_eq!(r.device_links_moved, 1);

        assert!(s.get_book(src).await.unwrap().is_none(), "src is gone");
        let texts: Vec<String> = s
            .list_highlights(dst)
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.text)
            .collect();
        assert_eq!(texts.len(), 2, "two distinct annotations, got {texts:?}");
        assert_eq!(
            s.find_book_by_partial_md5("abc").await.unwrap().unwrap().id,
            Some(dst)
        );
        assert_eq!(
            count(&s, "SELECT count(*) FROM flashcards WHERE book_id = ?", dst).await,
            1
        );
    }

    /// The dropped copy's note anchors and flashcards have to survive it.
    /// `flashcards.highlight_id` cascades on delete and `notes.highlight_id`
    /// nulls, so deleting first and repointing after would lose both — silently,
    /// and only for books that happened to overlap.
    #[tokio::test]
    async fn a_dropped_duplicate_hands_its_dependents_to_the_survivor() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;

        let shared = hl("a shared passage", "2026-01-05 21:14:08");
        let keep = s.insert_highlight(dst, &shared).await.unwrap().unwrap();
        let doomed = s.insert_highlight(src, &shared).await.unwrap().unwrap();
        s.insert_flashcard(src, Some(doomed), "shared", None)
            .await
            .unwrap();

        s.merge_books(src, dst).await.unwrap();

        let anchored: Option<i64> =
            sqlx::query_scalar("SELECT highlight_id FROM flashcards WHERE word = ?")
                .bind("shared")
                .fetch_one(s.pool())
                .await
                .unwrap();
        assert_eq!(
            anchored,
            Some(keep),
            "the flashcard must follow the survivor"
        );
    }

    /// `flashcards` is `UNIQUE(book_id, word)`, so a word both books captured
    /// cannot simply move. It is dropped, and the merge does not fail.
    #[tokio::test]
    async fn a_word_both_books_captured_is_dropped_not_a_constraint_error() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;
        s.insert_flashcard(src, None, "pachinko", None)
            .await
            .unwrap();
        s.insert_flashcard(dst, None, "pachinko", None)
            .await
            .unwrap();
        s.insert_flashcard(src, None, "hanko", None).await.unwrap();

        let r = s.merge_books(src, dst).await.unwrap();
        assert_eq!(r.flashcards_dropped, 1);
        assert_eq!(r.flashcards_moved, 1);
        assert_eq!(
            count(&s, "SELECT count(*) FROM flashcards WHERE book_id = ?", dst).await,
            2
        );
    }

    /// `dst` is the row the user chose to keep, so `src` fills only its gaps —
    /// the inverse of `upsert_book`, where the incoming record wins. The ISBN is
    /// the one that would blow up if the order were wrong: both columns are
    /// UNIQUE, so `dst` cannot take `src`'s until `src` is gone.
    #[tokio::test]
    async fn dst_wins_and_src_fills_the_gaps() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    isbn_13: Some("9781455563937".into()),
                    description: Some("from the duplicate".into()),
                    page_count: Some(490),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        s.update_progress(src, None, Some(true)).await.unwrap();
        let dst = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko (the keeper)".into()),
                    description: Some("the one to keep".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        s.merge_books(src, dst).await.unwrap();

        let got = s.get_book(dst).await.unwrap().unwrap();
        assert_eq!(got.title.as_deref(), Some("Pachinko (the keeper)"));
        assert_eq!(got.description.as_deref(), Some("the one to keep"));
        assert_eq!(got.isbn_13.as_deref(), Some("9781455563937"), "gap filled");
        assert_eq!(got.page_count, Some(490));
        // Not a field merge any more: `src`'s reading came with it, and the
        // projection reads off that.
        assert!(got.finished, "src's reading survives the fold");
    }

    /// Provenance travels with the **value**, not with the row.
    ///
    /// `field_provenance` cascades on `books`, so a merge that ignored it would
    /// delete `src`'s claims — but moving them wholesale is worse than losing
    /// them, and the third case below is why. `dst` wins the fill, so a field
    /// `dst` already answered keeps `dst`'s claim; only a gap `src` closed hands
    /// its claim over. And a field `dst` holds *unattributed* — every book older
    /// than migration `0012` — must stay unattributed rather than quietly
    /// inheriting the claim attached to a value that was thrown away.
    #[tokio::test]
    async fn a_merge_moves_a_claim_only_where_it_moved_the_value() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko (dupe)".into()),
                    description: Some("from the duplicate".into()),
                    page_count: Some(490),
                    publisher: Some("from the duplicate".into()),
                    ..Default::default()
                },
                Some(Source::Goodreads),
            )
            .await
            .unwrap();
        let dst = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    description: Some("the one to keep".into()),
                    ..Default::default()
                },
                Some(Source::Calibre),
            )
            .await
            .unwrap();
        // A field `dst` has a value for and no claim on: the state every book
        // that predates the migration is in.
        sqlx::query("UPDATE books SET publisher = 'unattributed history' WHERE id = ?")
            .bind(dst)
            .execute(s.pool())
            .await
            .unwrap();

        s.merge_books(src, dst).await.unwrap();

        let got = s.get_book(dst).await.unwrap().unwrap();
        assert_eq!(got.description.as_deref(), Some("the one to keep"));
        assert_eq!(got.page_count, Some(490), "the gap is filled");
        assert_eq!(got.publisher.as_deref(), Some("unattributed history"));

        assert_eq!(
            s.field_source(dst, "description").await.unwrap().as_deref(),
            Some("calibre"),
            "dst kept its value, so it keeps its claim"
        );
        assert_eq!(
            s.field_source(dst, "page_count").await.unwrap().as_deref(),
            Some("goodreads"),
            "src's value arrived, so src's claim arrived with it"
        );
        assert_eq!(
            s.field_source(dst, "publisher").await.unwrap(),
            None,
            "an unattributed value must not inherit the claim of a discarded one"
        );
    }

    /// **Item 30's write, and the reason it is not one `UPDATE` per provider.**
    ///
    /// One record whose fields came from two sources: each is stamped with its
    /// own, in a single statement, so the values are the federated merge's tuned
    /// per-field preference rather than whichever provider happened to run
    /// first.
    #[tokio::test]
    async fn an_attributed_merge_stamps_each_field_with_its_own_source() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&sample(), None).await.unwrap();

        s.enrich_book_attributed(
            id,
            &Book {
                page_count: Some(490),
                description: Some("a sweeping saga".into()),
                publisher: Some("Grand Central".into()),
                ..Default::default()
            },
            &[
                ("page_count", Source::OpenLibrary),
                ("description", Source::GoogleBooks),
                // A claim for a field the record does not supply: dropped, since
                // a source can only be the origin of a value it gave us.
                ("language", Source::OpenLibrary),
            ],
        )
        .await
        .unwrap();

        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.page_count, Some(490));
        assert_eq!(got.description.as_deref(), Some("a sweeping saga"));
        assert_eq!(got.publisher.as_deref(), Some("Grand Central"));

        assert_eq!(
            s.field_source(id, "page_count").await.unwrap().as_deref(),
            Some("openlibrary")
        );
        assert_eq!(
            s.field_source(id, "description").await.unwrap().as_deref(),
            Some("googlebooks")
        );
        assert_eq!(
            s.field_source(id, "language").await.unwrap(),
            None,
            "a claim on a field the record is silent about must not be recorded"
        );
        assert_eq!(
            s.field_source(id, "publisher").await.unwrap(),
            None,
            "a field written with no claim stays unattributed"
        );
    }

    /// The user guard survives the multi-source form. `guards_user` is reached
    /// through the same predicate rather than re-spelled, so a mixed claim set
    /// guards — the safe direction — and a field the user owns is neither
    /// overwritten nor re-claimed.
    #[tokio::test]
    async fn an_attributed_merge_still_cannot_take_a_user_field() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&sample(), None).await.unwrap();
        s.enrich_book(
            id,
            &Book {
                page_count: Some(496),
                ..Default::default()
            },
            Some(Source::User),
        )
        .await
        .unwrap();

        s.enrich_book_attributed(
            id,
            &Book {
                page_count: Some(490),
                description: Some("a sweeping saga".into()),
                ..Default::default()
            },
            &[
                ("page_count", Source::OpenLibrary),
                ("description", Source::OpenLibrary),
            ],
        )
        .await
        .unwrap();

        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.page_count, Some(496));
        assert_eq!(got.description.as_deref(), Some("a sweeping saga"));
        assert_eq!(
            s.field_source(id, "page_count").await.unwrap().as_deref(),
            Some("user")
        );
    }

    /// Both books were being read. That is two open readings on one book the
    /// moment they fold together, which `idx_readings_one_open` forbids — so the
    /// older is closed as `abandoned` rather than deleted, because it is a
    /// reading that really happened.
    #[tokio::test]
    async fn merging_moves_readings_and_leaves_exactly_one_open() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;

        // `src` started first, and finished a reading before this one.
        s.open_reading(src, Some(1_000), "manual").await.unwrap();
        s.finish_reading(src).await.unwrap();
        s.open_reading(src, Some(2_000), "manual").await.unwrap();
        s.open_reading(dst, Some(3_000), "manual").await.unwrap();

        let r = s.merge_books(src, dst).await.unwrap();
        assert_eq!(r.readings_moved, 2);

        let readings = s.list_readings(dst).await.unwrap();
        assert_eq!(readings.len(), 3, "nothing is deleted");
        let open: Vec<&Reading> = readings
            .iter()
            .filter(|r| r.finished_at.is_none())
            .collect();
        assert_eq!(open.len(), 1, "the index invariant survives the merge");
        assert_eq!(open[0].started_at, Some(3_000), "the newer one stays open");

        let closed = readings
            .iter()
            .find(|r| r.started_at == Some(2_000))
            .expect("src's open reading moved");
        assert_eq!(closed.status, crate::storage::STATUS_ABANDONED);
        assert!(closed.finished_at.is_some());
    }

    /// Only one side was being read: nothing has to be closed, and the open
    /// reading must survive as open. The close is conditional, and a
    /// close-unconditionally version passes the test above.
    #[tokio::test]
    async fn merging_one_open_reading_leaves_it_open() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;
        s.open_reading(src, Some(2_000), "manual").await.unwrap();

        s.merge_books(src, dst).await.unwrap();
        let readings = s.list_readings(dst).await.unwrap();
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].finished_at, None);
        assert_eq!(readings[0].status, crate::storage::STATUS_READING);
    }

    /// `src` keeps its cover only when `dst` had none. Otherwise the file is
    /// unreferenced and the caller is told, the same contract `delete_book` has.
    #[tokio::test]
    async fn a_cover_is_adopted_or_reported_as_orphaned() {
        for (dst_cover, expect_orphan) in [(None, None), (Some("dst.jpg"), Some("src.jpg"))] {
            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let src = s
                .upsert_book(
                    &Book {
                        title: Some("a".into()),
                        cover_path: Some("src.jpg".into()),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .unwrap();
            let dst = s
                .upsert_book(
                    &Book {
                        title: Some("b".into()),
                        cover_path: dst_cover.map(str::to_string),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .unwrap();

            let r = s.merge_books(src, dst).await.unwrap();
            assert_eq!(r.orphaned_cover.as_deref(), expect_orphan);
            assert_eq!(
                s.get_book(dst)
                    .await
                    .unwrap()
                    .unwrap()
                    .cover_path
                    .as_deref(),
                Some(dst_cover.unwrap_or("src.jpg"))
            );
        }
    }

    /// A missing `dst` is a real error — there is nothing to merge into — while
    /// a missing `src` is a satisfied postcondition. Merging a book into itself
    /// must not delete it.
    #[tokio::test]
    async fn the_degenerate_cases_are_distinguished() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = book(&s, "Pachinko").await;

        assert!(matches!(
            s.merge_books(id, 9999).await,
            Err(EngineError::NotFound(_))
        ));

        let gone = s.merge_books(9999, id).await.unwrap();
        assert!(!gone.src_existed);

        let itself = s.merge_books(id, id).await.unwrap();
        assert!(itself.src_existed);
        assert!(
            s.get_book(id).await.unwrap().is_some(),
            "must not self-delete"
        );
    }

    // ---- provenance --------------------------------------------------------

    use super::tests_support::{PROBES, columns, probes_cover_the_merge_table, says_everything};

    async fn sources(s: &Storage, id: i64) -> Vec<(String, String)> {
        s.field_provenance(id)
            .await
            .unwrap()
            .into_iter()
            .map(|f| (f.field, f.source))
            .collect()
    }

    /// `show` and `says` are two readings of one question, and they have to
    /// agree on every column.
    ///
    /// `says` decides what the SQL moves and what may be claimed; `show` is what
    /// item 30's report prints for a field a provider offered and was not
    /// allowed to write. A column where they disagree is either a field that is
    /// claimed and unprintable — a held-back value the user is told about by
    /// name and never shown — or one that is printable and unclaimed.
    ///
    /// Swept over `PROBES`, so a seventeenth rule is covered by arriving rather
    /// than by being remembered.
    #[test]
    fn show_and_says_agree_on_every_column() {
        probes_cover_the_merge_table();
        for (col, set) in PROBES {
            let mut book = Book::default();
            // Silent about this column: neither may speak.
            assert!(!MERGE_RULES.iter().any(|r| r.col == col && (r.says)(&book)));
            assert_eq!(field_value(&book, col), None, "{col} printed from nothing");

            set(&mut book, 7);
            let rule = MERGE_RULES
                .iter()
                .find(|r| r.col == col)
                .expect("probes cover the table");
            assert!(
                (rule.says)(&book),
                "{col}: says nothing about a value it has"
            );
            assert!(
                field_value(&book, col).is_some(),
                "{col}: claimed but unprintable"
            );
        }
    }

    /// `field_value` on a name the table does not govern is `None`, not a panic.
    /// Every caller is downstream of `merge_columns`, but the function is the
    /// wrong place to enforce that.
    #[test]
    fn an_unknown_column_prints_nothing() {
        assert_eq!(field_value(&says_everything(1), "ko_percent"), None);
        assert_eq!(
            merge_columns().count(),
            MERGE_RULES.len(),
            "merge_columns must be the whole table"
        );
    }

    /// **The three consumers of `MERGE_RULES` cannot disagree.**
    ///
    /// `the_two_statements_cannot_disagree` (above, as
    /// `enrich_merges_a_partial_record_exactly_as_the_upsert_does`) pins the two
    /// SQL renderings against each other. This pins the *third* consumer — the
    /// provenance stamp — against the SQL, column by column, and it is the
    /// assertion that stops the failure this arrangement exists to prevent: a
    /// field the merge writes and the stamp forgets, or the reverse.
    ///
    /// For each column: a record that speaks only to that column must move
    /// exactly that column, and claim exactly that field. A wrong `says`
    /// predicate shows up as a missing or extra claim; a wrong bind order shows
    /// up as the *neighbouring* column moving.
    #[tokio::test]
    async fn a_record_claims_exactly_the_fields_it_supplies() {
        probes_cover_the_merge_table();
        for (col, set) in PROBES {
            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let id = s
                .upsert_book(&says_everything(1), Some(Source::GoogleBooks))
                .await
                .unwrap();
            let before = columns(&s, id).await;

            let mut partial = Book::default();
            set(&mut partial, 2);
            s.enrich_book(id, &partial, Some(Source::OpenLibrary))
                .await
                .unwrap();

            let after = columns(&s, id).await;
            for ((name, was), (_, now)) in before.iter().zip(after.iter()) {
                if name == col {
                    assert_ne!(
                        was, now,
                        "{col}: the record said something and nothing moved"
                    );
                } else {
                    assert_eq!(was, now, "a {col} record moved {name}");
                }
            }
            for (name, _) in PROBES {
                let want = if name == col {
                    "openlibrary"
                } else {
                    "googlebooks"
                };
                assert_eq!(
                    s.field_source(id, name).await.unwrap().as_deref(),
                    Some(want),
                    "a {col} record claimed {name}"
                );
            }
        }
    }

    /// **The fourth consumer cannot disagree either.**
    ///
    /// `search::merge_into` was the one place this table did not reach, and a
    /// column added without it compiled cleanly and was silently dropped from
    /// every federated search result — item 32 did exactly that, and only
    /// `every_claimed_field_is_a_merge_column` noticed, by accident, on the
    /// claim rather than on the value. `Rule::federated` closes that, and this
    /// is what stops it being closed in name only: for **every** column, a
    /// record speaking to it either merges and is claimed, or is
    /// [`Federated::Local`] — and the `Local` set is pinned by name below, so
    /// "make it `Local` and move on" is not a quiet option.
    #[test]
    fn every_provider_column_survives_a_federated_merge() {
        probes_cover_the_merge_table();
        for (col, set) in PROBES {
            let rule = MERGE_RULES.iter().find(|r| r.col == col).unwrap();
            let mut src = Book::default();
            set(&mut src, 2);
            let mut dst = Book::default();
            let claimed = merge_provider_record(&mut dst, &src, Source::OpenLibrary);

            match rule.federated {
                Federated::Local => assert!(
                    claimed.is_empty() && !(rule.says)(&dst),
                    "{col} is Local and merged anyway: {claimed:?}"
                ),
                // `series_index` alone is never taken — the pair moves with the
                // name, never before it.
                Federated::WithPair(_) => assert!(
                    claimed.is_empty(),
                    "{col} moves with its pair and was taken on its own: {claimed:?}"
                ),
                Federated::Fill(_) | Federated::Prefer(..) => {
                    assert!(
                        (rule.says)(&dst),
                        "{col} was supplied and did not reach the merged record"
                    );
                    assert!(
                        claimed.contains(&col),
                        "{col} moved without being claimed, which is how a \
                         federated field loses its origin: {claimed:?}"
                    );
                }
            }
        }
    }

    /// The two columns no provider supplies, named — so a third one is a
    /// decision somebody makes here rather than a `Federated::Local` typed to
    /// make a new rule compile.
    #[test]
    fn only_our_own_columns_sit_out_the_federated_merge() {
        let local: Vec<&str> = MERGE_RULES
            .iter()
            .filter(|r| matches!(r.federated, Federated::Local))
            .map(|r| r.col)
            .collect();
        assert_eq!(local, ["sort_title", "cover_path"]);
    }

    /// The pair moves **whole**: an index without a name is not merged, and a
    /// name arriving without an index clears whatever index was there — a stale
    /// `#2` under a different series is the row `Rule::pair` exists to prevent.
    #[test]
    fn a_series_index_travels_with_the_name_or_not_at_all() {
        let mut dst = Book::default();
        let orphan = Book {
            series_index: Some(7.0),
            ..Default::default()
        };
        assert!(merge_provider_record(&mut dst, &orphan, Source::GoogleBooks).is_empty());
        assert_eq!(dst.series_index, None);

        let both = Book {
            series: Some("Dune".into()),
            series_index: Some(2.0),
            ..Default::default()
        };
        let claimed = merge_provider_record(&mut dst, &both, Source::GoogleBooks);
        assert_eq!(claimed, ["series", "series_index"]);
        assert_eq!(dst.series_index, Some(2.0));

        // A different provider's series loses (the field is filled), so nothing
        // moves — including the index, which must not be taken alone.
        let other = Book {
            series: Some("Other".into()),
            series_index: Some(9.0),
            ..Default::default()
        };
        assert!(merge_provider_record(&mut dst, &other, Source::OpenLibrary).is_empty());
        assert_eq!(dst.series.as_deref(), Some("Dune"));
        assert_eq!(dst.series_index, Some(2.0));

        // …and a namer with no index of its own clears the index rather than
        // leaving one series' number under another's name.
        let mut fresh = Book::default();
        let nameless = Book {
            series: Some("Dune".into()),
            ..Default::default()
        };
        let claimed = merge_provider_record(&mut fresh, &nameless, Source::OpenLibrary);
        assert_eq!(claimed, ["series"]);
        assert_eq!(fresh.series_index, None);
    }

    /// **A field the user owns is not overwritten by a provider merge**, for
    /// every column and every source. Without this, item 30's first run silently
    /// replaces every hand correction in the library — which is the reason this
    /// item goes before it.
    ///
    /// Both guards are asserted together on purpose: the value must survive
    /// *and* the claim must, because either one alone is a half-protected field
    /// that the next writer finishes off.
    #[tokio::test]
    async fn a_user_owned_field_survives_every_provider_merge() {
        probes_cover_the_merge_table();
        for (col, set) in PROBES {
            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let id = s
                .upsert_book(&says_everything(1), Some(Source::GoogleBooks))
                .await
                .unwrap();

            let mut mine = Book::default();
            set(&mut mine, 2);
            s.enrich_book(id, &mine, Some(Source::User)).await.unwrap();
            let corrected = columns(&s, id).await;

            for source in Source::ALL.into_iter().filter(|s| *s != Source::User) {
                let mut theirs = Book::default();
                set(&mut theirs, 3);
                s.enrich_book(id, &theirs, Some(source)).await.unwrap();
                // And an unattributed writer, which is the one that would
                // otherwise slip past a rule that keyed on the incoming source.
                s.enrich_book(id, &theirs, None).await.unwrap();
            }

            assert_eq!(columns(&s, id).await, corrected, "{col} was overwritten");
            assert_eq!(
                s.field_source(id, col).await.unwrap().as_deref(),
                Some("user"),
                "{col} lost the user's claim"
            );
        }
    }

    /// A pair that only guards one way guards nothing: the write it exists to
    /// stop is the one to the *other* half.
    #[test]
    fn pairs_are_symmetric() {
        for rule in MERGE_RULES.iter() {
            let Some(other) = rule.pair else { continue };
            let back = MERGE_RULES
                .iter()
                .find(|r| r.col == other)
                .unwrap_or_else(|| panic!("{}: pair names no column", rule.col));
            assert_eq!(
                back.pair,
                Some(rule.col),
                "{} pairs with {other}, which does not pair back",
                rule.col
            );
        }
    }

    /// **Owning half a pair holds the whole pair.**
    ///
    /// This is the second instance of the bug item 30 recorded and left: a
    /// user-owned `isbn_13` with an unowned `isbn_10` landing from a different
    /// edition, so one row carries two ISBNs of two books. `series` +
    /// `series_index` has exactly that shape and is worse to read — "Dune #2"
    /// is a claim about which book this is.
    ///
    /// Both instances are asserted here, because they are one rule.
    #[tokio::test]
    async fn owning_half_a_pair_holds_the_other_half() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&sample(), None).await.unwrap();

        // The user names the series and never says which number it is.
        s.enrich_book(
            id,
            &Book {
                series: Some("Dune".into()),
                ..Default::default()
            },
            Some(Source::User),
        )
        .await
        .unwrap();
        // …and the ISBN of the edition they actually hold.
        s.enrich_book(
            id,
            &Book {
                isbn_13: Some("9780441013593".into()),
                ..Default::default()
            },
            Some(Source::User),
        )
        .await
        .unwrap();

        // A provider that knows a *different* series, and a different edition.
        s.enrich_book(
            id,
            &Book {
                series: Some("Dune Chronicles".into()),
                series_index: Some(2.0),
                isbn_10: Some("0441013597".into()),
                ..Default::default()
            },
            Some(Source::OpenLibrary),
        )
        .await
        .unwrap();

        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.series.as_deref(), Some("Dune"));
        assert_eq!(
            got.series_index, None,
            "an index from a series the user did not name is not this book's"
        );
        assert_eq!(got.isbn_13.as_deref(), Some("9780441013593"));
        assert_eq!(
            got.isbn_10, None,
            "the other edition's isbn_10 landed beside the user's isbn_13"
        );
        // And the held halves stay unattributed rather than being claimed by a
        // provider whose value was never written.
        assert_eq!(s.field_source(id, "series_index").await.unwrap(), None);
        assert_eq!(s.field_source(id, "isbn_10").await.unwrap(), None);
    }

    /// The pair guard is *only* about the user. Two providers merging into an
    /// unowned pair is the ordinary case and must still fill it.
    #[tokio::test]
    async fn an_unowned_pair_still_fills() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&sample(), None).await.unwrap();
        s.enrich_book(
            id,
            &Book {
                series: Some("Dune".into()),
                series_index: Some(0.5),
                ..Default::default()
            },
            Some(Source::OpenLibrary),
        )
        .await
        .unwrap();
        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.series.as_deref(), Some("Dune"));
        // Half-numbered novellas are why the column is REAL.
        assert_eq!(got.series_index, Some(0.5));
    }

    /// A subject set is a value like `authors`: it round-trips as JSON, an empty
    /// one means "this record does not say", and a provider with one replaces
    /// the stored set rather than growing it.
    #[tokio::test]
    async fn subjects_are_replaced_whole_and_an_empty_set_says_nothing() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let mut b = sample();
        b.subjects = vec!["Fiction / Literary".into(), "Historical".into()];
        let id = s.upsert_book(&b, Some(Source::GoogleBooks)).await.unwrap();
        assert_eq!(
            s.get_book(id).await.unwrap().unwrap().subjects,
            vec!["Fiction / Literary", "Historical"]
        );

        // A record with nothing to say leaves them alone…
        s.enrich_book(id, &Book::default(), Some(Source::OpenLibrary))
            .await
            .unwrap();
        assert_eq!(
            s.get_book(id).await.unwrap().unwrap().subjects.len(),
            2,
            "an empty set must read as silence, not as 'no subjects'"
        );

        // …and one with a set replaces it, rather than unioning two
        // vocabularies into a list neither provider published.
        s.enrich_book(
            id,
            &Book {
                subjects: vec!["Korean Americans".into()],
                ..Default::default()
            },
            Some(Source::OpenLibrary),
        )
        .await
        .unwrap();
        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.subjects, vec!["Korean Americans"]);
        assert_eq!(
            s.field_source(id, "subjects").await.unwrap().as_deref(),
            Some("openlibrary")
        );
    }

    /// The guard is in the clause, so it is in **both** statements the clause
    /// generates. The sweep above goes through `enrich_book`; this is the upsert
    /// half, through the `ON CONFLICT DO UPDATE` branch.
    #[tokio::test]
    async fn the_upsert_honours_the_user_guard_too() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let mut b = sample();
        b.description = Some("what the provider said".into());
        let id = s.upsert_book(&b, Some(Source::GoogleBooks)).await.unwrap();

        s.enrich_book(
            id,
            &Book {
                description: Some("what I actually think".into()),
                ..Default::default()
            },
            Some(Source::User),
        )
        .await
        .unwrap();

        // Same ISBN, so this is the conflict branch of the upsert.
        let mut refreshed = sample();
        refreshed.description = Some("the provider, again".into());
        refreshed.publisher = Some("Grand Central".into());
        assert_eq!(
            s.upsert_book(&refreshed, Some(Source::OpenLibrary))
                .await
                .unwrap(),
            id
        );

        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.description.as_deref(), Some("what I actually think"));
        assert_eq!(
            got.publisher.as_deref(),
            Some("Grand Central"),
            "one held field must not hold the rest of the record"
        );
        assert_eq!(
            s.field_source(id, "description").await.unwrap().as_deref(),
            Some("user")
        );
        assert_eq!(
            s.field_source(id, "publisher").await.unwrap().as_deref(),
            Some("openlibrary")
        );
    }

    /// `fill_book` is the mirror: the stored row wins, so the incoming record
    /// may only claim what it actually filled. A source that claimed a field
    /// whose value it lost would attribute somebody else's value to itself,
    /// which is the failure mode this whole table exists to prevent.
    #[tokio::test]
    async fn a_fallback_record_claims_only_the_gaps_it_filled() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    description: Some("from the provider".into()),
                    ..Default::default()
                },
                Some(Source::GoogleBooks),
            )
            .await
            .unwrap();

        s.fill_book(
            id,
            &Book {
                title: Some("PACHINKO (epub metadata)".into()),
                description: Some("from the file".into()),
                language: Some("en".into()),
                ..Default::default()
            },
            Some(Source::Epub),
        )
        .await
        .unwrap();

        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.title.as_deref(), Some("Pachinko"), "stored wins");
        assert_eq!(got.description.as_deref(), Some("from the provider"));
        assert_eq!(got.language.as_deref(), Some("en"), "the gap is filled");

        let mut got = sources(&s, id).await;
        got.sort();
        assert_eq!(
            got,
            vec![
                ("description".to_string(), "googlebooks".to_string()),
                ("language".to_string(), "epub".to_string()),
                ("title".to_string(), "googlebooks".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn delete_returns_cover_path() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let mut b = sample();
        b.cover_path = Some("database/images/x.jpg".into());
        let id = s.upsert_book(&b, None).await.unwrap();
        let cover = s.delete_book(id).await.unwrap();
        assert_eq!(cover.as_deref(), Some("database/images/x.jpg"));
        assert!(s.get_book(id).await.unwrap().is_none());
        assert!(matches!(
            s.delete_book(id).await,
            Err(EngineError::NotFound(_))
        ));
    }
}

#[cfg(test)]
mod props {
    use super::tests_support::*;
    use proptest::prelude::*;

    proptest! {
        /// Merging `src` into `dst` twice equals merging it once.
        ///
        /// Stated as a property rather than an example because the interesting
        /// input is the *overlap*: which annotations and which flashcard words
        /// the two books happen to share is exactly what decides whether a row
        /// moves, is dropped, or collides — and picking three overlaps by hand
        /// is picking the three that work.
        ///
        /// It matters because the caller retries. A device screen firing the
        /// same merge twice, a re-run after a crash, a user pressing the key
        /// again: none of those should be able to leave the library different
        /// from one clean run.
        #[test]
        fn merging_twice_equals_merging_once(
            src_texts in proptest::collection::vec("[a-c]{1,3}", 0..6),
            dst_texts in proptest::collection::vec("[a-c]{1,3}", 0..6),
            src_words in proptest::collection::vec("[x-z]{1,2}", 0..4),
            dst_words in proptest::collection::vec("[x-z]{1,2}", 0..4),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let s = fixture(&src_texts, &dst_texts, &src_words, &dst_words).await;
                let (src, dst) = (s.src, s.dst);

                let first = s.storage.merge_books(src, dst).await.unwrap();
                prop_assert!(first.src_existed);
                let after_one = snapshot(&s.storage, dst).await;

                let second = s.storage.merge_books(src, dst).await.unwrap();
                prop_assert!(!second.src_existed, "src is gone; there is nothing left to do");
                prop_assert_eq!(second, super::MergeReport::default());
                prop_assert_eq!(after_one, snapshot(&s.storage, dst).await);
                Ok(())
            })?;
        }
    }
}

/// Shared setup for the merge property. Lives outside `mod tests` because
/// `mod props` needs it too, and duplicating the fixture is how the two would
/// drift into testing different things.
#[cfg(test)]
mod tests_support {
    use super::*;
    use crate::storage::NewHighlight;

    pub struct Fixture {
        pub storage: Storage,
        pub src: i64,
        pub dst: i64,
    }

    /// One writer per [`MERGE_RULES`] column, so a test can say "a record that
    /// speaks to *this* field" once per column without naming the column
    /// twice.
    ///
    /// It is a second list, and it is the only one: the first assertion of every
    /// test that uses it is that its column names are `MERGE_RULES`'s, in order.
    /// A seventeenth rule therefore fails these tests loudly rather than
    /// silently going uncovered — which is the whole reason the probes are here
    /// rather than inlined into one test that happens to name four fields.
    ///
    /// `n` distinguishes one record from another. Nothing validates these
    /// values — `upsert_book` does not check ISBNs, `normalize_isbn` guards the
    /// door much earlier — so they only have to differ.
    #[allow(clippy::type_complexity)]
    pub const PROBES: [(&str, fn(&mut Book, u32)); 19] = [
        ("title", |b, n| b.title = Some(format!("title-{n}"))),
        ("sort_title", |b, n| {
            b.sort_title = Some(format!("sort-{n}"))
        }),
        ("authors", |b, n| b.authors = vec![format!("author-{n}")]),
        ("translators", |b, n| {
            b.translators = vec![format!("translator-{n}")]
        }),
        ("publisher", |b, n| b.publisher = Some(format!("pub-{n}"))),
        ("publish_year", |b, n| {
            b.publish_year = Some(1900 + i64::from(n))
        }),
        ("language", |b, n| b.language = Some(format!("l{n}"))),
        ("isbn_10", |b, n| b.isbn_10 = Some(format!("isbn10-{n}"))),
        ("isbn_13", |b, n| b.isbn_13 = Some(format!("isbn13-{n}"))),
        ("openlibrary_key", |b, n| {
            b.openlibrary_key = Some(format!("/works/OL{n}W"))
        }),
        ("googlebooks_id", |b, n| {
            b.googlebooks_id = Some(format!("gb-{n}"))
        }),
        ("cover_url", |b, n| {
            b.cover_url = Some(format!("https://example.invalid/{n}.jpg"))
        }),
        ("cover_path", |b, n| {
            b.cover_path = Some(format!("database/images/{n}.jpg"))
        }),
        ("page_count", |b, n| b.page_count = Some(100 + i64::from(n))),
        ("description", |b, n| {
            b.description = Some(format!("description-{n}"))
        }),
        ("first_sentence", |b, n| {
            b.first_sentence = Some(format!("first-{n}"))
        }),
        ("subjects", |b, n| b.subjects = vec![format!("subject-{n}")]),
        ("series", |b, n| b.series = Some(format!("series-{n}"))),
        // Fractional, so the probe also pins that the REAL column round-trips a
        // half-numbered novella rather than truncating it to book two.
        ("series_index", |b, n| {
            b.series_index = Some(f64::from(n) + 0.5)
        }),
    ];

    /// A record that speaks to every column, all of it distinguishable by `n`.
    pub fn says_everything(n: u32) -> Book {
        let mut b = Book::default();
        for (_, set) in PROBES {
            set(&mut b, n);
        }
        b
    }

    /// Every `MERGE_RULES` column of one row, as text, keyed by column name.
    ///
    /// `CAST(… AS TEXT)` so the two integer columns come back comparably;
    /// the point is only whether a value moved, never what type it is.
    pub async fn columns(storage: &Storage, id: i64) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        for (col, _) in PROBES {
            let v: Option<String> = sqlx::query_scalar(&format!(
                "SELECT CAST({col} AS TEXT) FROM books WHERE id = ?"
            ))
            .bind(id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
            out.push((col.to_string(), v));
        }
        out
    }

    /// The probes cover the merge table exactly. Called first by every test that
    /// sweeps them, so "item 32 added a column" is a failure with a name.
    pub fn probes_cover_the_merge_table() {
        let probed: Vec<&str> = PROBES.iter().map(|(c, _)| *c).collect();
        let ruled: Vec<&str> = super::MERGE_RULES.iter().map(|r| r.col).collect();
        assert_eq!(
            probed, ruled,
            "PROBES must name every MERGE_RULES column, in order: a column with \
             no probe is a column no provenance test covers"
        );
    }

    /// Highlights are keyed on their text, so a text appearing in both lists is
    /// an identity collision and one appearing once is a plain move. Flashcard
    /// words work the same way against `UNIQUE(book_id, word)`.
    pub async fn fixture(
        src_texts: &[String],
        dst_texts: &[String],
        src_words: &[String],
        dst_words: &[String],
    ) -> Fixture {
        let storage = Storage::connect("sqlite::memory:").await.unwrap();
        let mk = |title: &str| Book {
            title: Some(title.to_string()),
            ..Default::default()
        };
        let src = storage.upsert_book(&mk("src"), None).await.unwrap();
        let dst = storage.upsert_book(&mk("dst"), None).await.unwrap();

        for (book_id, texts) in [(src, src_texts), (dst, dst_texts)] {
            for t in texts {
                let h = NewHighlight {
                    text: t.clone(),
                    chapter: None,
                    page: Some(1),
                    pos0: Some(format!("/body/p[1]/text().{t}")),
                    pos1: None,
                    ko_datetime: Some("2026-01-01 00:00:00".into()),
                    ko_datetime_updated: None,
                    color: None,
                    note: None,
                    source: "koreader".into(),
                };
                storage.insert_highlight(book_id, &h).await.unwrap();
            }
        }
        for (book_id, words) in [(src, src_words), (dst, dst_words)] {
            for w in words {
                storage.insert_flashcard(book_id, None, w, None).await.ok();
            }
        }
        Fixture { storage, src, dst }
    }

    /// Everything the merge could plausibly disturb, in a comparable shape.
    pub async fn snapshot(storage: &Storage, dst: i64) -> (Vec<String>, Vec<String>, i64) {
        let mut texts: Vec<String> = storage
            .list_highlights(dst)
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.text)
            .collect();
        texts.sort();
        let mut words: Vec<String> = storage
            .list_flashcards_for_book(dst)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.word)
            .collect();
        words.sort();
        let books: i64 = sqlx::query_scalar("SELECT count(*) FROM books")
            .fetch_one(storage.pool())
            .await
            .unwrap();
        (texts, words, books)
    }
}
