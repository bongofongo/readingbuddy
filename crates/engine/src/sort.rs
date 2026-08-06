//! The two sort keys the shelf is ordered by, derived.
//!
//! A sort key is a *derived fact*, which item 17 puts in the engine, and the
//! two here are the ones SQLite cannot derive for itself. They are stored
//! because an index is the whole point: `ORDER BY` over an expression the
//! database cannot index is a sort of the whole table however you paginate it,
//! and `BookSort::Author` was worse than that — no `ORDER BY` at all, a
//! whole-library read sorted in Rust and *then* truncated.
//!
//! Two columns, and they are not symmetric:
//!
//! - **`books.sort_title`** has been in the schema since `0001_init.sql` and had
//!   **never been computed by anything**. It was on `Book`, on `BookDto`, in the
//!   generated TypeScript, in `MERGE_RULES`, bound by the upsert — and NULL on
//!   every row of every database that has ever existed, which is why
//!   `BookSort::Title` ordered by `books.title` instead. A sort-key column with
//!   no writer looks answered and is not.
//! - **`books.sort_author`** is new in migration `0016` and is deliberately
//!   **not** on [`Book`] at all. It is the engine's own filing key, not a fact
//!   about the book, and keeping it off the domain type is what keeps it off the
//!   wire — there is no `BookDto` field to explain and no frontend that can
//!   accidentally show a string full of `\u{1}`.
//!
//! Both are written by exactly one function, `Storage::refresh_sort_keys`, which
//! runs inside the same transaction as every writer that can move a title or an
//! author list. That is item 20's arrangement for the cover metrics, and it is
//! here for the same reason: a key that a record-shaped writer could move
//! *without* moving the column it describes is a row that contradicts itself and
//! that nothing downstream can tell from a correct one.

use crate::book::Book;
use crate::names;

/// Leading articles dropped when filing a title.
///
/// **English only, and deliberately narrow.** `books.language` is NULL for most
/// of a real library, so a table covering `der`/`die`/`das`/`el`/`la`/`le`/`les`
/// would be applied on a guess — and it would file *Das Kapital* under "Kapital"
/// and *La Bamba* under "Bamba" while filing a Spanish book tagged `en` under
/// nothing recognisable. `pdf.rs`'s title heuristic is narrow for the same
/// reason: a general rule that is wrong for some books is worse than a specific
/// rule that is silent about them.
///
/// Each entry ends in a space, so "Anthem" is not "An" + "them" and a title that
/// *is* the word "A" keeps it.
const ARTICLES: [&str; 3] = ["the ", "a ", "an "];

