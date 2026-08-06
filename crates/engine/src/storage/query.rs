//! Asking the library a narrower question than "give me everything".
//!
//! [`BookQuery`] is the one argument [`Storage::list_books`] takes, and
//! [`BookFilter`] is the half of it that also answers
//! [`Storage::count_books`]. That pairing is the whole reason this module exists
//! rather than four more parameters on `list_books`: **a count and a page have
//! to mean the same `WHERE`**, and the only way to guarantee that is for one
//! function to write it and both to call that function. Two hand-written clauses
//! that agree today are two clauses that disagree the first time a filter is
//! added to one of them.
//!
//! ## Pagination is an offset, everywhere, on purpose
//!
//! `docs/gui/spec-gui-17-28.md` left the choice open between offset and keyset
//! and noted that [`BookSort::Progress`] has no stable cursor key. Item 17 then
//! added [`BookSort::Author`], which had no cursor key *and* no `ORDER BY` — so
//! the tally was two of five sorts that could not be keyset at all, and those
//! two were exactly the ones whose pages were already expensive. Keyset would
//! have bought nothing where it was needed most and cost a second pagination
//! shape at every call site to buy it. The argument in full is in
//! `docs/decisions.md` entry 18.
//!
//! **Item 34 halved that tally and the ruling still stands.** `sort_author` is
//! a real column with a real index now, so the author sort has both an
//! `ORDER BY` and a cursor key; [`BookSort::Progress`] remains a computed ratio
//! across a `LEFT JOIN` and remains unindexable. What changed is the *cost* of
//! an offset page, which is what actually hurt — every sort but `Progress` now
//! walks an index instead of sorting the whole table — and that is the cheaper
//! half of what keyset was being considered for. Revisit when a page is
//! measured to be slow, not before.
//!
//! What offset pagination *requires*, and what the spec did not mention, is a
//! **total order**. `LIMIT 20 OFFSET 20` is only the successor of `LIMIT 20` if
//! the two statements break ties the same way, and `books.publish_year DESC`
//! over a library where four hundred books share a year does not say. SQLite's
//! sorter is in fact deterministic for one plan over one set of rows — measured,
//! not assumed: removing the tie-breaks leaves the behavioural test green — so
//! this is insurance rather than a fix for something observed. It is worth the
//! line anyway, because the guarantee belongs to the current query plan and not
//! to the schema, and the failure it would eventually produce is a book on two
//! pages and another on none, intermittently, with nothing on screen looking
//! odd. Every arm therefore ends in `books.id`; `order_by_is_a_total_order`
//! asserts it structurally, since a behavioural test that cannot fail is not the
//! one guarding this.
//!
//! ## Absence is a filter case, not a reading state
//!
//! [`StatusFilter`] has four reachable cases and one of them is
//! [`StatusFilter::NoReading`], which is `cur.id IS NULL` — a book with no
//! reading at all. It lives *here*, in the question, and deliberately not in
//! [`ReadingState`] as a `NeverOpened` variant: a variant is a thing a UI
//! filters on, counts, and eventually puts a badge beside, which is the
//! completion framing `docs/decisions.md` bans. A filter case is a question
//! somebody asked once; a variant is a permanent claim about every book.

use crate::book::ReadingState;

use super::BookSort;

/// One filter predicate's bound value, in the order the clause names them.
///
/// The clause is built as text and the values travel beside it, because the
/// alternative — interpolating them — is how a title with an apostrophe becomes
/// a syntax error and a title with a semicolon becomes something worse.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Bind {
    Text(String),
    Int(i64),
}

/// A `WHERE` clause and the values it names, from one [`BookFilter`].
///
/// `sql` is either empty or starts with `WHERE `, so a caller interpolates it
/// without knowing which — a filtered and an unfiltered statement are the same
/// `format!`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Predicate {
    pub(super) sql: String,
    pub(super) binds: Vec<Bind>,
}

/// Which books the current reading's state puts in the answer.
///
/// Four reachable cases over three variants of [`ReadingState`] plus absence —
/// see the module header for why absence is here and not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusFilter {
    /// The book's current reading is in this state. `ReadingState::Other(raw)`
    /// reaches through unchanged, so a device vocabulary this build does not
    /// model is still filterable.
    Is(ReadingState),
    /// The book has no reading at all.
    NoReading,
}

/// Which books a list is about, before it is ordered or cut into pages.
///
/// `Default` is *every book*, which is what makes the filtered and unfiltered
/// call sites one code path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookFilter {
    pub status: Option<StatusFilter>,
    /// Matched as a **substring of the stored author list**, case-insensitively.
    ///
    /// Not `names::sort_key` and not a join: `books.authors` is a JSON array of
    /// whole names, and the useful question in a library screen is "the ones
    /// with Le Guin in them". A real author *entity* is a table this schema does
    /// not have, and inventing one behind a filter would be inventing it badly.
    pub author: Option<String>,
    pub year: Option<i64>,
    /// Matched whole and case-insensitively — `en` and `EN` are one language,
    /// `en-GB` is not `en`. The column holds whatever the origin said and we do
    /// not normalise it on write, so we do not pretend to on read.
    pub language: Option<String>,
    /// One of the minted shelves in `book_tags` (Goodreads, calibre), matched
    /// whole and case-insensitively. Not `books.subjects`, which is a
    /// bibliographic fact with an origin rather than a shelf.
    pub tag: Option<String>,
    /// `Some(true)` is a book with a stored jacket; `Some(false)` is one
    /// without. `None` is not a third answer, it is not asking.
    pub has_cover: Option<bool>,
}

