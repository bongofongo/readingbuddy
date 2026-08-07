//! One row per *reading* of a book.
//!
//! `books` used to carry `current_page`, `finished`, `date_started` and
//! `date_finished`, which modelled one reading of one book. Rereads are real, so
//! they moved here (migration `0005`). [`Book`](crate::book::Book) keeps all
//! four as **read-only projections** of the current reading, which is what left
//! every render call site untouched — see `BOOK_COLUMNS` in
//! [`super::books`].

use std::collections::HashMap;

use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use super::books::{BOOK_COLUMNS, BOOK_FROM, bind_all, book_current_join, row_to_book};
use super::highlights::card_passage_order;
use super::query::{Predicate, ReadingFilter, ReadingQuery};
use super::{Highlight, Storage, now_unix};
use crate::book::Book;
use crate::error::{EngineError, Result};
use crate::koreader::KoStatus;
use crate::progress::Progress;

/// Which read a highlight belongs to, as a person counts them.
///
/// Item 17c. This was `BookView::read_number` and `shows_read_gutter` in the
/// TUI's own state (`crates/tui/src/app.rs`) — a domain rule sitting in a
/// frontend, and one that **silently depends on the ordering contract of
/// [`Storage::list_readings`]** (oldest first). A second frontend numbering
/// reads off a differently-ordered list would disagree with the CLI's
/// `reading 2/3` about which read a highlight came from, and nothing would look
/// wrong on either screen. `decisions.md` already calls the read gutter
/// load-bearing: *a column that nothing renders is a claim nothing can check*.
///
/// Borrowed rather than owning: the caller has the list already, and the type
/// exists to name the rule, not to hold data.
#[derive(Debug, Clone, Copy)]
pub struct ReadNumbering<'a> {
    readings: &'a [Reading],
}

impl<'a> ReadNumbering<'a> {
    /// `readings` must be in [`Storage::list_readings`] order — oldest first.
    /// That is the order the 1-based numbers are counted in and the order
    /// `rb show` prints.
    pub fn new(readings: &'a [Reading]) -> ReadNumbering<'a> {
        ReadNumbering { readings }
    }

    /// Which read, 1-based, or `None`.
    ///
    /// `None` covers three things and the caller wants the same answer to all
    /// of them: the highlight is unattributed (no reading's window held its
    /// timestamp — `attribute_highlights` leaves those `NULL` on purpose), the
    /// reading it names is not in this list, or **the book has been read once**.
    /// A single-read book has nothing to tell apart, and a number on every row
    /// of it is a column that always reads `1`.
    ///
    /// The third case is [`ReadCount::ordinal`] and is reached through it, so
    /// there is one statement of "a lone read has no number" rather than two.
    pub fn number_of(&self, reading_id: Option<i64>) -> Option<usize> {
        let rid = reading_id?;
        let pos = self.readings.iter().position(|r| r.id == rid)?;
        ReadCount::new(pos as i64 + 1, self.readings.len() as i64)
            .ordinal()
            .map(|n| n as usize)
    }

    /// Is there more than one read to tell apart?
    ///
    /// Asked once for a whole list rather than per row, so every row is laid out
    /// the same: deciding per row leaves an unattributed highlight flush against
    /// the border while its neighbours are indented.
    pub fn is_meaningful(&self) -> bool {
        ReadCount::new(1, self.readings.len() as i64).tells_reads_apart()
    }
}

/// One read, counted: **which** it is and **how many** there are (item 41).
///
/// [`ReadNumbering`] answers the same two questions for a caller that already
/// holds the whole `Vec<Reading>` — the TUI's gutter, `rb show`. This is the
/// form the answer takes when the caller does *not*: a page of
/// [`Storage::list_reading_rows`] may hold the second read of a book without
/// holding the first, so the counting cannot be done over the page and is done
/// in SQL over every sibling. `ReadNumbering` now reaches its rule through this
/// type, which is what makes "the wall and the gutter agree" structural instead
/// of a coincidence between two files.
///
/// **Both numbers cross the wire and neither is an `Option`**, which is the
/// decision. `ReadNumbering::number_of` folds "read once" into the same `None`
/// as "unattributed" and "not in this list", and that is right for a gutter
/// wanting one answer to all three — but two of those three are facts about a
/// *highlight*, and on a list whose rows are readings they are unreachable.
/// Shipping the fold anyway would have made `None` mean a fourth thing, *the
/// caller who built this row could not tell*, which is the ambiguity a row type
/// must not carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadCount {
    /// 1-based, counted oldest first — [`Storage::list_readings`]' order, which
    /// is the ordering contract every read number in this app depends on.
    pub number: i64,
    /// How many readings this book has, in total and not on this page.
    pub of: i64,
}

impl ReadCount {
    pub fn new(number: i64, of: i64) -> ReadCount {
        ReadCount { number, of }
    }

    /// Is there more than one read to tell apart?
    pub fn tells_reads_apart(&self) -> bool {
        self.of > 1
    }

    /// The number to *show*, or `None` — [`ReadNumbering::number_of`]'s rule,
    /// stated once. A book read once has nothing to tell apart, and a number on
    /// every row of it is a column that always reads `1`.
    pub fn ordinal(&self) -> Option<i64> {
        self.tells_reads_apart().then_some(self.number)
    }
}

/// Our own status vocabulary. Distinct from `ko_status`, which mirrors what the
/// *device* said and is never written by us.
pub const STATUS_READING: &str = "reading";
pub const STATUS_FINISHED: &str = "finished";
pub const STATUS_ABANDONED: &str = "abandoned";

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub id: i64,
    pub book_id: i64,
    pub started_at: Option<i64>,
    /// `NULL` means open, and the partial unique index
    /// `idx_readings_one_open` makes "at most one open reading per book" an
    /// invariant rather than a convention.
    pub finished_at: Option<i64>,
    pub status: String,
    /// Who wrote this row: `manual` | `koreader` | `goodreads` | `migrated`.
    ///
    /// **This doc comment is the vocabulary.** Migration `0005` carries the
    /// original list in a SQL comment and it has been stale since item 15 added
    /// `goodreads`; it cannot be corrected, because a migration is append-only
    /// and CI has a job that refuses a modified one. So the list lives here,
    /// beside the type every reader of the column goes through, and the SQL
    /// comment is a historical note about what the vocabulary was in `0005`.
    ///
    /// A `String` and deliberately not an enum (item 17): it names a *writer*,
    /// it grows by one every time an importer is added, and nothing branches on
    /// it. An enum would be a second list of importers to keep in step with the
    /// first.
    ///
    /// **Item 22 considered adding `local` here and did not** — see
    /// `docs/decisions.md`, entry 22. Nothing new writes readings, so `local`
    /// would have been a synonym for `manual`; the word it earned is in
    /// `reading_events.source`, where sources are claimants rather than writers
    /// and a typed page is a genuinely new claimant.
    pub source: String,
    pub current_page: Option<i64>,
    /// The device-owned mirror. Refreshed by **straight assignment**, never
    /// `COALESCE` — a sidecar is the complete state, so a cleared rating means
    /// the user cleared it. The user's own rating belongs to a Review.
    pub ko_status: Option<String>,
    pub ko_percent: Option<f64>,
    pub ko_rating: Option<i64>,
    pub created_at: i64,
    pub last_modified: i64,
}

pub(super) const READING_COLUMNS: &str = "id, book_id, started_at, finished_at, status, source, current_page, \
     ko_status, ko_percent, ko_rating, created_at, last_modified";

/// What [`Storage::list_open_readings`] renames the reading's columns to.
///
/// Six of the twelve — `id`, `book_id`, `current_page`, `status`, `created_at`,
/// `last_modified` — are names `BOOK_COLUMNS` also projects, so joining the two
/// into one row means renaming one side of the collision.
const JOINED_READING_PREFIX: &str = "r_";