/// The name a title files under: the title with a leading English article
/// dropped, trimmed.
///
/// `None` for a blank title — absence, not the empty string, because
/// `COALESCE(sort_title, title)` is what the index and the `ORDER BY` are
/// written over and a stored `''` would file every untitled book above every
/// titled one rather than beside its own raw title.
///
/// **Case is preserved.** The comparison is `COLLATE NOCASE` at the other end;
/// lowercasing here would make the stored value useless to look at and would put
/// a second case convention in the schema.
///
/// Stripping never empties a title: "The " files under "The", because the
/// alternative is a book filed under nothing, which is the failure
/// [`names`] names as the one that matters.
pub fn sort_title(title: &str) -> Option<String> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    for article in ARTICLES {
        // `get` rather than an index: `article.len()` is a count of ASCII bytes
        // and the title need not have a char boundary there.
        if title
            .get(..article.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(article))
        {
            let rest = title[article.len()..].trim_start();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    Some(title.to_string())
}

/// The string `BookSort::Title` actually orders by, for a caller holding a page
/// it sorts itself.
///
/// The Rust reading of the SQL's `COALESCE(books.sort_title, books.title)`, and
/// it exists so that the TUI's own in-memory reorder of a fetched page cannot
/// hold a second opinion about where a book files. That is exactly the split
/// item 17 removed for author names, arriving a second time for titles: a
/// frontend sorting by `b.title.to_lowercase()` and a database sorting by
/// `sort_title` disagree about *The Overstory* and the disagreement looks like a
/// bug in the sort.
///
/// The **case folding is the caller's**, and the two ends do not agree
/// perfectly: SQLite's `COLLATE NOCASE` folds ASCII only, while Rust's
/// `to_lowercase` is full Unicode. That mismatch predates this module and is not
/// introduced by it; it is stated here rather than papered over.
pub fn title_key(book: &Book) -> &str {
    book.sort_title
        .as_deref()
        .or(book.title.as_deref())
        .unwrap_or("")
}

/// The separator between the components of an author key.
///
/// `\u{1}`, and the choice is load-bearing rather than cosmetic. For
/// `a ⧺ SEP ⧺ b` to compare like the tuple `(a, b)`, **`SEP` must be strictly
/// less than every byte either component can contain** — otherwise `("", "z")`
/// and `("\0", "a")` come out in the wrong order, and they come out in the wrong
/// order silently. [`escape`] is what buys that guarantee: it removes `\u{0}`
/// and `\u{1}` from the components, leaving `\u{1}` below everything that is
/// left.
///
/// Not `\u{0}`: SQLite stores an embedded NUL in a TEXT value happily and
/// compares past it correctly, but every C-string consumer of
/// `sqlite3_column_text` — the `sqlite3` shell `make dev-db` runs among them —
/// truncates at it, so the column would be unreadable by exactly the tools used
/// to check it.
const SEP: char = '\u{1}';

/// Remove `\u{0}`, `\u{1}` and `\u{2}` from a key component, order-preservingly.
///
/// This is the part that has to be *proved* rather than eyeballed, because the
/// whole claim of the column is that `ORDER BY sort_author` reproduces
/// [`names::sort_key`]'s tuple order **exactly**, and an encoding that merely
/// nearly agrees is worse than no column at all: both exist and only one is
/// read.
///
/// The code is
///
/// ```text
/// \u{0} → \u{2}\u{2}     \u{1} → \u{2}\u{3}     \u{2} → \u{2}\u{4}
/// ```
///
/// and everything else is itself. Two properties make concatenation
/// order-preserving, and between them they are the whole argument:
///
/// - **Monotone.** The images, in source order, are `\2\2 < \2\3 < \2\4 < \3 <
///   \4 < …`, which is the source order. So `a < b` implies
///   `escape(a) < escape(b)`.
/// - **Prefix-free.** Every image is either two bytes beginning `\2` or one byte
///   `≥ \3`, so no image is a prefix of another and a divergence in the source
///   is a divergence in the encoding at the same component.
///
/// UTF-8 is itself prefix-free and monotone on code points, and the three chars
/// rewritten here are single-byte ASCII, so the composition is monotone on whole
/// strings. `escaping_preserves_order` asserts it over arbitrary pairs.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{0}' => out.push_str("\u{2}\u{2}"),
            '\u{1}' => out.push_str("\u{2}\u{3}"),
            '\u{2}' => out.push_str("\u{2}\u{4}"),
            _ => out.push(c),
        }
    }
    out
}

