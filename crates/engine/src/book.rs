use time::OffsetDateTime;

/// Central domain struct. Identity: internal `id` is canonical; ISBNs are
/// unique lookup keys (a book may carry either, both, or neither).
#[derive(Debug, Clone, Default)]
pub struct Book {
    pub id: Option<i64>,
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub authors: Vec<String>,
    pub translators: Vec<String>,
    pub publisher: Option<String>,
    pub publish_year: Option<i64>,
    pub language: Option<String>,
    pub isbn_10: Option<String>,
    pub isbn_13: Option<String>,
    pub openlibrary_key: Option<String>,
    pub googlebooks_id: Option<String>,
    pub cover_url: Option<String>,
    pub cover_path: Option<String>,
    pub page_count: Option<i64>,
    pub description: Option<String>,
    pub first_sentence: Option<String>,
    /// What a *provider* says this book is about (migration `0013`).
    ///
    /// Deliberately **not** `book_tags`: those are shelves the user or another
    /// system minted, and `docs/decisions.md` defers collections because three
    /// systems minting them is a merge problem. A subject is a bibliographic
    /// fact with an origin, like a publisher, and merges like one — the empty
    /// vec means "this record does not say", never "this book has no subjects".
    ///
    /// The strings are raw as the provider spelled them. There is no controlled
    /// vocabulary, which means Google's BISAC paths (`Fiction / Literary`) and
    /// OpenLibrary's headings (`Korean Americans`) are two dialects — and is
    /// why a merge takes one provider's set whole rather than unioning them.
    pub subjects: Vec<String>,
    /// The series this book belongs to, as its origin names it.
    ///
    /// A **pair** with [`Book::series_index`], and the pair is why
    /// `MERGE_RULES` gained `Rule::pair`: owning "Dune" while a provider fills
    /// in "#2" from a differently-named series is an incoherent row, so
    /// user-ownership of either half protects both.
    pub series: Option<String>,
    /// The book's position in [`Book::series`]. Fractional on purpose —
    /// novellas are numbered 0.5 and 1.5 by calibre, Goodreads and publishers
    /// alike.
    pub series_index: Option<f64>,
    /// Reading state, as a **read-only projection of the current reading** —
    /// the open one if there is one, else the most recent. Since migration
    /// `0005` these are not `books` columns and
    /// [`Storage::upsert_book`](crate::Storage::upsert_book) ignores them;
    /// [`Storage::update_progress`](crate::Storage::update_progress) is the
    /// writer. They stay on `Book` because that is what left every render call
    /// site — `render.rs`, `progress_tag`, `progress_text`, the note page
    /// auto-anchor — untouched by the move.
    pub current_page: Option<i64>,
    /// True when the current reading's status is `finished`. See
    /// [`Book::current_page`].
    pub finished: bool,
    /// The current reading's `started_at`. See [`Book::current_page`].
    pub date_started: Option<i64>,
    /// The current reading's `finished_at`. See [`Book::current_page`].
    pub date_finished: Option<i64>,
    pub created_at: Option<OffsetDateTime>,
    pub last_modified: Option<OffsetDateTime>,
}

impl Book {
    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or("(untitled)")
    }

    pub fn display_authors(&self) -> String {
        if self.authors.is_empty() {
            "(unknown)".to_string()
        } else {
            self.authors.join(", ")
        }
    }

    /// Any ISBN, preferring 13.
    pub fn any_isbn(&self) -> Option<&str> {
        self.isbn_13.as_deref().or(self.isbn_10.as_deref())
    }

    /// Canonical dedup key: ISBN-13, converting a lone ISBN-10 when possible.
    pub fn canonical_isbn13(&self) -> Option<String> {
        if let Some(i13) = &self.isbn_13 {
            return Some(i13.clone());
        }
        self.isbn_10.as_deref().and_then(isbn10_to_13)
    }
}

/// Normalize a raw ISBN-ish string (hyphens/spaces stripped, `urn:isbn:`
/// prefixes dropped, X uppercased) and validate its checksum.
/// Returns None if it isn't a valid ISBN-10 or ISBN-13.
pub fn normalize_isbn(raw: &str) -> Option<String> {
    let stripped = raw.trim().rsplit(':').next().unwrap_or(raw);
    let cleaned: String = stripped
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    match cleaned.len() {
        10 if is_valid_isbn10(&cleaned) => Some(cleaned),
        13 if is_valid_isbn13(&cleaned) => Some(cleaned),
        _ => None,
    }
}

fn is_valid_isbn10(s: &str) -> bool {
    let mut sum: u32 = 0;
    for (i, c) in s.chars().enumerate() {
        let v = match c {
            '0'..='9' => c as u32 - '0' as u32,
            'X' if i == 9 => 10,
            _ => return false,
        };
        sum += (10 - i as u32) * v;
    }
    sum.is_multiple_of(11)
}

fn is_valid_isbn13(s: &str) -> bool {
    let mut sum: u32 = 0;
    for (i, c) in s.chars().enumerate() {
        let Some(v) = c.to_digit(10) else {
            return false;
        };
        sum += if i % 2 == 0 { v } else { 3 * v };
    }
    sum.is_multiple_of(10)
}