/// LIKE's own wildcards, so a search for `100%` is not a search for everything.
///
/// `\` is the escape character the clause declares, so it has to be escaped
/// first or escaping `%` would produce a `\` the next pass escapes again.
fn like_escape(needle: &str) -> String {
    let mut out = String::with_capacity(needle.len());
    for c in needle.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl BookFilter {
    /// True when this filter asks nothing, so every book is in the answer.
    pub fn is_empty(&self) -> bool {
        *self == BookFilter::default()
    }

    /// The clause, against the aliases `BOOK_FROM` establishes: `books` and the
    /// current reading `cur`.
    ///
    /// Every predicate here is written once and read by both `list_books` and
    /// `count_books`. See the module header.
    pub(super) fn predicate(&self) -> Predicate {
        let mut parts: Vec<String> = Vec::new();
        let mut binds: Vec<Bind> = Vec::new();

        match &self.status {
            Some(StatusFilter::Is(state)) => {
                // `cur.status` is the stored vocabulary, which is what
                // `ReadingState::Display` round-trips to — so an `Other(raw)`
                // filter names the raw word the importer wrote.
                parts.push("cur.status = ?".into());
                binds.push(Bind::Text(state.to_string()));
            }
            // Not `cur.status IS NULL`: a reading may exist with a NULL status,
            // and that book *has* been opened. The join's own absence is the
            // only honest spelling of "no reading".
            Some(StatusFilter::NoReading) => parts.push("cur.id IS NULL".into()),
            None => {}
        }
        if let Some(author) = &self.author {
            parts.push(r"books.authors LIKE ? ESCAPE '\'".into());
            binds.push(Bind::Text(format!("%{}%", like_escape(author))));
        }
        if let Some(year) = self.year {
            parts.push("books.publish_year = ?".into());
            binds.push(Bind::Int(year));
        }
        if let Some(language) = &self.language {
            parts.push("books.language = ? COLLATE NOCASE".into());
            binds.push(Bind::Text(language.clone()));
        }
        if let Some(tag) = &self.tag {
            // EXISTS rather than a join: a book carrying the same tag from two
            // sources is one book, and a join would list it twice — which would
            // also make `count_books` disagree with the length of its own page.
            parts.push(
                "EXISTS (SELECT 1 FROM book_tags bt \
                 WHERE bt.book_id = books.id AND bt.tag = ? COLLATE NOCASE)"
                    .into(),
            );
            binds.push(Bind::Text(tag.clone()));
        }
        match self.has_cover {
            Some(true) => parts.push("books.cover_path IS NOT NULL".into()),
            Some(false) => parts.push("books.cover_path IS NULL".into()),
            None => {}
        }

        let sql = if parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", parts.join(" AND "))
        };
        Predicate { sql, binds }
    }
}

/// A page of the library: which books, in what order, and which slice.
///
/// One struct rather than four arguments because `limit` and `offset` are both
/// `i64` and adjacent, which is a swap nobody's type checker catches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookQuery {
    pub sort: BookSort,
    pub filter: BookFilter,
    /// How many rows. **Negative means no limit**, which is SQLite's own reading
    /// of `LIMIT -1`.
    ///
    /// It used to need saying twice — [`BookSort::Author`] sliced in Rust and
    /// had to be taught the same convention — and since item 34 every sort is
    /// one `LIMIT ? OFFSET ?`, so the convention is the database's alone and
    /// two arms can no longer mean different things by the same number.
    pub limit: i64,
    /// How many rows to skip first. Negative is clamped to zero — there is no
    /// page before the first one.
    pub offset: i64,
}

impl Default for BookQuery {
    /// Every book, most recently touched first. The unfiltered whole-library
    /// read, which is what a caller with no opinion means.
    fn default() -> Self {
        BookQuery {
            sort: BookSort::LastModified,
            filter: BookFilter::default(),
            limit: -1,
            offset: 0,
        }
    }
}

impl BookQuery {
    /// The first `limit` books by `sort` — what `list_books(limit, sort)` meant
    /// before this item, spelled the new way.
    pub fn new(limit: i64, sort: BookSort) -> BookQuery {
        BookQuery {
            sort,
            limit,
            ..BookQuery::default()
        }
    }

    pub fn with_filter(mut self, filter: BookFilter) -> BookQuery {
        self.filter = filter;
        self
    }