/// [`names::sort_key`]'s tuple as one TEXT value, ordering identically under
/// SQLite's default BINARY collation.
///
/// The tuple is `(rank, last name, whole name)` — rank 0 for a book with an
/// author and 1 for one without, which is what keeps an authorless book at the
/// bottom rather than at the top under an empty string. A column is one value,
/// so the tuple has to be flattened, and the flattening has to be exact:
///
/// ```text
/// '0' SEP escape(last name) SEP escape(whole name)
/// '1' SEP SEP                        (authorless)
/// ```
///
/// Three things make the byte order of that string the lexicographic order of
/// the tuple. **The rank is one fixed-width ASCII digit**, so it decides at
/// position 0 or not at all, and `'0' < '1'` puts authorless last. **Every
/// component is escaped**, so none of them contains a byte `≤ SEP`. And
/// **SQLite's BINARY collation is `memcmp`**, which is byte-for-byte what Rust's
/// `Ord for String` is — so the two ends are comparing the same way and not
/// merely comparing similarly.
///
/// Every component is escaped, including the last one, which strictly does not
/// need it. One rule with no "except the final field" caveat is a rule a fourth
/// component cannot quietly break.
///
/// The stored value is **never shown to anybody**: it holds control characters
/// by construction, it is not on [`Book`], and it is not on the wire.
pub fn author_key(authors: &[String]) -> String {
    let (rank, last, whole) = names::sort_key(authors);
    let rank = if rank == 0 { '0' } else { '1' };
    format!("{rank}{SEP}{}{SEP}{}", escape(&last), escape(&whole))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_article_is_dropped() {
        assert_eq!(sort_title("The Overstory").as_deref(), Some("Overstory"));
        assert_eq!(
            sort_title("A Clockwork Orange").as_deref(),
            Some("Clockwork Orange")
        );
        assert_eq!(
            sort_title("An Artist of the Floating World").as_deref(),
            Some("Artist of the Floating World")
        );
        // Case is the author's, not ours.
        assert_eq!(sort_title("THE ROAD").as_deref(), Some("ROAD"));
    }

    /// The three ways a naive prefix match goes wrong, all of them real titles.
    #[test]
    fn an_article_is_a_word_and_not_a_prefix() {
        assert_eq!(sort_title("Anthem").as_deref(), Some("Anthem"));
        assert_eq!(sort_title("Theft").as_deref(), Some("Theft"));
        assert_eq!(
            sort_title("Ancillary Justice").as_deref(),
            Some("Ancillary Justice")
        );
    }

    /// Stripping never files a book under nothing, and a blank title is absence
    /// rather than an empty string.
    #[test]
    fn stripping_never_empties_a_title() {
        assert_eq!(sort_title("The").as_deref(), Some("The"));
        assert_eq!(sort_title("The ").as_deref(), Some("The"));
        assert_eq!(sort_title("A").as_deref(), Some("A"));
        assert_eq!(sort_title(""), None);
        assert_eq!(sort_title("   "), None);
    }

    /// A title starting with a multi-byte character is sliced by *bytes* against
    /// an ASCII article, which is a panic in the obvious implementation.
    #[test]
    fn a_non_ascii_title_is_not_sliced_through_a_character() {
        assert_eq!(sort_title("Éclair").as_deref(), Some("Éclair"));
        assert_eq!(sort_title("日").as_deref(), Some("日"));
        assert_eq!(sort_title("Ю").as_deref(), Some("Ю"));
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_authorless_book_files_after_every_author() {
        let none = author_key(&[]);
        for who in ["Zzyzx, Zoe", "Colette", "\u{10FFFF}"] {
            assert!(
                author_key(&names(&[who])) < none,
                "{who} must file before an authorless book"
            );
        }
        // Absence and a blank string are the same absence.
        assert_eq!(author_key(&names(&["   "])), none);
    }

    #[test]
    fn the_key_files_under_the_last_name() {
        let mut keys = [
            author_key(&names(&["Ursula K. Le Guin"])),
            author_key(&names(&["Jorge Luis Borges"])),
            author_key(&names(&["Min Jin Lee"])),
        ];
        keys.sort();
        // Borges, Le Guin, Lee — by surname, not by the string as written.
        assert!(keys[0].contains("borges"));
        assert!(keys[1].contains("le guin"));
        assert!(keys[2].contains("lee"));
    }

    /// The tie-break is the whole name, so two people sharing a surname file in
    /// a stable order rather than an arbitrary one.
    #[test]
    fn a_shared_surname_is_broken_by_the_whole_name() {
        let a = author_key(&names(&["Amis, Kingsley"]));
        let b = author_key(&names(&["Amis, Martin"]));
        assert!(a < b);
    }

    /// The separator is not a character an author can smuggle in. Without the
    /// escape these two collide into the same key, and the collision is silent.
    #[test]
    fn a_separator_inside_a_name_does_not_forge_a_component() {
        let sneaky = author_key(&names(&[&format!("Zed{SEP}Aaa")]));
        let plain = author_key(&names(&["Zed Aaa"]));
        assert_ne!(sneaky, plain);
        assert_eq!(
            sneaky.matches(SEP).count(),
            2,
            "a key has exactly two separators however many the name had: {sneaky:?}"
        );
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    /// The claim the column is worth having, stated as the property it has to
    /// satisfy: **`ORDER BY sort_author` is `names::sort_key`'s tuple order.**
    ///
    /// Rust's `Ord for String` is byte-wise and so is SQLite's BINARY collation,
    /// so comparing the two encodings in Rust is comparing what the database
    /// will compare. Asserted over arbitrary author lists, including the
    /// control characters the escape exists for, because a column that *nearly*
    /// agrees with the function is worse than no column: both exist and only one
    /// is read.
    fn agree(a: &[String], b: &[String]) -> bool {
        let tuple = names::sort_key(a).cmp(&names::sort_key(b));
        let stored = author_key(a).cmp(&author_key(b));
        tuple == stored
    }

    proptest! {
        #[test]
        fn the_stored_order_is_the_tuple_order(
            a in proptest::collection::vec("[\\x00-\\x08a-z, .]{0,12}", 0..3),
            b in proptest::collection::vec("[\\x00-\\x08a-z, .]{0,12}", 0..3),
        ) {
            prop_assert!(agree(&a, &b), "{a:?} vs {b:?}");
        }

        /// The same property over ordinary names rather than hostile ones — the
        /// generator above almost never produces two names sharing a surname,
        /// which is exactly the case the third tuple component exists for.
        #[test]
        fn the_stored_order_is_the_tuple_order_for_real_names(
            a in proptest::collection::vec("(Le |van |)[A-Z][a-z]{1,4}(, [A-Z][a-z]{1,4}| [A-Z][a-z]{1,4})?", 0..3),
            b in proptest::collection::vec("(Le |van |)[A-Z][a-z]{1,4}(, [A-Z][a-z]{1,4}| [A-Z][a-z]{1,4})?", 0..3),
        ) {
            prop_assert!(agree(&a, &b), "{a:?} vs {b:?}");
        }

        /// The half of the encoding that carries the whole argument: the escape
        /// must not reorder anything. If it does, the separator's guarantee is
        /// worthless however careful the rest is.
        #[test]
        fn escaping_preserves_order(a in "[\\x00-\\x05a-c]{0,8}", b in "[\\x00-\\x05a-c]{0,8}") {
            prop_assert_eq!(a.cmp(&b), super::escape(&a).cmp(&super::escape(&b)));
        }

        /// And it must not *lose* anything either — an escape that collides two
        /// names would make them one key, which under a `books.id` tie-break
        /// looks exactly like two books by one person.
        #[test]
        fn escaping_is_injective(a in "[\\x00-\\x05a-c]{0,8}", b in "[\\x00-\\x05a-c]{0,8}") {
            prop_assert_eq!(a == b, super::escape(&a) == super::escape(&b));
        }

        /// Arbitrary input is parsed rather than panicked on, the same contract
        /// `names.rs` holds itself to: titles arrive from five importers and a
        /// text field and none of them promise a shape.
        #[test]
        fn arbitrary_input_never_panics(s in ".{0,80}") {
            let _ = sort_title(&s);
            let _ = author_key(&[s]);
        }

        /// A sort title is never blank when the title is not, because
        /// `COALESCE(sort_title, title)` cannot recover from a stored `''` — it
        /// is not NULL, so the raw title never gets its turn.
        #[test]
        fn a_titled_book_never_files_under_nothing(s in ".{0,40}") {
            if let Some(key) = sort_title(&s) {
                prop_assert!(!key.trim().is_empty(), "{s:?} filed under {key:?}");
            } else {
                prop_assert!(s.trim().is_empty());
            }
        }
    }
}
