use std::collections::HashMap;
use std::time::Duration;

use strsim::jaro_winkler;

use crate::book::Book;
use crate::diagnostic::Diagnostic;
use crate::error::Result;
use crate::providers::{MetadataProvider, ProviderBook, ProviderId, SearchRequest};

const PROVIDER_TIMEOUT: Duration = Duration::from_secs(5);

/// One deduped, merged, scored search result.
#[derive(Debug, Clone)]
pub struct RankedResult {
    pub book: Book,
    pub sources: Vec<ProviderId>,
    pub score: f64,
}

#[derive(Debug, Default)]
pub struct SearchOutcome {
    pub results: Vec<RankedResult>,
    /// Per-provider failures — a dead API degrades, never kills the search.
    pub warnings: Vec<Diagnostic>,
}

impl SearchOutcome {
    /// Providers that contributed nothing because they failed or timed out.
    /// Lets a frontend say *which* source is missing rather than just showing
    /// fewer results.
    pub fn failed_providers(&self) -> Vec<ProviderId> {
        self.warnings.iter().filter_map(|d| d.provider()).collect()
    }

    pub fn timed_out(&self) -> bool {
        self.warnings.iter().any(|d| d.is_timeout())
    }
}

pub async fn federated_search(
    providers: &[Box<dyn MetadataProvider>],
    req: &SearchRequest,
) -> Result<SearchOutcome> {
    let fetches = providers.iter().map(|p| async {
        let id = p.id();
        match tokio::time::timeout(PROVIDER_TIMEOUT, p.search(req)).await {
            Ok(Ok(books)) => (id, None, Some(books)),
            Ok(Err(e)) => (id, Some(Diagnostic::provider_failed(id, &e)), None),
            Err(_) => (
                id,
                Some(Diagnostic::provider_timed_out(id, PROVIDER_TIMEOUT)),
                None,
            ),
        }
    });
    let responses = futures::future::join_all(fetches).await;

    let mut outcome = SearchOutcome::default();
    let mut raw: Vec<ProviderBook> = Vec::new();
    for (id, diag, books) in responses {
        match (diag, books) {
            (_, Some(books)) => {
                tracing::debug!(provider = %id, count = books.len(), "provider answered");
                raw.extend(books);
            }
            (Some(d), None) => {
                // Emitted here, beside the push, so the log line and the
                // in-band diagnostic can never drift apart.
                tracing::warn!(provider = %id, detail = %d.detail, "provider degraded");
                outcome.warnings.push(d);
            }
            (None, None) => {}
        }
    }

    let mut merged = dedup(raw);
    for m in &mut merged {
        m.score = rank(&m.book, &m.sources, m.best_position, req);
    }
    merged.sort_by(|a, b| b.score.total_cmp(&a.score));
    outcome.results = merged
        .into_iter()
        .map(|m| RankedResult {
            book: m.book,
            sources: m.sources,
            score: m.score,
        })
        .collect();
    Ok(outcome)
}

/// Merge per-provider lookups of the SAME edition (e.g. by_isbn results)
/// into one Book. Returns None when the input is empty.
pub fn merge_provider_books(raw: Vec<ProviderBook>) -> Option<Book> {
    dedup(raw).into_iter().next().map(|m| m.book)
}

// ---- dedup -----------------------------------------------------------------

#[derive(Debug)]
struct Merged {
    book: Book,
    sources: Vec<ProviderId>,
    best_position: usize,
    score: f64,
}

/// Strip punctuation, lowercase, drop leading articles, collapse whitespace.
pub fn normalize(s: &str) -> String {
    let lowered = s.to_lowercase();
    let cleaned: String = lowered
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.first().is_some_and(|w| is_article(w)) {
        words.remove(0);
    }
    words.join(" ")
}