/// `READING_COLUMNS`, qualified by a table alias and prefixed.
///
/// Derived from the one list rather than spelled out a second time: a
/// hand-written rename list is a second copy of the columns waiting to drift
/// from the first, and a column added to `Reading` but forgotten here would
/// surface as a decode error at runtime rather than a compile error.
fn reading_columns_as(alias: &str, prefix: &str) -> String {
    READING_COLUMNS
        .split(',')
        .map(|c| {
            let c = c.trim();
            format!("{alias}.{c} AS {prefix}{c}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn row_to_reading(row: &SqliteRow) -> Result<Reading> {
    row_to_reading_prefixed(row, "")
}

/// One mapping for both the bare `SELECT {READING_COLUMNS}` shape and the
/// joined, prefixed one.
fn row_to_reading_prefixed(row: &SqliteRow, prefix: &str) -> Result<Reading> {
    let col = |name: &str| format!("{prefix}{name}");
    Ok(Reading {
        id: row.try_get(col("id").as_str())?,
        book_id: row.try_get(col("book_id").as_str())?,
        started_at: row.try_get(col("started_at").as_str())?,
        finished_at: row.try_get(col("finished_at").as_str())?,
        status: row.try_get(col("status").as_str())?,
        source: row.try_get(col("source").as_str())?,
        current_page: row.try_get(col("current_page").as_str())?,
        ko_status: row.try_get(col("ko_status").as_str())?,
        ko_percent: row.try_get(col("ko_percent").as_str())?,
        ko_rating: row.try_get(col("ko_rating").as_str())?,
        created_at: row.try_get(col("created_at").as_str())?,
        last_modified: row.try_get(col("last_modified").as_str())?,
    })
}

/// The device-owned columns, and the null-safe test for "the sidecar disagrees
/// with what we stored".
///
/// One copy, shared by the conditional `UPDATE` in
/// [`Storage::set_device_state`] and the read-only `SELECT` in
/// [`Storage::device_state_differs`], for the same reason
/// `DEVICE_FIELDS_DIFFER` is shared in [`super::highlights`]: a preview that
/// reported different changes than the write it previews would be worse than no
/// preview at all.
///
/// `IS NOT` rather than `!=`: SQLite's `!=` yields NULL when either side is
/// NULL, so a rating the user *cleared* on the device would compare as "no
/// change" and never be removed.
const DEVICE_STATE_DIFFER: &str =
    "(ko_status IS NOT ?2 OR ko_percent IS NOT ?3 OR ko_rating IS NOT ?4)";

/// One book's readings as half-open-ish intervals of unix seconds, `?1` the
/// book id, columns `reading_id`/`win_start`/`win_end`.
///
/// **Extracted so there is one definition, not two.** The derivation of a
/// missing `started_at` is the subtle part — see [`Storage::attribute_highlights`]
/// for the full argument and for the ±infinity bug it replaced — and item 31
/// needs the same windows to place a *day* of measured reading time. Two copies
/// of this would be two chances to reintroduce that bug, and only one of them
/// would have a test aimed at it.
///
/// `?1` rather than `?` because both callers mention the book id twice.
pub(super) const READING_WINDOWS: &str = "
    SELECT r.id AS reading_id,
           COALESCE(
               r.started_at,
               (SELECT MAX(p.finished_at) + 1 FROM readings p
                 WHERE p.book_id = r.book_id
                   AND p.id <> r.id
                   AND p.finished_at IS NOT NULL
                   AND p.finished_at <= COALESCE(r.finished_at, 8640000000000)),
               -8640000000000
           ) AS win_start,
           COALESCE(r.finished_at, 8640000000000) AS win_end
      FROM readings r
     WHERE r.book_id = ?1";

/// **When a read began**, as an `ORDER BY` list over `alias` — and the one
/// definition of it.
///
/// `COALESCE(started_at, created_at)` is the key [`Storage::list_readings`] has
/// ordered by since `0005`, and that ordering is a **silent contract**:
/// `ReadNumbering`'s doc records that the CLI's `reading 2/3`, the TUI's gutter
/// and now [`Storage::list_reading_rows`]' `read_number` all count in it, and a
/// second spelling would make two of them disagree with nothing on either screen
/// looking wrong.
///
/// A function of the alias because it has three callers in three shapes: an
/// `ORDER BY` (oldest first, for a book), a *descending* `ORDER BY`
/// ([`ReadingSort::Started`], for the library) and a **row value** on both sides
/// of a `<=` (the ordinal's correlated subquery). The comma list is the same
/// text in all three, which is precisely what parenthesising it as a row value
/// requires.
///
/// `started_at` is NULL for ordinary rows rather than broken ones — the CSV has
/// no start date and `goodreads.rs` refuses to invent one — so the fallback is
/// `created_at`, which is when we learned of the read.
pub(super) fn reading_age_key(alias: &str) -> String {
    format!("{}, {alias}.id", reading_began(alias))
}

/// **When a read began**, as a single expression — the leading term of
/// [`reading_age_key`] and the expression `idx_readings_started` is declared on.
///
/// Split out because [`ReadingSort::Started`] cannot use the key: a comma list's
/// terms take their own direction, so appending ` DESC` to it would reverse only
/// `id`. It orders by this term descending and takes its own tie-break, and
/// `the_started_sort_orders_by_the_key_list_readings_counts_in` is what stops
/// the two spellings drifting.
pub(super) fn reading_began(alias: &str) -> String {
    format!("COALESCE({alias}.started_at, {alias}.created_at)")
}

/// What [`Storage::list_reading_rows`] orders by.
///
/// Three arms, each indexed by migration `0018`, and there is deliberately no
/// fourth: a title sort would order this list by a `books` column, which cannot
/// be served by any index on `readings` and would put the whole-table sort back
/// that `0018` exists to remove. A wall sorted by book title is a wall the
/// caller can ask for one book at a time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadingSort {
    /// Most recently finished first. Open readings have no `finished_at` and
    /// land last, which is SQLite's own ordering for NULLs under `DESC` and is
    /// where a read that has not ended belongs on a list of reads that did.
    #[default]
    Finished,
    /// Most recently begun first, by [`reading_age_key`].
    Started,
    LastModified,
}

/// The `ORDER BY` for one reading sort, **ending in `readings.id`**.
///
/// Item 18 learned this the hard way one table over and wrote down that the
/// behavioural test cannot catch its absence: SQLite's sorter is deterministic
/// for one plan over one set of rows, so deleting the tie-break leaves a
/// partition test green until the day the plan changes and a reading appears on
/// two pages and another on none. `readings.id` is the `INTEGER PRIMARY KEY`,
/// i.e. the rowid, which SQLite appends to every index entry — so the tie-break
/// comes free off the same scan in either direction and costs nothing to keep.
/// `the_reading_order_is_a_total_order` reads this function's output.
///
/// The tie-break follows the primary key's direction so a descending list's ties
/// read newest-first like the rest of it.
fn reading_order_by(sort: ReadingSort) -> &'static str {
    match sort {
        ReadingSort::Finished => "readings.finished_at DESC, readings.id DESC",
        // The same expression `idx_readings_started` is declared on, spelled the
        // same way — `0016`'s lesson about `COALESCE(sort_title, title)`. Not
        // built from `reading_age_key` even though it is that key descending:
        // the key is a comma list whose terms take their own direction, so
        // appending ` DESC` would reverse only `readings.id` and leave the
        // leading term ascending — a clause that reads right, runs, and orders
        // by neither thing the caller asked for.
        ReadingSort::Started => {
            "COALESCE(readings.started_at, readings.created_at) DESC, readings.id DESC"
        }
        ReadingSort::LastModified => "readings.last_modified DESC, readings.id DESC",
    }
}

/// The exact statement [`Storage::list_reading_rows`] issues, minus its two
/// trailing binds.
///
/// A function rather than a `format!` inside the method **so the plan test can
/// ask for the real thing.** `0016`'s equivalent rebuilds the statement in the
/// test out of the same pieces, which is one edit away from testing an
/// approximation — and the likeliest failure an index migration has is a
/// *mismatch* between the clause and the index, which an approximation is
/// exactly what cannot catch.
fn reading_rows_sql(sort: ReadingSort, predicate: &Predicate) -> String {
    let reading = reading_columns_as("readings", JOINED_READING_PREFIX);
    let mine = reading_age_key("readings");
    let sibling = reading_age_key("sib");
    let passage = card_passage_order("h");
    let join = book_current_join!();
    let where_sql = &predicate.sql;
    let order = reading_order_by(sort);
    format!(
        "SELECT {BOOK_COLUMNS}, {reading},
                (SELECT count(*) FROM readings sib
                  WHERE sib.book_id = readings.book_id) AS of_reads,
                (SELECT count(*) FROM readings sib
                  WHERE sib.book_id = readings.book_id
                    AND ({sibling}) <= ({mine})) AS read_number,
                (SELECT h.id FROM highlights h
                  WHERE h.reading_id = readings.id
                  ORDER BY {passage} LIMIT 1) AS passage_id
           FROM readings JOIN books ON books.id = readings.book_id {join}
          {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"
    )
}

/// The exact statement [`Storage::reading_years`] issues, minus its binds.
///
/// A function for [`reading_rows_sql`]'s reason — the plan test must read the
/// real thing — and here it guards a specific, silent mistake. `count_readings`
/// keeps `JOIN books ON books.id = readings.book_id` on purpose, so that it and
/// the page differ in exactly one clause; copying that join here **destroys the
/// plan**. `FROM readings` alone is served by `idx_readings_finished_at` as a
/// *covering* index and the table is never touched; with the join the planner
/// scans `books` and searches `readings` per book, which is a 500-book scan to
/// draw six pills. The join cannot change the answer (`readings.book_id` is a
/// NOT NULL foreign key) and `ReadingFilter::predicate` names only `readings`,
/// so dropping it is safe for every filter.
///
/// `strftime` is in the **projection and never in the `WHERE`** — an expression
/// over the column is what `0018`'s own control test shows the index declining
/// in silence. The year filter still binds the bare column; only the grouping
/// converts.
fn reading_years_sql(predicate: &Predicate) -> String {
    let where_sql = &predicate.sql;
    format!(
        "SELECT CAST(strftime('%Y', readings.finished_at, 'unixepoch') AS INTEGER) AS year
           FROM readings {where_sql}
          GROUP BY year ORDER BY year DESC"
    )
}

/// The years a filter's readings ended in, and whether any of them is still
/// open (item 51).
///
/// **No count per year.** The wall asks `count_readings` for the year it is
/// drawing, and a second number arriving here would be a number nobody asked
/// for and nobody can be shown to have got right — while a row of years each
/// carrying a figure is a scoreboard, which is the framing the axiom bans by
/// name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadingYears {
    /// Newest first. A year is here iff at least one matching reading closed in
    /// it — never because a note, a highlight or a device measurement is dated
    /// in it.
    pub years: Vec<i32>,
    /// Whether any matching reading has **not** ended. Those rows are in no
    /// year, and without this the years do not partition the wall.
    pub open: bool,
}

/// One row of the library-wide readings list (item 43): a reading, the book it
/// is of, and the two things a card needs that a `Reading` cannot answer.
///
/// **Not a card, and the difference is what keeps item 44's refusal standing.**
/// That item declined a `CardDto` because a card is a *layout* — cover, dates,
/// rating, passage, notes — and a layout is a frontend's composition of facts
/// the API already serves. This row carries no rating and no notes, and it is
/// not shaped by what a card draws; it is shaped by what **one round trip** has
/// to contain for a page of readings to be drawable at all. That is item 18's
/// `book_summaries` argument (one call for a page, not four per row) rather than
/// a layout, and the two are told apart by asking what would be added next: a
/// card would grow the rating, and this will not.
///
/// The `passage` is here rather than on `ReadingDto` for item 44's own reason,
/// read the other way round: putting it on `ReadingDto` would ride the reader's
/// private highlight text along on every row of every `ListReadings`, while
/// every row of *this* list is one somebody is drawing a card for.
// No `PartialEq`: neither `Book` nor `Highlight` derives one, and adding it to
// two domain types so a row can be compared whole would be a test's convenience
// changing the domain.
#[derive(Debug, Clone)]
pub struct ReadingRow {
    pub book: Book,
    pub reading: Reading,
    /// **This** reading's progress, not the book's.
    ///
    /// Filled here rather than by the caller because this is the one place that
    /// holds both halves: `readings` carries no length, and
    /// `Engine::readings_with_progress` exists precisely because a caller with a
    /// bare `Reading` cannot reach [`Progress::of_reading`]. Handing back a row
    /// that holds the book *and* leaves the pairing to the caller would rebuild
    /// that trap in a new shape.
    pub progress: Progress,
    /// Which read this is, out of how many — see [`ReadCount`]. Counted over
    /// every reading of the book and never over the page.
    pub count: ReadCount,
    /// The passage a card shows for this reading, by item 44's rule, or `None`.
    pub passage: Option<Highlight>,
}

/// KOReader's `datetime` as unix seconds.
///
/// The device writes `YYYY-MM-DD HH:MM:SS` with no zone, so it is read as UTC —
/// the same reading SQLite's `strftime('%s', …)` takes, which is what
/// [`Storage::attribute_highlights`] compares against. The two must agree; a
/// second convention here would put attribution and its test hours apart for
/// anyone not on UTC.
pub fn ko_datetime_to_unix(s: &str) -> Option<i64> {
    let fmt = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    time::PrimitiveDateTime::parse(s.trim(), fmt)
        .ok()
        .map(|dt| dt.assume_utc().unix_timestamp())
}

impl Storage {
    /// The open reading, if there is one.
    pub async fn active_reading(&self, book_id: i64) -> Result<Option<Reading>> {
        let sql = format!(
            "SELECT {READING_COLUMNS} FROM readings WHERE book_id = ? AND finished_at IS NULL"
        );
        let row = sqlx::query(&sql)
            .bind(book_id)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_reading).transpose()
    }

    /// One reading by id.
    pub async fn get_reading(&self, id: i64) -> Result<Option<Reading>> {
        let sql = format!("SELECT {READING_COLUMNS} FROM readings WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_reading).transpose()
    }

    /// Every reading of a book, oldest first.
    ///
    /// **This ordering is a contract, not a preference** — see
    /// [`reading_age_key`], which is where it is now written down once so that
    /// `read_number` in [`Storage::list_reading_rows`] counts in the same
    /// direction this list numbers in.
    pub async fn list_readings(&self, book_id: i64) -> Result<Vec<Reading>> {
        let age = reading_age_key("readings");
        let sql = format!(
            "SELECT {READING_COLUMNS} FROM readings WHERE book_id = ?
             ORDER BY {age} ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(book_id)
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_reading).collect()
    }

    /// Every book with an **open** reading, most-recently-touched first.
    ///
    /// This is the home screen's query. It is deliberately the *same* join
    /// `BOOK_COLUMNS`/`BOOK_FROM` already resolve `Book`'s four progress
    /// projections through, plus `WHERE cur.finished_at IS NULL` — a second join
    /// written to the same intent would be free to disagree with the first, and
    /// a home screen whose progress bar came from a different reading than
    /// `Book::current_page` would look right while being wrong.
    ///
    /// `BOOK_FROM`'s subquery orders `(finished_at IS NULL) DESC` first, so
    /// where a book has an open reading `cur` **is** that reading, and the
    /// `WHERE` is a filter on the joined row rather than a second search for it.
    ///
    /// **`cur.id IS NOT NULL` is load-bearing, not belt-and-braces.** The join
    /// is a `LEFT JOIN`, so a book with no reading at all still produces a row —
    /// with every `cur` column NULL, which satisfies `finished_at IS NULL` on
    /// its own. Filtering on that alone listed the whole library as currently
    /// being read, the empty state included.
    ///
    /// Sorted by the later of the book's and the reading's `last_modified`:
    /// reading the book bumps `books.last_modified` (`update_progress` does it
    /// explicitly), but a device sync writes `readings.last_modified` alone
    /// (`set_device_state`), so either stamp on its own would leave a book you
    /// just read on the reader sitting at the bottom of the list.
    ///
    /// **An abandoned reading is an open reading and appears here.** `status`
    /// is on the returned [`Reading`], so a frontend can say so; dropping the
    /// row instead would make a book you might pick up unreachable from the one
    /// screen that lists what you are reading, and there is no other place it
    /// would show up.
    pub async fn list_open_readings(&self, limit: i64) -> Result<Vec<(Book, Reading)>> {
        let reading = reading_columns_as("cur", JOINED_READING_PREFIX);
        let sql = format!(
            "SELECT {BOOK_COLUMNS}, {reading} {BOOK_FROM}
             WHERE cur.id IS NOT NULL AND cur.finished_at IS NULL
             ORDER BY MAX(books.last_modified, cur.last_modified) DESC, books.id DESC
             LIMIT ?"
        );
        let rows = sqlx::query(&sql).bind(limit).fetch_all(self.pool()).await?;
        rows.iter()
            .map(|row| {
                Ok((
                    row_to_book(row)?,
                    row_to_reading_prefixed(row, JOINED_READING_PREFIX)?,
                ))
            })
            .collect()
    }

    /// One page of the library's readings (item 43).
    ///
    /// Everything `readings` could answer before this was scoped to one
    /// `book_id`, except [`Storage::list_open_readings`], which is filtered to
    /// `finished_at IS NULL` — so a **finished** reading was reachable only by
    /// already knowing its book, and `ActivitySummary::books_finished` counted
    /// exactly the rows nothing could list. See
    /// [`ReadingQuery`](super::ReadingQuery) for the filter, the paging and the
    /// year, and [`reading_order_by`] for why every arm ends in `readings.id`.
    ///
    /// ## It drives from `readings`, and that is the whole of the plan
    ///
    /// `BOOK_FROM` drives from `books`, which is right for every other query in
    /// the repo and wrong for this one: `ORDER BY readings.finished_at` over a
    /// books-driven join is a sort of the whole library, and migration `0018`'s
    /// three indexes would never be reached. So the `FROM` is `readings JOIN
    /// books` plus the *same* current-reading join, shared verbatim through
    /// `book_current_join!` — a second spelling of "current" is the
    /// contradiction `BOOK_FROM`'s own doc describes.
    ///
    /// The book's four reading projections therefore describe the **current**
    /// read while `reading` is *this* one, which is not a slip: on a reread the
    /// two genuinely differ, and item 22 is the item that found a frontend
    /// printing one under the other's heading. `progress` is this reading's.
    ///
    /// ## The ordinal is a correlated subquery and must not be a window function
    ///
    /// `ROW_NUMBER() OVER (PARTITION BY book_id …)` is the obvious spelling and
    /// it is **wrong here**, because a window function is computed over the rows
    /// that survived the `WHERE`. A page filtered to one year, or to
    /// `open: false`, holds the second read of a book without holding the first
    /// — and would then number it `1`. The subqueries count over every sibling
    /// in the table, so the number is a fact about the book and not about the
    /// page; `the_read_number_survives_a_filter_that_hides_the_first_read` is
    /// the assertion, and it fails against the window-function version.
    ///
    /// ## The passage costs one extra statement, not one per row
    ///
    /// The list picks each row's passage *id* in a correlated subquery ordered
    /// by [`card_passage_order`] — item 44's rule, from the same function
    /// `card_passage` calls, so the wall and a single card cannot disagree — and
    /// the highlights themselves come back in one `IN (…)` fetch. Two
    /// statements a page, whatever the page size. Item 44 wrote down in advance
    /// that N `CardPassage` calls across a wall is "the pathology item 18 exists
    /// to remove"; a `card_passage` call per row inside this function would have
    /// been the same pathology moved below the seam where nobody counts it.
    pub async fn list_reading_rows(&self, query: &ReadingQuery) -> Result<Vec<ReadingRow>> {
        let predicate = query.filter.predicate();
        let sql = reading_rows_sql(query.sort, &predicate);
        let q = bind_all(sqlx::query(&sql), &predicate.binds)
            .bind(query.limit)
            .bind(query.offset.max(0));
        let rows = q.fetch_all(self.pool()).await?;

        let passage_ids: Vec<i64> = rows
            .iter()
            .filter_map(|r| r.try_get::<Option<i64>, _>("passage_id").ok().flatten())
            .collect();
        let mut passages: HashMap<i64, Highlight> = self
            .highlights_by_ids(&passage_ids)
            .await?
            .into_iter()
            .map(|h| (h.id, h))
            .collect();

        rows.iter()
            .map(|row| {
                let book = row_to_book(row)?;
                let reading = row_to_reading_prefixed(row, JOINED_READING_PREFIX)?;
                let progress = Progress::of_reading(&reading, book.page_count);
                let passage = row
                    .try_get::<Option<i64>, _>("passage_id")?
                    .and_then(|id| passages.remove(&id));
                Ok(ReadingRow {
                    book,
                    reading,
                    progress,
                    count: ReadCount::new(row.try_get("read_number")?, row.try_get("of_reads")?),
                    passage,
                })
            })
            .collect()
    }

    /// How many readings a [`ReadingFilter`](super::ReadingFilter) matches.
    ///
    /// **Its own call and not a field beside the rows** — item 18's ruling, and
    /// a wall of cards is the case it was made for: the count is a property of
    /// the filter, asked once per year the reader picks, while the page is asked
    /// on every scroll. The clause is the same one the page uses, from
    /// `ReadingFilter::predicate` and from nothing else.
    ///
    /// The `FROM` drops the current-reading join the page needs for its `Book`,
    /// and cannot change the answer by dropping it: that is a `LEFT JOIN` on an
    /// equality with a scalar subquery, so it matches at most one row and never
    /// zero. `JOIN books` is kept because `readings.book_id` is a NOT NULL
    /// foreign key, so it too cannot change the count — keeping it means the two
    /// statements differ in exactly one clause rather than in two.
    pub async fn count_readings(&self, filter: &ReadingFilter) -> Result<i64> {
        let predicate = filter.predicate();
        let where_sql = &predicate.sql;
        let sql = format!(
            "SELECT COUNT(*) FROM readings JOIN books ON books.id = readings.book_id {where_sql}"
        );
        let row = bind_all(sqlx::query(&sql), &predicate.binds)
            .fetch_one(self.pool())
            .await?;
        Ok(row.try_get::<i64, _>(0)?)
    }

    /// Which years the readings a filter matches actually **ended** in, newest
    /// first, and whether any of them has not ended (item 51).
    ///
    /// The question a year picker asks, and until this existed the only thing
    /// that could answer it was `activity_by_month` — which is a proxy and is
    /// wrong in five independent directions. `reading_events` gets a row when a
    /// read *started*, when a note was written, when a highlight carries a
    /// device date, when a device measured minutes, and it keeps the days a
    /// since-deleted reading explained; and it holds nothing at all until
    /// `rb activity --refill` has run. So a picker built on it offers a year in
    /// which nothing was finished, offers nothing on a library that never
    /// refilled, and cannot be narrowed to a book at all.
    ///
    /// It shares [`ReadingFilter::predicate`](super::ReadingFilter) with the
    /// page and the count **and nothing else**, which is what makes the picker
    /// and the wall agree by construction rather than by coincidence: every
    /// year offered has at least one row behind it under the same clause the
    /// wall will draw with.
    ///
    /// ## The open reading is a bucket, not a year
    ///
    /// A wall under `ReadingFilter::default()` holds open readings — deliberately,
    /// since gating on `finished_at` would tell a reader the book they are in
    /// has no card. Those rows are in no year, so a bare list of years does not
    /// partition the wall: picking every year in turn would never show them
    /// again with nothing on screen to say where they went. `strftime` of NULL
    /// is NULL, so they fall out of the same `GROUP BY` and come back as
    /// [`ReadingYears::open`] — a bool, because the picker needs to know
    /// whether the chip *exists*, and a number of books-in-progress sitting on
    /// a control is one decision away from the framing the axiom bans. The
    /// count for the chip is the same `count_readings` every other chip asks
    /// for, with `open: Some(true)`.
    ///
    /// Two degenerate cases follow and are worth stating rather than
    /// rediscovering: with `finished_in` set `open` is always false (a NULL
    /// fails both bare-column comparisons), and with `open: Some(true)` set
    /// `years` is always empty.
    pub async fn reading_years(&self, filter: &ReadingFilter) -> Result<ReadingYears> {
        let predicate = filter.predicate();
        let sql = reading_years_sql(&predicate);
        let rows = bind_all(sqlx::query(&sql), &predicate.binds)
            .fetch_all(self.pool())
            .await?;

        let mut years = Vec::with_capacity(rows.len());
        let mut open = false;
        for row in &rows {
            match row.try_get::<Option<i64>, _>("year")? {
                Some(y) => years.push(y as i32),
                None => open = true,
            }
        }
        Ok(ReadingYears { years, open })
    }

    /// Open a reading. Returns its id.
    ///
    /// A second open reading violates `idx_readings_one_open`, and that arrives
    /// here as a raw sqlx constraint error. It is translated to
    /// [`EngineError::InvalidInput`] because a caller branches on it — the CLI's
    /// `progress --reread` closes the open one first, and wants to say so rather
    /// than print a constraint message.
    pub async fn open_reading(
        &self,
        book_id: i64,
        started_at: Option<i64>,
        source: &str,
    ) -> Result<i64> {
        let now = now_unix();
        let row = sqlx::query(
            "INSERT INTO readings (book_id, started_at, status, source, created_at, last_modified)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(book_id)
        .bind(started_at)
        .bind(STATUS_READING)
        .bind(source)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => EngineError::InvalidInput(
                format!("book {book_id} already has an open reading; finish it first"),
            ),
            _ => EngineError::from(e),
        })?;
        Ok(row.try_get("id")?)
    }

    /// Record a reading that already happened, dates and status as given.
    ///
    /// The general form of [`Storage::open_reading`], and it exists because an
    /// *import* knows things `open_reading` cannot express: a reading that is
    /// already finished, and finished on a day that is not today. Goodreads'
    /// `Read Count` is the first caller; Calibre will be the second.
    ///
    /// `finished_at = None` still means open, so the same
    /// `idx_readings_one_open` violation surfaces here as
    /// [`EngineError::InvalidInput`] — one translation, one message, whichever
    /// door the second open reading came through.
    pub async fn record_reading(
        &self,
        book_id: i64,
        started_at: Option<i64>,
        finished_at: Option<i64>,
        status: &str,
        source: &str,
    ) -> Result<i64> {
        let now = now_unix();
        let row = sqlx::query(
            "INSERT INTO readings (book_id, started_at, finished_at, status, source,
                                   created_at, last_modified)
             VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(book_id)
        .bind(started_at)
        .bind(finished_at)
        .bind(status)
        .bind(source)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => EngineError::InvalidInput(
                format!("book {book_id} already has an open reading; finish it first"),
            ),
            _ => EngineError::from(e),
        })?;
        Ok(row.try_get("id")?)
    }

    /// Every reading of a book that came from one importer, oldest first.
    ///
    /// What makes a re-import idempotent: the question an importer has to ask
    /// is "how many of these did *I* record", not "how many are there" — a
    /// reading the user opened by hand is not one of ours to count against the
    /// far side's `Read Count`.
    pub async fn readings_from_source(&self, book_id: i64, source: &str) -> Result<Vec<Reading>> {
        let age = reading_age_key("readings");
        let sql = format!(
            "SELECT {READING_COLUMNS} FROM readings WHERE book_id = ? AND source = ?
             ORDER BY {age} ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(book_id)
            .bind(source)
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_reading).collect()
    }

    /// Close one reading at a date the caller knows. Returns whether anything
    /// changed.
    ///
    /// Beside [`Storage::finish_reading`], which closes *the open one* at
    /// `now()` — right for a person pressing a key, wrong for an import
    /// replaying a date from three years ago.
    pub async fn close_reading_at(
        &self,
        reading_id: i64,
        finished_at: i64,
        status: &str,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE readings SET finished_at = ?2, status = ?3, last_modified = ?4
             WHERE id = ?1 AND (finished_at IS NOT ?2 OR status IS NOT ?3)",
        )
        .bind(reading_id)
        .bind(finished_at)
        .bind(status)
        .bind(now_unix())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Close the open reading. Returns false when there was none.
    pub async fn finish_reading(&self, book_id: i64) -> Result<bool> {
        let now = now_unix();
        let done = sqlx::query(
            "UPDATE readings SET finished_at = ?2, status = ?3, last_modified = ?2
             WHERE book_id = ?1 AND finished_at IS NULL",
        )
        .bind(book_id)
        .bind(now)
        .bind(STATUS_FINISHED)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Mark the open reading abandoned **without closing it**.
    ///
    /// `finished_at` stays NULL on purpose: an abandoned book is one you might
    /// still pick up, and closing it would make resuming a *reread* rather than
    /// a continuation. It is the status that changed, not the reading.
    pub async fn abandon_reading(&self, book_id: i64) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE readings SET status = ?2, last_modified = ?3
             WHERE book_id = ?1 AND finished_at IS NULL AND status IS NOT ?2",
        )
        .bind(book_id)
        .bind(STATUS_ABANDONED)
        .bind(now_unix())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The current reading's id, opening one only when the book has **none at
    /// all**.
    ///
    /// Deliberately not "open one when none is *open*". A `complete` sidecar
    /// closes the reading it just wrote, so that rule would open a fresh reading
    /// on every subsequent import and the KOReader import — whose whole contract
    /// is idempotency — would grow the reading history without bound.
    ///
    /// Writing to a closed reading is the right answer for the caller that needs
    /// this: KOReader's sidecar is per-file and a reread appends to it, so the
    /// device cannot tell us a second reading has begun. Saying so is
    /// [`Storage::reread`]'s job, and it is the user's decision.
    pub async fn ensure_reading(
        &self,
        book_id: i64,
        started_at: Option<i64>,
        source: &str,
    ) -> Result<i64> {
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM readings WHERE book_id = ?
             ORDER BY (finished_at IS NULL) DESC, COALESCE(started_at, created_at) DESC, id DESC
             LIMIT 1",
        )
        .bind(book_id)
        .fetch_optional(self.pool())
        .await?;
        match current {
            Some(id) => Ok(id),
            None => self.open_reading(book_id, started_at, source).await,
        }
    }

    /// Close the open reading (if any) and open a fresh one. Returns its id.
    ///
    /// One statement pair rather than two calls from the CLI: between them the
    /// partial unique index is satisfied only in one order, and getting that
    /// order wrong is an error the caller would meet as a constraint violation.
    pub async fn reread(&self, book_id: i64) -> Result<i64> {
        self.finish_reading(book_id).await?;
        self.open_reading(book_id, Some(now_unix()), "manual").await
    }

    /// Write reading progress and return the book with its projections
    /// refreshed.
    ///
    /// `id` is the **book** id — the signature `books.rs` had, kept so its two
    /// callers (`cli/src/commands/book.rs`, `tui/src/app.rs`) change nothing at
    /// all.
    ///
    /// Opens a reading when none is open, *except* when the caller is clearing
    /// `finished`: the TUI's finished-toggle would otherwise leave a finished
    /// reading behind and start an empty new one every time it was pressed.
    /// Un-finishing reopens the most recent reading instead, which is what the
    /// user meant.
    ///
    /// **It also files the day in the activity log** (item 22). A typed page is
    /// the one piece of reading evidence readingbuddy fully originates, and
    /// before this the log knew only the day a read *opened* and the day it
    /// closed — so a reader who typed a page every evening for six weeks had one
    /// event for it. The write goes through
    /// [`Storage::record_typed_page`](super::Storage::record_typed_page), and it
    /// happens here rather than in a frontend for the reason `progress.rs` gives
    /// about derivation: two frontends each remembering to log would be two
    /// frontends that eventually disagree about whether today counted.
    pub async fn update_progress(
        &self,
        id: i64,
        page: Option<i64>,
        finished: Option<bool>,
    ) -> Result<Book> {
        if self.get_book(id).await?.is_none() {
            return Err(EngineError::NotFound(format!("book id {id}")));
        }
        let now = now_unix();

        let reading_id = match self.active_reading(id).await? {
            Some(r) => r.id,
            None if finished == Some(false) => {
                // Reopen rather than create. `finished_at IS NULL` is what makes
                // a reading open, so clearing it is the whole operation.
                let latest: Option<i64> = sqlx::query_scalar(
                    "SELECT id FROM readings WHERE book_id = ?
                     ORDER BY COALESCE(finished_at, started_at, created_at) DESC, id DESC LIMIT 1",
                )
                .bind(id)
                .fetch_optional(self.pool())
                .await?;
                match latest {
                    Some(rid) => rid,
                    None => self.open_reading(id, Some(now), "manual").await?,
                }
            }
            None => self.open_reading(id, Some(now), "manual").await?,
        };

        // Read before the write: the delta the activity log records is against
        // where the reading *was*, and one statement later that is gone.
        let previous_page: Option<i64> =
            sqlx::query_scalar("SELECT current_page FROM readings WHERE id = ?")
                .bind(reading_id)
                .fetch_optional(self.pool())
                .await?
                .flatten();

        sqlx::query(
            r#"UPDATE readings SET
                   current_page  = COALESCE(?2, current_page),
                   started_at    = COALESCE(started_at, ?3),
                   status        = CASE WHEN ?4 = 1 THEN 'finished'
                                        WHEN ?4 = 0 THEN 'reading'
                                        ELSE status END,
                   finished_at   = CASE WHEN ?4 = 1 THEN COALESCE(finished_at, ?3)
                                        WHEN ?4 = 0 THEN NULL
                                        ELSE finished_at END,
                   last_modified = ?3
               WHERE id = ?1"#,
        )
        .bind(reading_id)
        .bind(page)
        .bind(now)
        .bind(finished)
        .execute(self.pool())
        .await?;

        // The book's own `last_modified` is what the library list sorts on, and
        // reading it is why the user reaches for it.
        sqlx::query("UPDATE books SET last_modified = ?2 WHERE id = ?1")
            .bind(id)
            .bind(now)
            .execute(self.pool())
            .await?;

        // Item 22's fifth filler. Only a *page* files a day — toggling finished
        // says nothing about today's reading, and `fill_events_from_readings`
        // already owns the two endpoints of a read.
        if let Some(page) = page {
            self.record_typed_page(id, reading_id, previous_page, page, now)
                .await?;
        }

        self.get_book(id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {id}")))
    }

    /// Mirror the device's status/percent/rating onto the current reading.
    /// Returns whether anything actually changed.
    ///
    /// **Straight assignment, not `COALESCE`.** A sidecar is the complete state
    /// of that book on the device, so an absent rating means the user cleared
    /// it. The provider no-clobber pattern is right for partial records and
    /// wrong here — the same seam migration `0004` drew for `ko_note`.
    ///
    /// Writes nothing and reports false when the book has no reading at all;
    /// deciding to *open* one is the import's call, not this function's, because
    /// only the import knows what to start it at.
    pub async fn set_device_state(
        &self,
        book_id: i64,
        status: Option<&KoStatus>,
        percent: Option<f64>,
        rating: Option<i64>,
    ) -> Result<bool> {
        let sql = format!(
            "UPDATE readings SET ko_status = ?2, ko_percent = ?3, ko_rating = ?4,
                                 last_modified = ?5
             WHERE id = (SELECT id FROM readings WHERE book_id = ?1
                         ORDER BY (finished_at IS NULL) DESC,
                                  COALESCE(started_at, created_at) DESC, id DESC LIMIT 1)
               AND {DEVICE_STATE_DIFFER}"
        );
        let done = sqlx::query(&sql)
            .bind(book_id)
            .bind(status.map(|s| s.to_string()))
            .bind(percent)
            .bind(rating)
            .bind(now_unix())
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Would [`Storage::set_device_state`] change anything? Read-only, for a
    /// dry-run preview.
    pub async fn device_state_differs(
        &self,
        book_id: i64,
        status: Option<&KoStatus>,
        percent: Option<f64>,
        rating: Option<i64>,
    ) -> Result<bool> {
        let sql = format!(
            "SELECT count(*) FROM readings
             WHERE id = (SELECT id FROM readings WHERE book_id = ?1
                         ORDER BY (finished_at IS NULL) DESC,
                                  COALESCE(started_at, created_at) DESC, id DESC LIMIT 1)
               AND {DEVICE_STATE_DIFFER}"
        );
        let n: i64 = sqlx::query_scalar(&sql)
            .bind(book_id)
            .bind(status.map(|s| s.to_string()))
            .bind(percent)
            .bind(rating)
            .fetch_one(self.pool())
            .await?;
        Ok(n > 0)
    }

    /// Assign `highlights.reading_id` by matching each highlight's
    /// `ko_datetime` into a reading's window.
    ///
    /// Returns how many highlights of this book now carry a reading.
    ///
    /// **Leaving it `NULL` is correct** when no window contains it. KOReader's
    /// sidecar is per-file and a reread appends to the same file, so the device
    /// cannot supply this attribution and we must not invent it. Unattributed is
    /// reached from the book, which is why it needs no staging bucket.
    ///
    /// Recomputed from scratch rather than filled in: a highlight whose reading
    /// was deleted, or whose window moved, must lose its stale attribution too.
    ///
    /// ## A missing `started_at` is derived, not taken as −∞
    ///
    /// This is the whole substance of the query and it was wrong in the first
    /// cut, which `COALESCE`d an absent bound straight to ±8.64e12. That makes
    /// an unstarted reading's window *contain every earlier reading's window*,
    /// and since the match is "the latest window that holds it", the newest
    /// reading then takes the older readings' highlights and the older readings
    /// end up with none — permanently, and with nothing on screen looking
    /// wrong.
    ///
    /// Both ways in are ordinary rather than exotic. A **Goodreads** import
    /// writes `Read Count > 1` as several readings with NULL `started_at` by
    /// design (the CSV has no start date and `goodreads.rs` refuses to invent
    /// one), so *every* highlight landed on the most recent read. And any
    /// reading opened through [`Storage::record_reading`] or
    /// `open_reading(.., None, ..)` is unstarted, so a reread swallowed the
    /// first read's highlights too.
    ///
    /// So an absent `started_at` derives one: **an unstarted reading begins
    /// where the previous reading ended.** "Previous" is the latest
    /// `finished_at` of another reading of the same book that is not after this
    /// one's own end — which needs no reading-order convention, only the dates
    /// the rows already carry, and is exactly the bound the user would draw by
    /// hand. With no such reading it really is −∞, which is the one case the
    /// old `COALESCE` had right.
    ///
    /// `+ 1` makes that derived bound **exclusive**: reading 1 owns the instant
    /// it finished, and without it that one second lies in both windows and the
    /// tie is settled by the `ORDER BY` in the newer reading's favour. It is not
    /// an epsilon — these are integer unix seconds, so it is the next
    /// representable value. An *explicit* `started_at` stays inclusive: the user
    /// said the reading started then.
    ///
    /// The `ORDER BY` survives as a tie-break rather than as the deciding rule.
    /// Derived windows are disjoint by construction, but a user is free to give
    /// two readings explicitly overlapping dates, and the later window is the
    /// better guess for a highlight that falls in both.
    pub async fn attribute_highlights(&self, book_id: i64) -> Result<usize> {
        sqlx::query(&format!(
            "WITH windows AS ({READING_WINDOWS})
               UPDATE highlights SET reading_id = (
                   SELECT w.reading_id FROM windows w
                    WHERE CAST(strftime('%s', highlights.ko_datetime) AS INTEGER)
                          BETWEEN w.win_start AND w.win_end
                    ORDER BY w.win_start DESC, w.reading_id DESC
                    LIMIT 1)
               WHERE book_id = ?1"
        ))
        .bind(book_id)
        .execute(self.pool())
        .await?;

        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM highlights WHERE book_id = ? AND reading_id IS NOT NULL",
        )
        .bind(book_id)
        .fetch_one(self.pool())
        .await?;
        Ok(n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seeded() -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    page_count: Some(490),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        (s, id)
    }

    async fn add(s: &Storage, title: &str) -> i64 {
        s.upsert_book(
            &Book {
                title: Some(title.into()),
                page_count: Some(300),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap()
    }

    /// Hand-set the two stamps `list_open_readings` sorts on.
    ///
    /// They are unix *seconds*, so two writes in one test tie and the ordering
    /// would be decided by the tie-break instead of by the thing under test.
    /// `None` leaves that side alone, which is how the device-sync case (the
    /// reading moved, the book did not) is expressed.
    async fn touched_at(s: &Storage, book_id: i64, book: Option<i64>, reading: Option<i64>) {
        if let Some(t) = book {
            sqlx::query("UPDATE books SET last_modified = ? WHERE id = ?")
                .bind(t)
                .bind(book_id)
                .execute(s.pool())
                .await
                .unwrap();
        }
        if let Some(t) = reading {
            sqlx::query("UPDATE readings SET last_modified = ? WHERE book_id = ?")
                .bind(t)
                .bind(book_id)
                .execute(s.pool())
                .await
                .unwrap();
        }
    }

    /// What the home screen lists: books with a reading still open. A finished
    /// one drops off, a reread brings it back, and a book nobody has started is
    /// never there at all.
    #[tokio::test]
    async fn open_readings_are_the_ones_still_open() {
        let (s, pachinko) = seeded().await;
        let unread = add(&s, "Kokoro").await;

        assert!(
            s.list_open_readings(10).await.unwrap().is_empty(),
            "a library nobody has opened is not a currently-reading list"
        );

        s.update_progress(pachinko, Some(120), None).await.unwrap();
        let open = s.list_open_readings(10).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].0.id, Some(pachinko));
        assert_eq!(open[0].1.current_page, Some(120));
        assert!(
            !open.iter().any(|(b, _)| b.id == Some(unread)),
            "a book with no reading has nothing open"
        );

        s.update_progress(pachinko, None, Some(true)).await.unwrap();
        assert!(
            s.list_open_readings(10).await.unwrap().is_empty(),
            "finishing it takes it off the list"
        );

        // A reread is a new reading, and it is the new one that must come back
        // — not the finished one the book's projections would still show if the
        // filter had been written against `books` instead of `cur`.
        let second = s.reread(pachinko).await.unwrap();
        let open = s.list_open_readings(10).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].1.id, second);
        assert_eq!(open[0].1.current_page, None);
    }

    /// The anti-drift assertion. `Book`'s four progress fields are projections
    /// of the current reading, and this query returns the reading beside them —
    /// so if it ever grows a join of its own that resolves "current" differently,
    /// the two sides of one row will disagree here first.
    #[tokio::test]
    async fn the_projections_agree_with_the_reading_beside_them() {
        let (s, id) = seeded().await;
        // A finished reading first, so "current" and "open" are genuinely
        // different rows for this book and picking the wrong one is visible.
        s.update_progress(id, Some(490), Some(true)).await.unwrap();
        s.reread(id).await.unwrap();
        s.update_progress(id, Some(40), None).await.unwrap();

        let open = s.list_open_readings(10).await.unwrap();
        assert_eq!(open.len(), 1);
        let (book, reading) = &open[0];

        assert_eq!(book.current_page, reading.current_page);
        assert_eq!(book.date_started, reading.started_at);
        assert_eq!(book.date_finished, reading.finished_at);
        assert_eq!(book.finished, reading.status == STATUS_FINISHED);
        // And the same four values `get_book` reports, which is what every
        // other surface in the app renders from.
        let elsewhere = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(book.id, elsewhere.id);
        assert_eq!(book.current_page, elsewhere.current_page);
        assert_eq!(book.finished, elsewhere.finished);
        assert_eq!(book.date_started, elsewhere.date_started);
        assert_eq!(book.date_finished, elsewhere.date_finished);
    }

    /// Most-recently-touched first, and "touched" has to mean either stamp: a
    /// device sync writes the reading's `last_modified` and never the book's, so
    /// sorting on `books.last_modified` alone would leave a book you read on the
    /// reader this morning at the bottom of the list.
    #[tokio::test]
    async fn ordering_is_most_recently_touched() {
        let (s, first) = seeded().await;
        let second = add(&s, "Kokoro").await;
        let third = add(&s, "Snow Country").await;
        for id in [first, second, third] {
            s.update_progress(id, Some(10), None).await.unwrap();
        }

        touched_at(&s, first, Some(300), Some(300)).await;
        touched_at(&s, second, Some(100), Some(100)).await;
        touched_at(&s, third, Some(200), Some(200)).await;
        let order: Vec<i64> = s
            .list_open_readings(10)
            .await
            .unwrap()
            .iter()
            .map(|(b, _)| b.id.unwrap())
            .collect();
        assert_eq!(order, vec![first, third, second]);

        // The reading moves, the book does not. This is exactly what
        // `set_device_state` does.
        touched_at(&s, second, None, Some(900)).await;
        let order: Vec<i64> = s
            .list_open_readings(10)
            .await
            .unwrap()
            .iter()
            .map(|(b, _)| b.id.unwrap())
            .collect();
        assert_eq!(order, vec![second, first, third]);
    }

    /// An abandoned reading is still open — `abandon_reading` deliberately does
    /// not stamp `finished_at` — so it is listed, carrying the status that says
    /// so. Filtering it out here would make a book you might pick up unreachable
    /// from the one screen that lists what you are reading.
    #[tokio::test]
    async fn an_abandoned_reading_is_still_listed_and_says_so() {
        let (s, id) = seeded().await;
        s.update_progress(id, Some(60), None).await.unwrap();
        assert!(s.abandon_reading(id).await.unwrap());

        let open = s.list_open_readings(10).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].1.status, STATUS_ABANDONED);
    }

    /// The three states `finished: bool` cannot tell apart, told apart.
    ///
    /// `abandon_reading` leaves the reading open and stamps no `finished_at`, so
    /// an abandoned book and an open one are both `finished: false` with a
    /// `current_page` — and a book nobody has opened is `finished: false` too.
    /// Three different things behind one boolean is the gap `reading_status`
    /// closes, and it is what lets a frontend honour "abandoning a book is not
    /// failure" without deriving the state from three other columns.
    #[tokio::test]
    async fn the_projection_tells_abandoned_from_reading_from_never_opened() {
        let (s, reading) = seeded().await;
        let abandoned = add(&s, "Kokoro").await;
        let untouched = add(&s, "Season of Migration to the North").await;

        s.update_progress(reading, Some(60), None).await.unwrap();
        s.update_progress(abandoned, Some(20), None).await.unwrap();
        assert!(s.abandon_reading(abandoned).await.unwrap());

        let mut got = Vec::new();
        for id in [reading, abandoned, untouched] {
            let b = s
                .get_book(id)
                .await
                .unwrap()
                .expect("seeded book is stored");
            // The boolean all three share is still what it was, so nothing that
            // reads `finished` had to change.
            assert!(!b.finished);
            got.push(b.reading_status);
        }
        // The third is `None`, not a status string of its own: the book has no
        // reading at all, and an invented `"unread"` would be this layer
        // claiming a row exists.
        assert_eq!(
            got,
            vec![
                Some(STATUS_READING.to_string()),
                Some(STATUS_ABANDONED.to_string()),
                None
            ]
        );
    }

    #[tokio::test]
    async fn the_limit_is_honoured() {
        let (s, first) = seeded().await;
        let second = add(&s, "Kokoro").await;
        for id in [first, second] {
            s.update_progress(id, Some(10), None).await.unwrap();
        }
        assert_eq!(s.list_open_readings(1).await.unwrap().len(), 1);
        assert_eq!(s.list_open_readings(0).await.unwrap().len(), 0);
    }

    /// The invariant is an index, not a convention — and it must reach the
    /// caller as something branchable, not a raw sqlx constraint error.
    #[tokio::test]
    async fn the_index_refuses_a_second_open_reading() {
        let (s, id) = seeded().await;
        s.open_reading(id, Some(1), "manual").await.unwrap();
        assert!(matches!(
            s.open_reading(id, Some(2), "manual").await,
            Err(EngineError::InvalidInput(_))
        ));
        // Closing the first one is what makes room.
        assert!(s.finish_reading(id).await.unwrap());
        assert!(s.open_reading(id, Some(2), "manual").await.is_ok());
        assert_eq!(s.list_readings(id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn finishing_nothing_reports_nothing() {
        let (s, id) = seeded().await;
        assert!(!s.finish_reading(id).await.unwrap());
    }

    /// The reread flow end to end, through the projections the render layer
    /// actually reads.
    #[tokio::test]
    async fn a_reread_gets_its_own_reading_and_the_book_follows_it() {
        let (s, id) = seeded().await;
        s.update_progress(id, Some(200), None).await.unwrap();
        let b = s.update_progress(id, None, Some(true)).await.unwrap();
        assert!(b.finished);
        assert_eq!(b.current_page, Some(200));
        let first_finished = b.date_finished.expect("stamped");

        s.reread(id).await.unwrap();
        let b = s.update_progress(id, Some(30), None).await.unwrap();
        assert_eq!(b.current_page, Some(30), "the book follows the new reading");
        assert!(!b.finished);

        let readings = s.list_readings(id).await.unwrap();
        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].current_page, Some(200));
        assert_eq!(readings[0].finished_at, Some(first_finished));
        assert_eq!(readings[1].current_page, Some(30));
        assert_eq!(readings[1].finished_at, None);
    }

    /// The TUI's finished-toggle. Un-finishing must reopen the reading it just
    /// closed, not start a new one — otherwise pressing the key twice leaves two
    /// readings behind and the reading history fills with noise.
    #[tokio::test]
    async fn unfinishing_reopens_rather_than_starting_over() {
        let (s, id) = seeded().await;
        s.update_progress(id, Some(120), Some(true)).await.unwrap();
        let b = s.update_progress(id, None, Some(false)).await.unwrap();
        assert!(!b.finished);
        assert_eq!(b.current_page, Some(120), "progress survives the toggle");
        assert_eq!(b.date_finished, None);
        assert_eq!(s.list_readings(id).await.unwrap().len(), 1);
    }

    /// A book with no reading at all projects as untouched, which is what the
    /// library list renders for something merely added.
    #[tokio::test]
    async fn a_book_with_no_reading_projects_as_blank() {
        let (s, id) = seeded().await;
        let b = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(b.current_page, None);
        assert!(!b.finished);
        assert_eq!(b.date_started, None);
        assert_eq!(b.date_finished, None);
        assert!(s.list_readings(id).await.unwrap().is_empty());
    }

    /// The counter comes straight off this boolean, so it has to be exact in
    /// both directions — including the one `!=` would get wrong.
    #[tokio::test]
    async fn device_state_reports_only_a_real_change() {
        let (s, id) = seeded().await;
        s.open_reading(id, Some(1), "koreader").await.unwrap();

        let complete = KoStatus::Complete;
        assert!(
            s.device_state_differs(id, Some(&complete), Some(0.5), Some(4))
                .await
                .unwrap()
        );
        assert!(
            s.set_device_state(id, Some(&complete), Some(0.5), Some(4))
                .await
                .unwrap()
        );
        assert!(
            !s.set_device_state(id, Some(&complete), Some(0.5), Some(4))
                .await
                .unwrap(),
            "an identical sidecar is not an update"
        );
        assert!(
            !s.device_state_differs(id, Some(&complete), Some(0.5), Some(4))
                .await
                .unwrap(),
            "the preview must agree with the write"
        );

        let r = &s.list_readings(id).await.unwrap()[0];
        assert_eq!(r.ko_status.as_deref(), Some("complete"));
        assert_eq!(r.ko_percent, Some(0.5));
        assert_eq!(r.ko_rating, Some(4));

        // Cleared on the device means cleared here. `COALESCE` would make this
        // impossible to sync, permanently.
        assert!(s.set_device_state(id, None, None, None).await.unwrap());
        let r = &s.list_readings(id).await.unwrap()[0];
        assert_eq!(r.ko_rating, None);
        assert_eq!(r.ko_status, None);
    }

    #[tokio::test]
    async fn device_state_with_no_reading_writes_nothing() {
        let (s, id) = seeded().await;
        assert!(
            !s.set_device_state(id, Some(&KoStatus::Reading), Some(0.1), None)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn ko_datetime_parses_as_utc_and_rejects_junk() {
        assert_eq!(ko_datetime_to_unix("1970-01-01 00:00:01"), Some(1));
        assert_eq!(ko_datetime_to_unix("2026-01-05 21:14:08"), Some(1767647648));
        assert_eq!(ko_datetime_to_unix("not a date"), None);
        assert_eq!(ko_datetime_to_unix(""), None);
    }

    // ---- attribution ------------------------------------------------------

    use crate::storage::NewHighlight;

    fn hl(text: &str, when: &str) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: None,
            page: Some(1),
            pos0: Some(format!("/body/p[1]/text().{text}")),
            pos1: None,
            ko_datetime: Some(when.into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    async fn reading_of(s: &Storage, book_id: i64, text: &str) -> Option<i64> {
        sqlx::query_scalar("SELECT reading_id FROM highlights WHERE book_id = ? AND text = ?")
            .bind(book_id)
            .bind(text)
            .fetch_one(s.pool())
            .await
            .unwrap()
    }

    /// Two readings, three highlights, and one of them outside both windows.
    /// The out-of-window one stays `NULL` on purpose: the device cannot tell us
    /// which reading it belonged to, and guessing would be worse than saying so.
    #[tokio::test]
    async fn attribution_splits_by_window_and_leaves_the_rest_null() {
        let (s, id) = seeded().await;
        let first = s
            .open_reading(id, ko_datetime_to_unix("2024-01-01 00:00:00"), "manual")
            .await
            .unwrap();
        s.finish_reading(id).await.unwrap();
        sqlx::query("UPDATE readings SET finished_at = ? WHERE id = ?")
            .bind(ko_datetime_to_unix("2024-02-01 00:00:00"))
            .bind(first)
            .execute(s.pool())
            .await
            .unwrap();
        let second = s
            .open_reading(id, ko_datetime_to_unix("2026-01-01 00:00:00"), "manual")
            .await
            .unwrap();

        for h in [
            hl("in the first", "2024-01-15 09:00:00"),
            hl("in the second", "2026-03-02 09:00:00"),
            // Between the two readings: no window holds it.
            hl("in neither", "2025-06-01 09:00:00"),
        ] {
            s.insert_highlight(id, &h).await.unwrap();
        }

        assert_eq!(s.attribute_highlights(id).await.unwrap(), 2);
        assert_eq!(reading_of(&s, id, "in the first").await, Some(first));
        assert_eq!(reading_of(&s, id, "in the second").await, Some(second));
        assert_eq!(reading_of(&s, id, "in neither").await, None);
    }

    /// A stale attribution has to be dropped, not kept. A recompute that only
    /// ever *added* would leave a highlight pointing at a reading whose window
    /// no longer contains it, and nothing on screen would look wrong.
    #[tokio::test]
    async fn attribution_is_recomputed_not_accumulated() {
        let (s, id) = seeded().await;
        let r = s
            .open_reading(id, ko_datetime_to_unix("2024-01-01 00:00:00"), "manual")
            .await
            .unwrap();
        s.insert_highlight(id, &hl("inside", "2024-01-15 09:00:00"))
            .await
            .unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 1);

        // The window moves out from under it.
        sqlx::query("UPDATE readings SET started_at = ? WHERE id = ?")
            .bind(ko_datetime_to_unix("2025-01-01 00:00:00"))
            .bind(r)
            .execute(s.pool())
            .await
            .unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 0);
        assert_eq!(reading_of(&s, id, "inside").await, None);
    }

    /// The Goodreads shape: several readings, none with a start date, each
    /// closed at the `Date Read` its row carried.
    ///
    /// This is the case the first cut got wrong, and it is not a corner — it is
    /// what *every* `Read Count > 1` row imports as, because the CSV has no
    /// start date and `goodreads.rs` refuses to invent one. Taking an absent
    /// `started_at` as −∞ makes the newest reading's window contain both older
    /// ones, so both highlights landed on the last read and the first two
    /// readings held none.
    #[tokio::test]
    async fn unstarted_readings_do_not_swallow_the_earlier_ones_highlights() {
        let (s, id) = seeded().await;
        let first = s
            .record_reading(
                id,
                None,
                ko_datetime_to_unix("2020-02-01 00:00:00"),
                STATUS_FINISHED,
                "goodreads",
            )
            .await
            .unwrap();
        let second = s
            .record_reading(
                id,
                None,
                ko_datetime_to_unix("2023-02-01 00:00:00"),
                STATUS_FINISHED,
                "goodreads",
            )
            .await
            .unwrap();
        let third = s
            .record_reading(
                id,
                None,
                ko_datetime_to_unix("2026-02-01 00:00:00"),
                STATUS_FINISHED,
                "goodreads",
            )
            .await
            .unwrap();

        for h in [
            hl("read once", "2020-01-15 09:00:00"),
            hl("read twice", "2023-01-15 09:00:00"),
            hl("read thrice", "2026-01-15 09:00:00"),
        ] {
            s.insert_highlight(id, &h).await.unwrap();
        }
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 3);

        assert_eq!(reading_of(&s, id, "read once").await, Some(first));
        assert_eq!(reading_of(&s, id, "read twice").await, Some(second));
        assert_eq!(reading_of(&s, id, "read thrice").await, Some(third));
    }

    /// The same defect through the other door: a reread whose reading was
    /// opened without a start date.
    ///
    /// An open reading has no `finished_at` either, so its window was −∞..+∞ and
    /// it took the finished reading's highlights along with its own. The derived
    /// bound is what stops it, and the assertion that matters is the *first*
    /// read keeping what it already had.
    #[tokio::test]
    async fn an_unstarted_reread_leaves_the_first_reads_highlights_alone() {
        let (s, id) = seeded().await;
        let first = s
            .record_reading(
                id,
                ko_datetime_to_unix("2020-01-01 00:00:00"),
                ko_datetime_to_unix("2020-02-01 00:00:00"),
                STATUS_FINISHED,
                "manual",
            )
            .await
            .unwrap();
        s.insert_highlight(id, &hl("first read", "2020-01-15 09:00:00"))
            .await
            .unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 1);
        assert_eq!(reading_of(&s, id, "first read").await, Some(first));

        // Open, and with nothing to derive a start from — what
        // `open_reading(.., None, ..)` gives, and what a sidecar with no usable
        // datetimes reaches through `ensure_reading`.
        let second = s.open_reading(id, None, "koreader").await.unwrap();
        s.insert_highlight(id, &hl("second read", "2026-03-01 09:00:00"))
            .await
            .unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 2);

        assert_eq!(
            reading_of(&s, id, "first read").await,
            Some(first),
            "opening a reread must not move the first read's highlights onto it"
        );
        assert_eq!(reading_of(&s, id, "second read").await, Some(second));
    }

    /// The derived bound is exclusive; an explicit one is inclusive.
    ///
    /// A highlight captured at the exact second a reading closed belongs to that
    /// reading. Without the `+ 1` that instant lies in both windows and the
    /// `ORDER BY` hands it to the newer one — a one-second hole that only ever
    /// shows up on real device data, where a highlight and a "finished" tap land
    /// in the same second often enough to matter.
    #[tokio::test]
    async fn the_derived_start_is_exclusive_and_an_explicit_one_is_not() {
        let (s, id) = seeded().await;
        let closed_at = ko_datetime_to_unix("2020-02-01 00:00:00");
        let first = s
            .record_reading(
                id,
                ko_datetime_to_unix("2020-01-01 00:00:00"),
                closed_at,
                STATUS_FINISHED,
                "manual",
            )
            .await
            .unwrap();
        s.open_reading(id, None, "manual").await.unwrap();
        s.insert_highlight(id, &hl("on the boundary", "2020-02-01 00:00:00"))
            .await
            .unwrap();
        s.attribute_highlights(id).await.unwrap();
        assert_eq!(
            reading_of(&s, id, "on the boundary").await,
            Some(first),
            "a reading owns the instant it finished; the derived start is the second after"
        );

        // Said explicitly, the same instant is the *later* reading's start, and
        // the later window wins. The user's own dates are taken at face value.
        let (s, id) = seeded().await;
        s.record_reading(
            id,
            ko_datetime_to_unix("2020-01-01 00:00:00"),
            closed_at,
            STATUS_FINISHED,
            "manual",
        )
        .await
        .unwrap();
        let second = s.open_reading(id, closed_at, "manual").await.unwrap();
        s.insert_highlight(id, &hl("on the boundary", "2020-02-01 00:00:00"))
            .await
            .unwrap();
        s.attribute_highlights(id).await.unwrap();
        assert_eq!(reading_of(&s, id, "on the boundary").await, Some(second));
    }

    // ---- the library-wide list (item 43) ----------------------------------

    use crate::book::ReadingState;
    use crate::storage::DayRange;

    const EVERY_SORT: [ReadingSort; 3] = [
        ReadingSort::Finished,
        ReadingSort::Started,
        ReadingSort::LastModified,
    ];

    /// The outer query's own plan lines: `EXPLAIN QUERY PLAN` rows whose
    /// `parent` is 0.
    ///
    /// The filter matters for the same reason it does in [`super::super::books`]
    /// — the current-reading join carries a correlated subquery with an
    /// `ORDER BY` of its own, and so do the two ordinal counts and the passage
    /// pick, so asserting over the whole tree would fail on lines that have
    /// nothing to do with the sort key.
    async fn outer_plan(s: &Storage, sql: &str) -> Vec<String> {
        sqlx::query(&format!("EXPLAIN QUERY PLAN {sql}"))
            .fetch_all(s.pool())
            .await
            .expect("query plan")
            .iter()
            .filter(|r| r.try_get::<i64, _>("parent").unwrap_or(-1) == 0)
            .map(|r| r.try_get::<String, _>("detail").unwrap_or_default())
            .collect()
    }

    fn plan_of(sort: ReadingSort, filter: ReadingFilter) -> String {
        reading_rows_sql(sort, &filter.predicate())
    }

    /// **Migration `0018`'s only claim, asserted: the planner reaches the
    /// index.**
    ///
    /// A behavioural test cannot see an index — every one of these sorts
    /// returns the rows it returned the day before the migration. This is
    /// `0008`'s rule applied to a second table, and it reads the SQL
    /// `list_reading_rows` actually issues, through the same
    /// [`reading_rows_sql`] the method calls, because the likeliest failure of
    /// an index migration is a *mismatch* between the clause and the index — a
    /// `COALESCE` spelled differently, a column left bare — and an approximate
    /// rebuild is exactly what cannot catch that.
    ///
    /// Two assertions per sort, and the second is the one with teeth: the index
    /// is named, **and** the outer query no longer says `USE TEMP B-TREE FOR
    /// ORDER BY`. An index scanned for its rows and then sorted anyway bought
    /// nothing.
    #[tokio::test]
    async fn the_reading_sort_indexes_are_the_plan_the_planner_picks() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        for (sort, index) in [
            (ReadingSort::Finished, "idx_readings_finished_at"),
            (ReadingSort::Started, "idx_readings_started"),
            (ReadingSort::LastModified, "idx_readings_last_modified"),
        ] {
            let plan = outer_plan(&s, &plan_of(sort, ReadingFilter::default()))
                .await
                .join("; ");
            assert!(
                plan.contains(index),
                "{sort:?} must scan {index}, got: {plan}"
            );
            assert!(
                !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
                "{sort:?} reached {index} and sorted anyway, which buys nothing: {plan}"
            );
        }
    }

    /// The control that makes the assertion above evidence rather than
    /// ceremony, and the reason decision 3 of this item exists.
    ///
    /// `books` has `BookSort::Progress` as a live control — a sort no index can
    /// serve. Every arm of `ReadingSort` *is* indexed, so the control has to be
    /// built: this is the year filter written the way `activity_summary` used to
    /// write it, `date(finished_at, 'unixepoch') BETWEEN ? AND ?`, which is an
    /// expression over the column. SQLite reads it correctly and declines the
    /// index in silence — the same silence `0008` found for a mismatched
    /// collation — so the plan falls back to a full scan. If this ever passes,
    /// the bare-column rewrite next door has stopped being the thing that buys
    /// the index.
    #[tokio::test]
    async fn a_year_filter_over_an_expression_loses_the_index() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let honest = plan_of(
            ReadingSort::LastModified,
            ReadingFilter {
                finished_in: Some(DayRange::new("2025-01-01", "2025-12-31").unwrap()),
                ..Default::default()
            },
        );
        let plan = outer_plan(&s, &honest).await.join("; ");
        assert!(
            plan.contains("idx_readings_finished_at"),
            "the bare-column year filter must reach the index: {plan}"
        );

        // The same question, spelled over an expression.
        let naive = honest.replace(
            "readings.finished_at >= ? AND readings.finished_at < ?",
            "date(readings.finished_at, 'unixepoch') BETWEEN ? AND ?",
        );
        assert_ne!(naive, honest, "the substitution must have bitten");
        let plan = outer_plan(&s, &naive).await.join("; ");
        assert!(
            !plan.contains("idx_readings_finished_at"),
            "an index that serves a function of its own column would make \
             decision 3 of item 43 unnecessary; it does not: {plan}"
        );
    }

    /// Every arm orders by something unique, so a page is the successor of the
    /// one before it by construction rather than by the planner's current mood.
    ///
    /// Structural on purpose. Item 18 measured that the behavioural partition
    /// test stays green with the tie-break deleted, because SQLite's sorter is
    /// deterministic for one plan over one set of rows — a guarantee that
    /// belongs to the plan and not to the schema.
    #[test]
    fn the_reading_order_is_a_total_order() {
        for sort in EVERY_SORT {
            let clause = reading_order_by(sort);
            assert!(
                clause.trim_end().ends_with("readings.id ASC")
                    || clause.trim_end().ends_with("readings.id DESC"),
                "{sort:?} orders by `{clause}`, which ties — and a tie under \
                 LIMIT/OFFSET is a reading on two pages and another on none"
            );
        }
    }

    /// `ReadingSort::Started` orders by the same expression `list_readings`
    /// counts in, and the index is declared on.
    ///
    /// Three spellings of "when this read began" would otherwise be free to
    /// drift: the sort's literal, the key the ordinal's row values compare, and
    /// the migration's index. The first two are pinned here; the third is pinned
    /// by the plan test, which fails the moment the index stops matching.
    #[test]
    fn the_started_sort_orders_by_the_key_list_readings_counts_in() {
        let began = reading_began("readings");
        let clause = reading_order_by(ReadingSort::Started);
        assert!(
            clause.starts_with(&format!("{began} DESC")),
            "`{clause}` does not lead with `{began}`"
        );
        assert!(
            reading_age_key("readings").starts_with(&began),
            "the ordinal counts in a different expression from the sort"
        );
    }

    /// The year filter names the **bare column**, and that is a decision rather
    /// than an implementation detail — see `a_year_filter_over_an_expression_
    /// loses_the_index` for the measurement behind it.
    #[test]
    fn the_year_filter_compares_the_bare_column() {
        let p = ReadingFilter {
            finished_in: Some(DayRange::new("2025-01-01", "2025-12-31").unwrap()),
            ..Default::default()
        }
        .predicate();
        assert!(
            !p.sql.contains("date(") && !p.sql.contains("strftime"),
            "a function over the column is an index SQLite declines: {}",
            p.sql
        );
        assert_eq!(
            p.binds,
            vec![
                super::super::query::Bind::Int(1735689600), // 2025-01-01T00:00:00Z
                super::super::query::Bind::Int(1767225600), // 2026-01-01T00:00:00Z, exclusive
            ]
        );
    }

    #[test]
    fn an_empty_reading_filter_writes_no_clause() {
        let p = ReadingFilter::default().predicate();
        assert!(p.sql.is_empty());
        assert!(p.binds.is_empty());
        assert!(ReadingFilter::default().is_empty());
        let q = ReadingQuery::default();
        assert!(q.limit < 0, "a negative limit is SQLite's own `no limit`");
        assert_eq!(q.offset, 0);
        assert_eq!(q.sort, ReadingSort::Finished);
    }

    /// A shelf of readings to ask questions of: three books, five readings, two
    /// of them of one book, spread across two years.
    async fn wall(s: &Storage) -> (i64, i64, i64) {
        let a = add(s, "Piranesi").await;
        let b = add(s, "Kokoro").await;
        let c = add(s, "Snow Country").await;
        // Piranesi twice: 2024 and 2025. The reread is the whole reason the read
        // number exists.
        s.record_reading(
            a,
            day("2024-01-01"),
            day("2024-02-01"),
            STATUS_FINISHED,
            "manual",
        )
        .await
        .unwrap();
        s.record_reading(
            a,
            day("2025-03-01"),
            day("2025-04-01"),
            STATUS_FINISHED,
            "manual",
        )
        .await
        .unwrap();
        s.record_reading(
            b,
            day("2025-05-01"),
            day("2025-06-01"),
            STATUS_FINISHED,
            "manual",
        )
        .await
        .unwrap();
        s.record_reading(
            c,
            day("2024-07-01"),
            day("2024-08-01"),
            STATUS_FINISHED,
            "goodreads",
        )
        .await
        .unwrap();
        // One still open, so `finished_at IS NULL` is exercised everywhere.
        s.open_reading(c, day("2026-01-01"), "manual")
            .await
            .unwrap();
        (a, b, c)
    }

    fn day(d: &str) -> Option<i64> {
        ko_datetime_to_unix(&format!("{d} 00:00:00"))
    }

    fn year(y: &str) -> DayRange {
        DayRange::new(&format!("{y}-01-01"), &format!("{y}-12-31")).unwrap()
    }

    /// A count and a page are the same question asked two ways, which is the
    /// whole reason one function writes the clause — and the year filter has to
    /// compose with the count like every other one, or the wall's page numbers
    /// are about a different set than its rows.
    #[tokio::test]
    async fn the_reading_count_agrees_with_the_page_for_every_filter() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let (a, _, _) = wall(&s).await;

        for filter in [
            ReadingFilter::default(),
            ReadingFilter {
                book_id: Some(a),
                ..Default::default()
            },
            ReadingFilter {
                open: Some(true),
                ..Default::default()
            },
            ReadingFilter {
                open: Some(false),
                ..Default::default()
            },
            ReadingFilter {
                status: Some(ReadingState::Finished),
                ..Default::default()
            },
            ReadingFilter {
                status: Some(ReadingState::Reading),
                ..Default::default()
            },
            ReadingFilter {
                finished_in: Some(year("2024")),
                ..Default::default()
            },
            ReadingFilter {
                finished_in: Some(year("2025")),
                ..Default::default()
            },
            // The year composing with a second predicate, which is the case a
            // count written beside the page rather than from it gets wrong.
            ReadingFilter {
                finished_in: Some(year("2025")),
                book_id: Some(a),
                ..Default::default()
            },
            ReadingFilter {
                finished_in: Some(year("2030")),
                ..Default::default()
            },
        ] {
            let count = s.count_readings(&filter).await.unwrap();
            for sort in EVERY_SORT {
                let rows = s
                    .list_reading_rows(&ReadingQuery::new(-1, sort).with_filter(filter.clone()))
                    .await
                    .unwrap();
                assert_eq!(
                    rows.len() as i64,
                    count,
                    "{filter:?} counted {count} and listed {} under {sort:?}",
                    rows.len()
                );
            }
        }
    }

    /// A page and its successor partition the filtered list, and the count
    /// agrees with a full page-walk. The year is the filter under test because
    /// it is the one this item exists for.
    #[tokio::test]
    async fn a_page_and_its_successor_partition_the_filtered_list() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        wall(&s).await;
        let filter = ReadingFilter {
            open: Some(false),
            ..Default::default()
        };
        let count = s.count_readings(&filter).await.unwrap();
        assert_eq!(count, 4, "four closed readings on this shelf");

        for sort in EVERY_SORT {
            let query = ReadingQuery::new(2, sort).with_filter(filter.clone());
            let mut walked: Vec<i64> = Vec::new();
            let mut offset = 0;
            loop {
                let page = s
                    .list_reading_rows(&query.clone().at_offset(offset))
                    .await
                    .unwrap();
                if page.is_empty() {
                    break;
                }
                walked.extend(page.iter().map(|r| r.reading.id));
                offset += 2;
            }
            let whole: Vec<i64> = s
                .list_reading_rows(&ReadingQuery::new(-1, sort).with_filter(filter.clone()))
                .await
                .unwrap()
                .iter()
                .map(|r| r.reading.id)
                .collect();
            assert_eq!(walked, whole, "the pages do not reassemble under {sort:?}");
            assert_eq!(walked.len() as i64, count);
        }
    }

    /// **The whole of item 41**: the number this list carries is the number the
    /// TUI's gutter shows for the same book.
    ///
    /// That agreement is the reason the rule left the frontend at all —
    /// `ReadNumbering` silently depends on `list_readings`' oldest-first
    /// ordering, so a second frontend counting off a differently-ordered list
    /// would disagree with `rb show` about which read a highlight came from with
    /// nothing on either screen looking wrong.
    #[tokio::test]
    async fn the_read_number_agrees_with_the_gutter() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let (a, b, _) = wall(&s).await;

        for book in [a, b] {
            let readings = s.list_readings(book).await.unwrap();
            let numbering = ReadNumbering::new(&readings);
            let rows = s
                .list_reading_rows(&ReadingQuery::default().with_filter(ReadingFilter {
                    book_id: Some(book),
                    ..Default::default()
                }))
                .await
                .unwrap();
            assert_eq!(rows.len(), readings.len());
            for row in &rows {
                assert_eq!(
                    row.count.of as usize,
                    readings.len(),
                    "of_reads counts the book's readings"
                );
                assert_eq!(
                    row.count.ordinal().map(|n| n as usize),
                    numbering.number_of(Some(row.reading.id)),
                    "the wall and the gutter number reading {} differently",
                    row.reading.id
                );
                assert_eq!(row.count.tells_reads_apart(), numbering.is_meaningful());
            }
        }
        // And the shape of the answer: a book read twice numbers 1 and 2 in
        // `list_readings` order, a book read once has no ordinal to show.
        let twice = s
            .list_reading_rows(&ReadingQuery::new(-1, ReadingSort::Started).with_filter(
                ReadingFilter {
                    book_id: Some(a),
                    ..Default::default()
                },
            ))
            .await
            .unwrap();
        // `Started` is newest first, so the second read leads.
        assert_eq!(
            twice.iter().map(|r| r.count.number).collect::<Vec<_>>(),
            vec![2, 1]
        );
        let once = s
            .list_reading_rows(&ReadingQuery::default().with_filter(ReadingFilter {
                book_id: Some(b),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(once[0].count.ordinal(), None, "a lone read has no number");
    }

    /// The number is counted over the **book**, never over the page — which is
    /// what forbids the window function the obvious implementation reaches for.
    ///
    /// `ROW_NUMBER() OVER (PARTITION BY book_id …)` is computed after the
    /// `WHERE`, so a page filtered to 2025 holds *Piranesi*'s second read
    /// without its first and would call it read 1. There is nothing on a card
    /// saying "your first read" that looks wrong.
    #[tokio::test]
    async fn the_read_number_survives_a_filter_that_hides_the_first_read() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let (a, _, _) = wall(&s).await;
        let rows = s
            .list_reading_rows(&ReadingQuery::default().with_filter(ReadingFilter {
                book_id: Some(a),
                finished_in: Some(year("2025")),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "only the 2025 read is in this year");
        assert_eq!(rows[0].count.number, 2, "it is still the second read");
        assert_eq!(rows[0].count.of, 2);
        assert_eq!(rows[0].count.ordinal(), Some(2));
    }

    /// The wall and the single card show the **same** passage for one reading,
    /// which is item 44's rule surviving its second caller.
    #[tokio::test]
    async fn the_wall_and_the_single_card_choose_the_same_passage() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let (a, _, _) = wall(&s).await;
        for (text, when) in [
            ("short", "2024-01-15 09:00:00"),
            (
                "the long one that a card should show, dragged across a whole paragraph",
                "2024-01-16 09:00:00",
            ),
            ("also short", "2025-03-15 09:00:00"),
        ] {
            s.insert_highlight(a, &hl(text, when)).await.unwrap();
        }
        s.attribute_highlights(a).await.unwrap();

        let rows = s
            .list_reading_rows(&ReadingQuery::default().with_filter(ReadingFilter {
                book_id: Some(a),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            let alone = s.card_passage(row.reading.id).await.unwrap();
            assert_eq!(
                row.passage.as_ref().map(|h| h.id),
                alone.as_ref().map(|h| h.id),
                "reading {} shows one passage on the wall and another on its card",
                row.reading.id
            );
        }
        // And it is the long one, not the first one — the rule, not the order.
        let first_read = rows.iter().find(|r| r.count.number == 1).unwrap();
        assert!(
            first_read
                .passage
                .as_ref()
                .is_some_and(|h| h.text.starts_with("the long one")),
            "the passage is the longest mark of the reading"
        );

        // A reading whose highlights are all unattributed has no passage, the
        // way `highlights_for_reading` returns an empty list.
        let rows = s.list_reading_rows(&ReadingQuery::default()).await.unwrap();
        assert!(
            rows.iter().any(|r| r.passage.is_none()),
            "a reading with no marks must report the absence as an absence"
        );
    }

    /// The row carries **this** reading's progress, not the book's — item 22's
    /// finding, which a list of rereads is exactly where it bites.
    #[tokio::test]
    async fn the_row_progresses_by_its_own_reading() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        // The book that has one finished read and one still open: two reads of
        // one book whose progress genuinely differs, which is what a wall of
        // rereads shows side by side.
        let (_, _, c) = wall(&s).await;
        let readings = s.list_readings(c).await.unwrap();
        sqlx::query("UPDATE readings SET current_page = ? WHERE id = ?")
            .bind(300i64)
            .bind(readings[0].id)
            .execute(s.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE readings SET current_page = ? WHERE id = ?")
            .bind(30i64)
            .bind(readings[1].id)
            .execute(s.pool())
            .await
            .unwrap();

        let rows = s
            .list_reading_rows(&ReadingQuery::default().with_filter(ReadingFilter {
                book_id: Some(c),
                ..Default::default()
            }))
            .await
            .unwrap();
        for row in &rows {
            let own = Progress::of_reading(&row.reading, row.book.page_count);
            assert_eq!(row.progress, own);
        }
        assert_ne!(
            rows[0].progress, rows[1].progress,
            "two reads of one book must be able to report different progress"
        );
    }

    /// A highlight with no `ko_datetime` cannot be placed at all.
    #[tokio::test]
    async fn a_highlight_without_a_datetime_stays_unattributed() {
        let (s, id) = seeded().await;
        s.open_reading(id, Some(0), "manual").await.unwrap();
        let mut h = hl("undated", "");
        h.ko_datetime = None;
        s.insert_highlight(id, &h).await.unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 0);
    }

    // ---- the years the wall has (item 51) -----------------------------------

    /// The picker's whole claim: a year is offered because a read **ended** in
    /// it, and the open read is a bucket beside the years rather than one of
    /// them.
    #[tokio::test]
    async fn the_years_are_the_years_reads_ended_in_newest_first() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        wall(&s).await;
        let y = s.reading_years(&ReadingFilter::default()).await.unwrap();
        assert_eq!(y.years, vec![2025, 2024], "newest first, one entry a year");
        assert!(
            y.open,
            "an open read is in no year, and a picker that did not say so \
             would offer years that do not add up to the wall"
        );
    }

    /// **The years and the wall are the same set, filter by filter.**
    ///
    /// This is what "agree by construction" has to mean in a test: every year
    /// the picker offers, asked for as a `finished_in`, returns rows — and the
    /// years plus the open bucket account for **every** row the unfiltered wall
    /// holds. A picker derived from `activity_by_month` fails the second half
    /// on any library where a note was written in a year nothing was finished.
    #[tokio::test]
    async fn every_year_offered_has_rows_and_together_they_are_the_whole_wall() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let (a, _, _) = wall(&s).await;

        for filter in [
            ReadingFilter::default(),
            ReadingFilter {
                book_id: Some(a),
                ..Default::default()
            },
            ReadingFilter {
                status: Some(ReadingState::Finished),
                ..Default::default()
            },
        ] {
            let years = s.reading_years(&filter).await.unwrap();
            let total = s.count_readings(&filter).await.unwrap();

            let mut counted = 0;
            for y in &years.years {
                let scoped = ReadingFilter {
                    finished_in: Some(year(&y.to_string())),
                    ..filter.clone()
                };
                let n = s.count_readings(&scoped).await.unwrap();
                assert!(n > 0, "{y} was offered and holds nothing under {filter:?}");
                counted += n;
            }
            if years.open {
                counted += s
                    .count_readings(&ReadingFilter {
                        open: Some(true),
                        ..filter.clone()
                    })
                    .await
                    .unwrap();
            }
            assert_eq!(
                counted, total,
                "the picker must partition the wall under {filter:?}"
            );
        }
    }

    /// The two degenerate cases, stated so neither is rediscovered as a bug.
    #[tokio::test]
    async fn a_year_filter_answers_no_open_and_an_open_filter_answers_no_years() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        wall(&s).await;

        // A NULL `finished_at` fails both bare-column comparisons, so a year
        // can never contain the open read.
        let in_2025 = s
            .reading_years(&ReadingFilter {
                finished_in: Some(year("2025")),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(in_2025.years, vec![2025]);
        assert!(!in_2025.open);

        let open = s
            .reading_years(&ReadingFilter {
                open: Some(true),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(open.years.is_empty());
        assert!(open.open);
    }

    /// An empty library offers nothing, and that is an empty answer rather than
    /// a year of zero.
    #[tokio::test]
    async fn a_library_with_no_readings_offers_no_years() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        add(&s, "Unread").await;
        assert_eq!(
            s.reading_years(&ReadingFilter::default()).await.unwrap(),
            ReadingYears::default()
        );
    }

    /// **The year is UTC, and the boundary is the same instant `finished_in`
    /// binds.**
    ///
    /// `strftime('%Y', …, 'unixepoch')` and `DayRange::unix_bounds` are two
    /// different conversions of one idea, and a disagreement of one second at
    /// New Year would file a read under one year in the picker and the other in
    /// the wall. Asserted on the second either side of a boundary rather than
    /// on a comfortable date in June.
    #[tokio::test]
    async fn the_year_boundary_is_the_instant_the_year_filter_binds() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let b = add(&s, "New Year").await;
        // 2025-12-31T23:59:59Z and 2026-01-01T00:00:00Z.
        s.record_reading(b, None, Some(1_767_225_599), STATUS_FINISHED, "manual")
            .await
            .unwrap();
        let c = add(&s, "New Year, Later").await;
        s.record_reading(c, None, Some(1_767_225_600), STATUS_FINISHED, "manual")
            .await
            .unwrap();

        let y = s.reading_years(&ReadingFilter::default()).await.unwrap();
        assert_eq!(y.years, vec![2026, 2025]);
        for (year_of, expected) in [("2025", 1), ("2026", 1)] {
            let n = s
                .count_readings(&ReadingFilter {
                    finished_in: Some(year(year_of)),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(n, expected, "{year_of} must hold what the picker offered");
        }
    }

    /// **Migration `0018`'s index has a fourth job, and the join that would
    /// lose it is the control.**
    ///
    /// `count_readings` keeps `JOIN books` deliberately, so that it and the
    /// page differ in one clause. Copying that here reads correctly, runs, and
    /// silently turns a covering-index scan into a scan of `books` — `0008`'s
    /// genre of failure, which is why the control is spelled out rather than
    /// trusted to review.
    #[tokio::test]
    async fn the_year_list_is_covered_by_the_finished_index() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let sql = reading_years_sql(&ReadingFilter::default().predicate());
        let plan = outer_plan(&s, &sql).await.join("; ");
        assert!(
            plan.contains("COVERING INDEX idx_readings_finished_at"),
            "the year list must never touch the table: {plan}"
        );

        // The control: the same question with `count_readings`' join.
        let joined = sql.replace(
            "FROM readings ",
            "FROM readings JOIN books ON books.id = readings.book_id ",
        );
        assert_ne!(joined, sql, "the substitution must have bitten");
        let plan = outer_plan(&s, &joined).await.join("; ");
        assert!(
            !plan.contains("COVERING INDEX idx_readings_finished_at"),
            "if the join is free, this assertion is measuring nothing: {plan}"
        );
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// However many times a book is finished and reopened, there is never
        /// more than one open reading and the book's projections come from the
        /// newest one.
        ///
        /// A property rather than more examples because the rule is general and
        /// the interesting input is the *sequence*: the ordering between
        /// `finish`, `reread` and a bare page update is exactly what decides
        /// which reading a write lands on, and three hand-picked sequences are
        /// three that happened to work.
        #[test]
        fn a_book_never_has_two_open_readings(
            steps in proptest::collection::vec(0u8..3, 1..12),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let s = Storage::connect("sqlite::memory:").await.unwrap();
                let id = s.upsert_book(&Book { title: Some("t".into()), ..Default::default() }, None)
                    .await.unwrap();

                let mut page = 0i64;
                for step in &steps {
                    match step {
                        0 => { page += 10; s.update_progress(id, Some(page), None).await.unwrap(); }
                        1 => { s.update_progress(id, None, Some(true)).await.unwrap(); }
                        _ => { s.reread(id).await.unwrap(); page = 0; }
                    }
                    let open: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM readings WHERE book_id = ? AND finished_at IS NULL")
                        .bind(id).fetch_one(s.pool()).await.unwrap();
                    prop_assert!(open <= 1, "two open readings after {steps:?}");
                }

                // The projection tracks the current reading: open if there is
                // one, else the latest closed one.
                let book = s.get_book(id).await.unwrap().unwrap();
                let readings = s.list_readings(id).await.unwrap();
                let current = readings.iter().find(|r| r.finished_at.is_none())
                    .or_else(|| readings.last())
                    .expect("every sequence writes at least one reading");
                prop_assert_eq!(book.current_page, current.current_page);
                prop_assert_eq!(book.finished, current.status == STATUS_FINISHED);
                prop_assert_eq!(book.date_finished, current.finished_at);
                Ok(())
            })?;
        }

        /// A book is on the currently-reading list **exactly when**
        /// [`Storage::active_reading`] finds it something open — and the row
        /// carries that same reading.
        ///
        /// A property because the rule is a biconditional over the whole
        /// library, and the two ways to get it wrong are opposite: the `LEFT
        /// JOIN` makes a book with *no* reading look open (every `cur` column
        /// NULL satisfies `finished_at IS NULL`, which listed the entire library
        /// once already), while a filter written against `books` rather than
        /// `cur` would drop a reread. Books are stepped independently so the
        /// generated sequences put the library in mixed states rather than
        /// marching it through one.
        #[test]
        fn the_open_list_is_exactly_the_books_with_an_open_reading(
            steps in proptest::collection::vec((0usize..3, 0u8..3), 1..16),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let s = Storage::connect("sqlite::memory:").await.unwrap();
                let mut ids = Vec::new();
                for n in 0..3 {
                    ids.push(s.upsert_book(&Book {
                        title: Some(format!("book {n}")), ..Default::default()
                    }, None).await.unwrap());
                }

                for (which, step) in &steps {
                    let id = ids[*which];
                    match step {
                        0 => { s.update_progress(id, Some(10), None).await.unwrap(); }
                        1 => { s.update_progress(id, None, Some(true)).await.unwrap(); }
                        _ => { s.reread(id).await.unwrap(); }
                    }
                }

                let listed = s.list_open_readings(10).await.unwrap();
                for id in &ids {
                    let active = s.active_reading(*id).await.unwrap();
                    let row = listed.iter().find(|(b, _)| b.id == Some(*id));
                    match (active, row) {
                        (Some(a), Some((_, r))) => prop_assert_eq!(a.id, r.id),
                        (None, None) => {}
                        (a, r) => prop_assert!(
                            false,
                            "book {} listed as {:?} but active is {:?} after {:?}",
                            id, r.map(|(_, r)| r.id), a.map(|a| a.id), steps
                        ),
                    }
                }
                Ok(())
            })?;
        }
    }
}
