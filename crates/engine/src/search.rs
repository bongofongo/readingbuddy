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
    if let Some(first) = words.first()
        && matches!(*first, "the" | "a" | "an")
    {
        words.remove(0);
    }
    words.join(" ")
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