/// Convert a valid ISBN-10 to its ISBN-13 (978-prefix) form.
pub fn isbn10_to_13(isbn10: &str) -> Option<String> {
    if isbn10.len() != 10 || !is_valid_isbn10(isbn10) {
        return None;
    }
    let body = format!("978{}", &isbn10[..9]);
    let mut sum: u32 = 0;
    for (i, c) in body.chars().enumerate() {
        let v = c.to_digit(10)?;
        sum += if i % 2 == 0 { v } else { 3 * v };
    }
    let check = (10 - sum % 10) % 10;
    Some(format!("{body}{check}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hyphens_and_prefixes() {
        assert_eq!(
            normalize_isbn("urn:isbn:978-1-4555-6393-7"),
            Some("9781455563937".to_string())
        );
        assert_eq!(
            normalize_isbn(" 0-306-40615-2 "),
            Some("0306406152".to_string())
        );
    }

    #[test]
    fn keeps_leading_zero_and_x_check_digit() {
        // Both impossible under the old i64 representation.
        assert_eq!(
            normalize_isbn("0-19-853453-1"),
            Some("0198534531".to_string())
        );
        assert_eq!(normalize_isbn("080442957x"), Some("080442957X".to_string()));
    }

    #[test]
    fn rejects_bad_checksums_and_shapes() {
        assert_eq!(normalize_isbn("9781455563938"), None); // bad check digit
        assert_eq!(normalize_isbn("0306406153"), None); // bad check digit
        assert_eq!(normalize_isbn("12345"), None);
        assert_eq!(normalize_isbn("not-an-isbn"), None);
        assert_eq!(normalize_isbn("123456789X123"), None); // X only legal in isbn10
    }

    #[test]
    fn converts_10_to_13() {
        assert_eq!(
            isbn10_to_13("0306406152"),
            Some("9780306406157".to_string())
        );
        assert_eq!(
            isbn10_to_13("155404295X"),
            Some("9781554042951".to_string())
        );
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    /// Generate a *valid* ISBN-10 by computing the check digit, rather than
    /// generating candidates and rejecting — reject-sampling would discard
    /// ~10/11 of them and make the runs slow and shallow.
    fn valid_isbn10() -> impl Strategy<Value = String> {
        proptest::collection::vec(0u32..10, 9).prop_map(|digits| {
            let sum: u32 = digits
                .iter()
                .enumerate()
                .map(|(i, d)| (10 - i as u32) * d)
                .sum();
            // c satisfies sum + 1*c ≡ 0 (mod 11)
            let check = (11 - (sum % 11)) % 11;
            let mut s: String = digits
                .iter()
                .map(|d| char::from_digit(*d, 10).unwrap())
                .collect();
            s.push(if check == 10 {
                'X'
            } else {
                char::from_digit(check, 10).unwrap()
            });
            s
        })
    }

    /// ISBN-13 check digit for 12 body digits.
    fn isbn13_check(body: &[u32]) -> u32 {
        let sum: u32 = body
            .iter()
            .enumerate()
            .map(|(i, d)| if i % 2 == 0 { *d } else { 3 * d })
            .sum();
        (10 - sum % 10) % 10
    }

    /// A valid ISBN-13 that is *guaranteed* to have two adjacent digits
    /// differing by exactly 5, and the position of that pair.
    fn isbn13_with_a_five_gap_pair() -> impl Strategy<Value = (String, usize)> {
        (
            proptest::collection::vec(0u32..10, 12),
            0usize..11,
            0u32..5,
            any::<bool>(),
        )
            .prop_map(|(mut body, pos, low, flip)| {
                let (a, b) = if flip { (low + 5, low) } else { (low, low + 5) };
                body[pos] = a;
                body[pos + 1] = b;
                let check = isbn13_check(&body);
                let mut s: String = body
                    .iter()
                    .map(|d| char::from_digit(*d, 10).unwrap())
                    .collect();
                s.push(char::from_digit(check, 10).unwrap());
                (s, pos)
            })
    }

    fn valid_isbn13() -> impl Strategy<Value = String> {
        valid_isbn10().prop_map(|t| isbn10_to_13(&t).expect("generator produced a valid isbn10"))
    }

    proptest! {
        /// The real contract of the conversion, which two hand-picked examples
        /// could never establish: the output is always itself a valid ISBN-13.
        #[test]
        fn isbn10_converts_to_a_genuinely_valid_isbn13(t in valid_isbn10()) {
            let thirteen = isbn10_to_13(&t).expect("valid 10 must convert");
            prop_assert_eq!(thirteen.len(), 13);
            prop_assert!(thirteen.starts_with("978"));
            prop_assert!(is_valid_isbn13(&thirteen), "bad checksum: {}", thirteen);
            // And it survives a round trip through the front door.
            let round = normalize_isbn(&thirteen);
            prop_assert_eq!(round.as_deref(), Some(thirteen.as_str()));
        }

        #[test]
        fn normalize_is_idempotent(t in valid_isbn10()) {
            let once = normalize_isbn(&t).unwrap();
            let twice = normalize_isbn(&once);
            prop_assert_eq!(twice.as_deref(), Some(once.as_str()));
        }

        /// Hyphens, spaces, a `urn:isbn:` prefix and lowercase `x` are all
        /// formatting, not identity. Every ISBN entering the system goes
        /// through here, so this is the invariant the whole library leans on.
        #[test]
        fn formatting_never_changes_the_result(
            t in valid_isbn10(),
            cuts in proptest::collection::vec(0usize..10, 0..4),
        ) {
            let mut decorated = t.clone();
            // Insert separators at descending positions so earlier inserts
            // don't shift later ones.
            let mut cuts = cuts;
            cuts.sort_unstable();
            cuts.dedup();
            for at in cuts.into_iter().rev() {
                decorated.insert(at, '-');
            }
            let variants = [
                decorated.clone(),
                format!("urn:isbn:{decorated}"),
                format!("  {decorated}  "),
                decorated.replace('-', " "),
                decorated.to_lowercase(),
            ];
            for v in variants {
                let got = normalize_isbn(&v);
                prop_assert_eq!(
                    got.as_deref(),
                    Some(t.as_str()),
                    "decoration changed the parse: {:?}", v
                );
            }
        }

        /// What the check digit is *for*. Catches an off-by-one in the
        /// alternating 1-3-1-3 weights, which a fixed example set will not.
        #[test]
        fn a_single_wrong_digit_invalidates_an_isbn13(
            t in valid_isbn13(),
            pos in 0usize..13,
            delta in 1u32..10,
        ) {
            let mut bytes: Vec<char> = t.chars().collect();
            let orig = bytes[pos].to_digit(10).unwrap();
            let changed = (orig + delta) % 10;
            prop_assume!(changed != orig);
            bytes[pos] = char::from_digit(changed, 10).unwrap();
            let mutated: String = bytes.into_iter().collect();
            prop_assert!(
                !is_valid_isbn13(&mutated),
                "single-digit error slipped through: {} -> {}", t, mutated
            );
        }

        /// Transposition detection is ASYMMETRIC between the two schemes, and
        /// this is the property a naive implementation gets wrong and then
        /// "fixes" by weakening the test.
        ///
        /// ISBN-10 is mod 11 with distinct weights, so swapping any two
        /// adjacent unequal digits always breaks it.
        #[test]
        fn adjacent_transposition_always_breaks_an_isbn10(
            t in valid_isbn10(),
            pos in 0usize..9,
        ) {
            let mut c: Vec<char> = t.chars().collect();
            // Skip the check position, where 'X' is not a digit swap.
            prop_assume!(c[pos] != c[pos + 1]);
            prop_assume!(c[pos].is_ascii_digit() && c[pos + 1].is_ascii_digit());
            c.swap(pos, pos + 1);
            let swapped: String = c.into_iter().collect();
            prop_assert!(
                !is_valid_isbn10(&swapped),
                "mod-11 must catch every adjacent transposition: {} -> {}", t, swapped
            );
        }

        /// The counterpart, stated so nobody later "strengthens" the test
        /// above into something false: ISBN-13's 1-3-1-3 weighting CANNOT see
        /// a transposition of two adjacent digits differing by 5, because the
        /// checksum moves by 2*5 = 10 ≡ 0 (mod 10).
        ///
        /// The qualifying pair is *constructed*, not filtered for — assuming
        /// it rejects ~90% of cases and the run dies on the reject cap without
        /// ever reaching the assertion.
        #[test]
        fn isbn13_provably_misses_transpositions_differing_by_five(
            (t, pos) in isbn13_with_a_five_gap_pair(),
        ) {
            let mut c: Vec<char> = t.chars().collect();
            let a = c[pos].to_digit(10).unwrap();
            let b = c[pos + 1].to_digit(10).unwrap();
            prop_assert_eq!(a.abs_diff(b), 5, "generator broke its own contract");

            c.swap(pos, pos + 1);
            let swapped: String = c.into_iter().collect();
            prop_assert!(
                is_valid_isbn13(&swapped),
                "expected mod-10 to be blind here; if this now fails the \
                 algorithm changed, it is not a bug in the test: {} -> {}", t, swapped
            );
        }

        /// Arbitrary input must be rejected, not panic. `normalize_isbn` is the
        /// single entry point for every ISBN in the system, including ones
        /// typed by hand and ones scraped from a provider.
        #[test]
        fn arbitrary_input_never_panics(s in ".{0,64}") {
            let _ = normalize_isbn(&s);
        }

        #[test]
        fn arbitrary_input_that_parses_round_trips(s in "[0-9Xx: -]{0,32}") {
            if let Some(n) = normalize_isbn(&s) {
                let again = normalize_isbn(&n);
                prop_assert_eq!(again.as_deref(), Some(n.as_str()));
                prop_assert!(n.len() == 10 || n.len() == 13);
            }
        }
    }
}