/// The leading articles [`normalize`] drops, as one definition.
///
/// **Exactly one is dropped, and that is deliberate**: stripping repeatedly
/// would take "The A Team" to `"team"` rather than `"a team"`, which would
/// change what the shared fuzzy matcher considers the same book on every import
/// path. The consequence is that `normalize` is *not* idempotent on a title
/// beginning with two articles — pinned rather than papered over by
/// `normalize_strips_one_article_and_is_otherwise_well_shaped`.
fn is_article(word: &str) -> bool {
    matches!(word, "the" | "a" | "an")
}

/// Whether an already-normalized string still begins with an article. Shares
/// [`is_article`] with `normalize`, so the property cannot disagree with the
/// function about what one is.
#[cfg(test)]
fn starts_with_article(normalized: &str) -> bool {
    normalized
        .split(' ')
        .next()
        .is_some_and(|w| !w.is_empty() && is_article(w))
}

fn fingerprint(book: &Book) -> String {
    let title = normalize(book.title.as_deref().unwrap_or(""));
    let author = normalize(book.authors.first().map(String::as_str).unwrap_or(""));
    format!("{title}|{author}")
}

/// True when two ISBN-less records look like the same work.
fn same_work(a: &Book, b: &Book) -> bool {
    let ta = normalize(a.title.as_deref().unwrap_or(""));
    let tb = normalize(b.title.as_deref().unwrap_or(""));
    if ta.is_empty() || tb.is_empty() {
        return false;
    }
    let title_sim = jaro_winkler(&ta, &tb);
    let author_sim = match (a.authors.first(), b.authors.first()) {
        (Some(x), Some(y)) => jaro_winkler(&normalize(x), &normalize(y)),
        // One side missing authors: lean on the title alone.
        _ => 1.0,
    };
    title_sim > 0.93 && author_sim > 0.9
}

/// Merge `b` into `a`. Default: fill missing fields. Provider priority:
/// OpenLibrary wins ISBN/page_count, Google wins description/language.
fn merge_into(a: &mut Book, b: &Book, b_provider: ProviderId) {
    fn fill<T: Clone>(dst: &mut Option<T>, src: &Option<T>) {
        if dst.is_none() && src.is_some() {
            *dst = src.clone();
        }
    }
    fn prefer<T: Clone>(dst: &mut Option<T>, src: &Option<T>, src_wins: bool) {
        if src.is_some() && (dst.is_none() || src_wins) {
            *dst = src.clone();
        }
    }
    let is_ol = b_provider == ProviderId::OpenLibrary;
    let is_gb = b_provider == ProviderId::GoogleBooks;

    fill(&mut a.title, &b.title);
    if a.authors.is_empty() {
        a.authors = b.authors.clone();
    }
    if a.translators.is_empty() {
        a.translators = b.translators.clone();
    }
    prefer(&mut a.isbn_10, &b.isbn_10, is_ol);
    prefer(&mut a.isbn_13, &b.isbn_13, is_ol);
    prefer(&mut a.page_count, &b.page_count, is_ol);
    prefer(&mut a.description, &b.description, is_gb);
    prefer(&mut a.language, &b.language, is_gb);
    fill(&mut a.publisher, &b.publisher);
    fill(&mut a.publish_year, &b.publish_year);
    fill(&mut a.first_sentence, &b.first_sentence);
    fill(&mut a.cover_url, &b.cover_url);
    fill(&mut a.openlibrary_key, &b.openlibrary_key);
    fill(&mut a.googlebooks_id, &b.googlebooks_id);
}