    pub fn at_offset(mut self, offset: i64) -> BookQuery {
        self.offset = offset;
        self
    }
}

/// What is behind one book: how many highlights, notes and owned files it has.
///
/// **Counts rather than marks**, and the reasoning is in `docs/decisions.md`
/// entry 18. The short version: the cheap-query argument for a boolean does not
/// survive contact with the implementation (this is three grouped aggregates
/// per page either way, not a subquery per row), a count is strictly more
/// information, and the boolean is one comparison away — [`BookSummary::has`]
/// is that comparison, written once so twelve components do not each write
/// `> 0`.
///
/// Every count is of **something the user did**: highlights they took, notes
/// they wrote, files they own. Nothing here counts what has not happened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BookSummary {
    pub book_id: i64,
    pub highlights: i64,
    pub notes: i64,
    pub files: i64,
}

impl BookSummary {
    /// True when there is anything behind this book at all — the "show a mark"
    /// reading of the same row, so a frontend that wants the mark does not
    /// re-derive it.
    pub fn has(&self) -> bool {
        self.highlights > 0 || self.notes > 0 || self.files > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_filter_writes_no_clause() {
        let p = BookFilter::default().predicate();
        assert!(p.sql.is_empty());
        assert!(p.binds.is_empty());
        assert!(BookFilter::default().is_empty());
    }

    /// Absence is the join's, not the status column's. A reading with a NULL
    /// status is a book that *has* been opened, and folding the two together
    /// would put it in a filter it does not belong in.
    #[test]
    fn no_reading_is_the_absent_join_and_not_a_null_status() {
        let p = BookFilter {
            status: Some(StatusFilter::NoReading),
            ..Default::default()
        }
        .predicate();
        assert_eq!(p.sql, "WHERE cur.id IS NULL");
        assert!(
            !p.sql.contains("status"),
            "no reading must not be spelled as a status value: {}",
            p.sql
        );
    }

    /// An importer's own word survives the round trip into a filter, which is
    /// the whole reason `ReadingState::Other` carries the raw string.
    #[test]
    fn an_unknown_status_is_filterable_by_its_own_word() {
        let p = BookFilter {
            status: Some(StatusFilter::Is(ReadingState::Other("paused".into()))),
            ..Default::default()
        }
        .predicate();
        assert_eq!(p.binds, vec![Bind::Text("paused".into())]);
    }

    #[test]
    fn every_predicate_binds_in_the_order_it_names() {
        let p = BookFilter {
            status: Some(StatusFilter::Is(ReadingState::Reading)),
            author: Some("Le Guin".into()),
            year: Some(1974),
            language: Some("en".into()),
            tag: Some("sf".into()),
            has_cover: Some(true),
        }
        .predicate();
        assert_eq!(
            p.binds,
            vec![
                Bind::Text("reading".into()),
                Bind::Text("%Le Guin%".into()),
                Bind::Int(1974),
                Bind::Text("en".into()),
                Bind::Text("sf".into()),
            ],
            "the clause names five values and `has_cover` names none"
        );
        // Six predicates, and `has_cover` is the one that binds nothing. Not
        // counted by splitting on " AND " — the tag clause contains one of its
        // own, which is exactly the sort of thing a structural count gets wrong.
        for named in [
            "cur.status",
            "books.authors LIKE",
            "books.publish_year",
            "books.language",
            "book_tags",
            "books.cover_path IS NOT NULL",
        ] {
            assert!(p.sql.contains(named), "{named} missing from {}", p.sql);
        }
        assert!(p.sql.starts_with("WHERE "));
    }

    /// A search for `100%` is a search for `100%`. Without the escape it is a
    /// search for everything beginning `100`, which returns confident nonsense
    /// rather than an error.
    #[test]
    fn a_wildcard_in_the_needle_is_a_literal() {
        assert_eq!(like_escape("100%"), r"100\%");
        assert_eq!(like_escape("a_b"), r"a\_b");
        assert_eq!(like_escape(r"a\b"), r"a\\b");
    }

    /// The whole-library read has one spelling, and it is the one
    /// `BookQuery::default()` produces.
    ///
    /// The `skip`/`take` accessors this used to assert against were
    /// [`BookSort::Author`]'s Rust slice teaching itself SQLite's convention,
    /// and item 34 deleted the slice — a negative limit is now `LIMIT -1` and
    /// nothing else reads it. What survives is the contract the callers depend
    /// on: `koreader::scores_for` and `goodreads::export` used to write `10_000`
    /// and `i64::MAX` for this.
    #[test]
    fn the_default_query_is_the_whole_library_by_recency() {
        let q = BookQuery::default();
        assert!(q.limit < 0, "a negative limit is SQLite's own `no limit`");
        assert_eq!(q.offset, 0);
        assert_eq!(q.sort, BookSort::LastModified);
        assert!(q.filter.is_empty());
        assert_eq!(BookQuery::new(20, BookSort::Title).limit, 20);
    }
}