fn dedup(raw: Vec<ProviderBook>) -> Vec<Merged> {
    let mut merged: Vec<Merged> = Vec::new();
    let mut by_isbn: HashMap<String, usize> = HashMap::new();
    let mut by_fingerprint: HashMap<String, usize> = HashMap::new();

    for pb in raw {
        let isbn_key = pb.book.canonical_isbn13();
        let existing = isbn_key
            .as_ref()
            .and_then(|k| by_isbn.get(k).copied())
            .or_else(|| {
                by_fingerprint
                    .get(&fingerprint(&pb.book))
                    .copied()
                    .filter(|&i| same_work(&merged[i].book, &pb.book))
            });

        match existing {
            Some(i) => {
                merge_into(&mut merged[i].book, &pb.book, pb.provider);
                if !merged[i].sources.contains(&pb.provider) {
                    merged[i].sources.push(pb.provider);
                }
                merged[i].best_position = merged[i].best_position.min(pb.position);
                if let Some(k) = merged[i].book.canonical_isbn13() {
                    by_isbn.entry(k).or_insert(i);
                }
            }
            None => {
                let i = merged.len();
                if let Some(k) = isbn_key {
                    by_isbn.insert(k, i);
                }
                by_fingerprint.entry(fingerprint(&pb.book)).or_insert(i);
                merged.push(Merged {
                    book: pb.book,
                    sources: vec![pb.provider],
                    best_position: pb.position,
                    score: 0.0,
                });
            }
        }
    }
    merged
}

// ---- ranking ---------------------------------------------------------------

/// Field-weighted relevance score. Pure function, unit-testable.
/// Weights: exact ISBN 1000 | title 40 | free query vs title 30 | author 25 |
/// publisher 10 | translator-in-contributors 10 | year 5 | language 5 |
/// provider position decay 8 | found-in-both bonus 6.
pub fn rank(book: &Book, sources: &[ProviderId], best_position: usize, req: &SearchRequest) -> f64 {
    let mut score = 0.0;

    if let Some(want) = &req.isbn
        && (book.isbn_10.as_deref() == Some(want.as_str())
            || book.isbn_13.as_deref() == Some(want.as_str()))
    {
        score += 1000.0;
    }
    if let (Some(want), Some(have)) = (&req.title, &book.title) {
        score += 40.0 * jaro_winkler(&normalize(want), &normalize(have));
    }
    if let (Some(q), Some(have)) = (&req.query, &book.title) {
        score += 30.0 * jaro_winkler(&normalize(q), &normalize(have));
    }
    if let Some(want) = &req.author {
        let best = book
            .authors
            .iter()
            .map(|a| jaro_winkler(&normalize(want), &normalize(a)))
            .fold(0.0_f64, f64::max);
        score += 25.0 * best;
    }
    if let (Some(want), Some(have)) = (&req.publisher, &book.publisher) {
        score += 10.0 * jaro_winkler(&normalize(want), &normalize(have));
    }
    if let Some(want) = &req.translator {
        let pool = book.translators.iter().chain(book.authors.iter());
        let best = pool
            .map(|a| jaro_winkler(&normalize(want), &normalize(a)))
            .fold(0.0_f64, f64::max);
        score += 10.0 * best;
    }
    if req.year.is_some() && book.publish_year == req.year {
        score += 5.0;
    }
    if let (Some(want), Some(have)) = (&req.language, &book.language)
        && want.eq_ignore_ascii_case(have)
    {
        score += 5.0;
    }
    score += 8.0 / (1.0 + best_position as f64);
    if sources.len() > 1 {
        score += 6.0;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pb(provider: ProviderId, position: usize, book: Book) -> ProviderBook {
        ProviderBook {
            book,
            provider,
            position,
        }
    }

    fn book(title: &str, author: &str) -> Book {
        Book {
            title: Some(title.into()),
            authors: vec![author.into()],
            ..Default::default()
        }
    }

    #[test]
    fn normalize_strips_articles_and_punct() {
        assert_eq!(normalize("The Trial"), "trial");
        assert_eq!(normalize("trial, the"), "trial the"); // article only dropped at front
        assert_eq!(normalize("A  Wizard of Earthsea!"), "wizard of earthsea");
    }

    #[test]
    fn dedups_across_isbn_forms() {
        // Same book: one provider has ISBN-10, the other ISBN-13.
        let mut a = book("Pachinko", "Min Jin Lee");
        a.isbn_10 = Some("1455563935".into());
        let mut b = book("Pachinko", "Min Jin Lee");
        b.isbn_13 = Some("9781455563937".into());
        b.description = Some("A saga.".into());

        let merged = dedup(vec![
            pb(ProviderId::OpenLibrary, 0, a),
            pb(ProviderId::GoogleBooks, 0, b),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].sources.len(), 2);
        assert_eq!(merged[0].book.description.as_deref(), Some("A saga."));
        assert_eq!(merged[0].book.isbn_10.as_deref(), Some("1455563935"));
    }

    #[test]
    fn dedups_by_fuzzy_fingerprint_without_isbn() {
        let a = book("The Trial", "Franz Kafka");
        let b = book("Trial", "Franz Kafka"); // article-stripped variant
        let c = book("The Castle", "Franz Kafka"); // different work stays separate
        let merged = dedup(vec![
            pb(ProviderId::OpenLibrary, 0, a),
            pb(ProviderId::GoogleBooks, 0, b),
            pb(ProviderId::GoogleBooks, 1, c),
        ]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_provider_preferences() {
        // OL wins page_count even when GB arrived first with one.
        let mut gb = book("Pachinko", "Min Jin Lee");
        gb.page_count = Some(512);
        gb.language = Some("en".into());
        let mut ol = book("Pachinko", "Min Jin Lee");
        ol.page_count = Some(490);

        let merged = dedup(vec![
            pb(ProviderId::GoogleBooks, 0, gb),
            pb(ProviderId::OpenLibrary, 0, ol),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].book.page_count, Some(490));
        assert_eq!(merged[0].book.language.as_deref(), Some("en"));
    }

    #[test]
    fn exact_isbn_dominates_ranking() {
        let req = SearchRequest {
            isbn: Some("9781455563937".into()),
            title: Some("Pachinko".into()),
            ..Default::default()
        };
        let mut with_isbn = book("Some Other Title", "Nobody");
        with_isbn.isbn_13 = Some("9781455563937".into());
        let perfect_title = book("Pachinko", "Min Jin Lee");

        let s1 = rank(&with_isbn, &[ProviderId::OpenLibrary], 5, &req);
        let s2 = rank(&perfect_title, &[ProviderId::OpenLibrary], 0, &req);
        assert!(s1 > s2);
    }

    #[test]
    fn title_match_beats_author_match() {
        let req = SearchRequest {
            title: Some("Pachinko".into()),
            author: Some("Min Jin Lee".into()),
            ..Default::default()
        };
        let title_hit = book("Pachinko", "Someone Else");
        let author_hit = book("Free Food for Millionaires", "Min Jin Lee");
        let s_title = rank(&title_hit, &[ProviderId::OpenLibrary], 0, &req);
        let s_author = rank(&author_hit, &[ProviderId::OpenLibrary], 0, &req);
        assert!(s_title > s_author);
    }

    #[test]
    fn both_provider_bonus_and_position_decay() {
        let req = SearchRequest {
            title: Some("Dune".into()),
            ..Default::default()
        };
        let b = book("Dune", "Frank Herbert");
        let one = rank(&b, &[ProviderId::OpenLibrary], 0, &req);
        let both = rank(
            &b,
            &[ProviderId::OpenLibrary, ProviderId::GoogleBooks],
            0,
            &req,
        );
        let deep = rank(&b, &[ProviderId::OpenLibrary], 20, &req);
        assert!(both > one);
        assert!(one > deep);
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    /// Every non-ISBN term's ceiling, summed: title 40 + query 30 + author 25 +
    /// publisher 10 + translator 10 + year 5 + language 5 + position 8 +
    /// multi-source 6. Kept here so the dominance property below states *why*
    /// it holds rather than just asserting a number.
    const MAX_NON_ISBN_SCORE: f64 = 40.0 + 30.0 + 25.0 + 10.0 + 10.0 + 5.0 + 5.0 + 8.0 + 6.0;

    fn text() -> impl Strategy<Value = String> {
        // Deliberately includes punctuation, accents and CJK: `normalize` and
        // jaro_winkler both have to cope with whatever a provider returns.
        prop_oneof![
            "[a-zA-Z ]{0,20}",
            "[\\p{Latin}\\p{Han}\\p{Hiragana}'’,.:!-]{0,20}",
            Just(String::new()),
        ]
    }

    fn arb_book() -> impl Strategy<Value = Book> {
        (
            proptest::option::of(text()),
            proptest::collection::vec(text(), 0..3),
            proptest::option::of(text()),
            proptest::option::of(1000i64..2100),
            proptest::option::of("[a-z]{2}"),
        )
            .prop_map(|(title, authors, publisher, publish_year, language)| Book {
                title,
                authors,
                publisher,
                publish_year,
                language,
                ..Default::default()
            })
    }

    fn arb_req() -> impl Strategy<Value = SearchRequest> {
        (
            proptest::option::of(text()),
            proptest::option::of(text()),
            proptest::option::of(text()),
            proptest::option::of(1000i64..2100),
        )
            .prop_map(|(query, title, author, year)| SearchRequest {
                query,
                title,
                author,
                year,
                ..Default::default()
            })
    }

    proptest! {
        /// `federated_search` sorts with `total_cmp`, which does not panic on
        /// NaN — it silently orders by bit pattern instead, so a NaN score
        /// would scramble results with nothing looking wrong. jaro_winkler("",
        /// "") returning 1.0 rather than NaN is the load-bearing assumption.
        #[test]
        fn rank_is_always_finite(
            book in arb_book(),
            req in arb_req(),
            pos in 0usize..1000,
            two_sources in any::<bool>(),
        ) {
            let sources: Vec<ProviderId> = if two_sources {
                vec![ProviderId::OpenLibrary, ProviderId::GoogleBooks]
            } else {
                vec![ProviderId::OpenLibrary]
            };
            let s = rank(&book, &sources, pos, &req);
            prop_assert!(s.is_finite(), "non-finite score {} for {:?}", s, book.title);
            prop_assert!(s >= 0.0, "negative score {}", s);
        }

        /// An exact ISBN match must outrank anything a non-ISBN match can
        /// possibly accumulate. Pinning this means a later "let's weight titles
        /// more heavily" that breaks it fails here rather than silently
        /// reordering someone's search.
        #[test]
        fn an_exact_isbn_match_always_wins(
            other in arb_book(),
            req_extra in arb_req(),
            pos_a in 0usize..50,
            pos_b in 0usize..50,
        ) {
            let isbn = "9780316228534";
            let mut req = req_extra;
            req.isbn = Some(isbn.to_string());

            let matching = Book {
                isbn_13: Some(isbn.to_string()),
                ..Default::default()
            };
            // The rival gets every advantage: both providers, top position.
            let mut rival = other;
            rival.isbn_13 = None;
            rival.isbn_10 = None;

            let hit = rank(&matching, &[ProviderId::OpenLibrary], pos_a, &req);
            let miss = rank(
                &rival,
                &[ProviderId::OpenLibrary, ProviderId::GoogleBooks],
                pos_b,
                &req,
            );

            prop_assert!(miss <= MAX_NON_ISBN_SCORE, "score model changed: {}", miss);
            prop_assert!(
                hit > miss,
                "ISBN match ({}) did not outrank a non-match ({})", hit, miss
            );
        }

        /// Monotone in position: a result the provider ranked higher never
        /// scores lower for that reason alone.
        #[test]
        fn earlier_positions_never_score_lower(
            book in arb_book(),
            req in arb_req(),
            a in 0usize..500,
            b in 0usize..500,
        ) {
            prop_assume!(a < b);
            let src = [ProviderId::OpenLibrary];
            prop_assert!(rank(&book, &src, a, &req) >= rank(&book, &src, b, &req));
        }

        #[test]
        fn agreement_between_providers_never_hurts(book in arb_book(), req in arb_req()) {
            let one = rank(&book, &[ProviderId::OpenLibrary], 3, &req);
            let two = rank(
                &book,
                &[ProviderId::OpenLibrary, ProviderId::GoogleBooks],
                3,
                &req,
            );
            prop_assert!(two >= one);
        }

        /// The shape invariants are unconditional; **idempotence is not, and
        /// claiming it was wrong about the function rather than about the
        /// input.**
        ///
        /// `normalize` strips exactly **one** leading article, deliberately:
        /// looping the strip would take "The A Team" to `"team"` instead of
        /// `"a team"`, mangling real titles across all three import paths that
        /// match on them. So it is idempotent precisely when its own output does
        /// not itself begin with an article — which is every title that does not
        /// start with two of them.
        ///
        /// Proptest found `"a A "` and said so: `normalize` gives `"a"`, and a
        /// second pass gives `""`. The property was over-claiming from the
        /// start; it survived only because the generator rarely produces two
        /// articles in a row. Weakened to what the function actually promises,
        /// per `CLAUDE.md` — asserting less beats asserting something false.
        #[test]
        fn normalize_strips_one_article_and_is_otherwise_well_shaped(s in text()) {
            let once = normalize(&s);
            if !starts_with_article(&once) {
                prop_assert_eq!(normalize(&once), once.clone());
            } else {
                // The one case idempotence fails, and it must fail *by exactly
                // one more article* — never by more, and never by anything else.
                prop_assert_eq!(
                    normalize(&once),
                    once.split_once(' ').map(|(_, rest)| rest).unwrap_or("")
                );
            }
            prop_assert!(!once.starts_with(' ') && !once.ends_with(' '), "{:?}", once);
            prop_assert!(!once.contains("  "), "double space in {:?}", once);
            prop_assert!(
                once.chars().all(|c| c.is_alphanumeric() || c == ' '),
                "unexpected char in {:?}", once
            );
        }

        /// `same_work` decides whether two provider results are the same book.
        /// It must be symmetric — an asymmetry would make dedup's output depend
        /// on provider ordering. Transitivity is deliberately NOT asserted:
        /// near-match chains are legitimately non-transitive under a similarity
        /// threshold, and demanding it would be demanding a different algorithm.
        #[test]
        fn same_work_is_symmetric(a in arb_book(), b in arb_book()) {
            prop_assert_eq!(same_work(&a, &b), same_work(&b, &a));
        }

        /// Dedup can only ever merge, never invent or lose a work.
        #[test]
        fn dedup_never_grows_and_never_empties(
            books in proptest::collection::vec(arb_book(), 0..8),
        ) {
            let raw: Vec<ProviderBook> = books
                .into_iter()
                .enumerate()
                .map(|(i, book)| ProviderBook {
                    book,
                    provider: if i % 2 == 0 {
                        ProviderId::OpenLibrary
                    } else {
                        ProviderId::GoogleBooks
                    },
                    position: i,
                })
                .collect();
            let n = raw.len();
            let out = dedup(raw);
            prop_assert!(out.len() <= n);
            prop_assert_eq!(out.is_empty(), n == 0);
        }

        /// Reordering the providers' replies must not change *which* works come
        /// out, nor the ISBN-bearing fields.
        ///
        /// Scoped to the `prefer`-managed fields on purpose. The `fill` fields
        /// (publisher, year, cover_url) are first-wins and therefore genuinely
        /// order-dependent by design — asserting full commutativity would fail
        /// for a reason that would then have to be relitigated rather than
        /// fixed.
        #[test]
        fn dedup_identity_is_permutation_invariant(
            books in proptest::collection::vec(arb_book(), 1..6),
        ) {
            let mk = |order: Vec<Book>| -> Vec<ProviderBook> {
                order
                    .into_iter()
                    .enumerate()
                    .map(|(i, book)| ProviderBook {
                        book,
                        provider: ProviderId::OpenLibrary,
                        position: i,
                    })
                    .collect()
            };
            let forward = dedup(mk(books.clone()));
            let mut reversed_input = books.clone();
            reversed_input.reverse();
            let backward = dedup(mk(reversed_input));

            prop_assert_eq!(
                forward.len(),
                backward.len(),
                "reordering changed how many distinct works were found"
            );

            let mut a: Vec<Option<String>> =
                forward.iter().map(|m| m.book.isbn_13.clone()).collect();
            let mut b: Vec<Option<String>> =
                backward.iter().map(|m| m.book.isbn_13.clone()).collect();
            a.sort();
            b.sort();
            prop_assert_eq!(a, b, "reordering changed the set of ISBNs");
        }
    }
}

/// Tests for the fan-out itself, which had no coverage at all: only the pure
/// `dedup`/`rank` helpers were exercised, so the 5s timeout and the
/// per-provider failure degradation had never once run.
#[cfg(test)]
mod fanout_tests {
    use super::*;
    use crate::error::EngineError;
    use async_trait::async_trait;

    enum Behaviour {
        Answers(Vec<Book>),
        /// Fails with this message. Carried rather than fixed so a test can
        /// steer the resulting `ErrorClass`.
        Fails(String),
        /// Sleeps past PROVIDER_TIMEOUT. Costs no real time under
        /// `start_paused` — tokio auto-advances its clock when every task is
        /// idle with a timer pending.
        Hangs,
    }

    struct FakeProvider {
        id: ProviderId,
        behaviour: Behaviour,
    }

    #[async_trait]
    impl MetadataProvider for FakeProvider {
        fn id(&self) -> ProviderId {
            self.id
        }
        async fn search(&self, _req: &SearchRequest) -> Result<Vec<ProviderBook>> {
            match &self.behaviour {
                Behaviour::Answers(books) => Ok(books
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(position, book)| ProviderBook {
                        book,
                        provider: self.id,
                        position,
                    })
                    .collect()),
                Behaviour::Fails(message) => Err(EngineError::Provider {
                    provider: self.id,
                    message: message.clone(),
                }),
                Behaviour::Hangs => {
                    tokio::time::sleep(PROVIDER_TIMEOUT * 2).await;
                    Ok(Vec::new())
                }
            }
        }
        async fn by_isbn(&self, _isbn: &str) -> Result<Option<ProviderBook>> {
            Ok(None)
        }
    }

    fn provider(id: ProviderId, behaviour: Behaviour) -> Box<dyn MetadataProvider> {
        Box::new(FakeProvider { id, behaviour })
    }

    fn book(title: &str) -> Book {
        Book {
            title: Some(title.into()),
            authors: vec!["Someone".into()],
            ..Default::default()
        }
    }

    fn req() -> SearchRequest {
        SearchRequest {
            query: Some("dune".into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn one_provider_failing_does_not_lose_the_other_s_results() {
        let providers = vec![
            provider(
                ProviderId::OpenLibrary,
                Behaviour::Fails("upstream exploded".into()),
            ),
            provider(
                ProviderId::GoogleBooks,
                Behaviour::Answers(vec![book("Dune"), book("Dune Messiah")]),
            ),
        ];
        let out = federated_search(&providers, &req()).await.unwrap();

        assert_eq!(
            out.results.len(),
            2,
            "surviving provider's results were lost"
        );
        assert_eq!(out.warnings.len(), 1);
        assert_eq!(out.failed_providers(), vec![ProviderId::OpenLibrary]);
        assert!(!out.timed_out(), "a failure is not a timeout");
    }

    /// The whole point of `start_paused`: a provider that sleeps twice the
    /// timeout resolves instantly here. This is why PROVIDER_TIMEOUT should not
    /// be widened into a config knob merely to make it testable.
    #[tokio::test(start_paused = true)]
    async fn a_hanging_provider_times_out_and_says_so() {
        let started = std::time::Instant::now();
        let providers = vec![
            provider(ProviderId::OpenLibrary, Behaviour::Hangs),
            provider(
                ProviderId::GoogleBooks,
                Behaviour::Answers(vec![book("Dune")]),
            ),
        ];
        let out = federated_search(&providers, &req()).await.unwrap();

        assert!(
            started.elapsed() < Duration::from_secs(1),
            "test actually waited {:?}; paused time is not in effect",
            started.elapsed()
        );
        assert_eq!(out.results.len(), 1);
        assert!(out.timed_out());
        assert_eq!(out.failed_providers(), vec![ProviderId::OpenLibrary]);
        // The exact user-visible string, unchanged by the typed-diagnostic
        // refactor.
        assert_eq!(out.warnings[0].to_string(), "openlibrary: timed out");
    }

    /// Every provider down is still a successful, empty search — never an Err.
    /// A dead network must not look like a bug in the app.
    #[tokio::test(start_paused = true)]
    async fn every_provider_down_is_still_ok_with_two_diagnostics() {
        let providers = vec![
            provider(ProviderId::OpenLibrary, Behaviour::Hangs),
            provider(
                ProviderId::GoogleBooks,
                Behaviour::Fails("upstream exploded".into()),
            ),
        ];
        let out = federated_search(&providers, &req())
            .await
            .expect("total provider failure must not be an Err");

        assert!(out.results.is_empty());
        assert_eq!(out.warnings.len(), 2);
        let mut failed = out.failed_providers();
        failed.sort();
        assert_eq!(
            failed,
            vec![ProviderId::OpenLibrary, ProviderId::GoogleBooks]
        );
    }

    #[tokio::test]
    async fn both_answering_dedups_and_credits_both_sources() {
        let providers = vec![
            provider(
                ProviderId::OpenLibrary,
                Behaviour::Answers(vec![book("Dune")]),
            ),
            provider(
                ProviderId::GoogleBooks,
                Behaviour::Answers(vec![book("Dune")]),
            ),
        ];
        let out = federated_search(&providers, &req()).await.unwrap();

        assert!(out.warnings.is_empty());
        assert_eq!(out.results.len(), 1, "the same work was not deduped");
        assert_eq!(
            out.results[0].sources.len(),
            2,
            "second source not credited"
        );
    }

    /// A keyless Google Books key hits quota routinely, and it is the one
    /// failure a frontend can act on ("add an API key"). It must survive the
    /// trip into a Diagnostic as something distinguishable, not as generic
    /// text.
    #[tokio::test]
    async fn a_quota_refusal_stays_recognisable_as_a_rate_limit() {
        let providers = vec![provider(
            ProviderId::GoogleBooks,
            Behaviour::Fails("HTTP 429 rate limit exceeded".into()),
        )];
        let out = federated_search(&providers, &req()).await.unwrap();

        assert_eq!(out.warnings.len(), 1);
        assert!(
            out.warnings[0].is_rate_limited(),
            "quota refusal lost its classification: {:?}",
            out.warnings[0]
        );
        assert!(!out.warnings[0].is_timeout());
    }

    #[tokio::test]
    async fn no_providers_at_all_is_an_empty_success() {
        let out = federated_search(&[], &req()).await.unwrap();
        assert!(out.results.is_empty() && out.warnings.is_empty());
    }

    /// Results come back ordered best-first, since the frontends render the
    /// list as given.
    #[tokio::test]
    async fn results_are_sorted_by_descending_score() {
        let providers = vec![provider(
            ProviderId::OpenLibrary,
            Behaviour::Answers(vec![book("Something Else"), book("Dune"), book("Dune Two")]),
        )];
        let out = federated_search(&providers, &req()).await.unwrap();
        let scores: Vec<f64> = out.results.iter().map(|r| r.score).collect();
        assert!(
            scores.windows(2).all(|w| w[0] >= w[1]),
            "not descending: {scores:?}"
        );
    }
}
