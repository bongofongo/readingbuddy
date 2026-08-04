//! Looking a book up again (item 30).
//!
//! `Storage::enrich_book` has existed since item 13 and **only calibre calls
//! it**. Every book created without an ISBN — `import_book_from_sidecar`, which
//! is offline by design and correctly so, and `import_file` matched on a
//! filename stem — has no cover, no description and no page count, permanently.
//! On a real library that is most of the shelf.
//!
//! This is the explicit action that fixes one of them. Not a sweep, not a
//! schedule, not a badge: `docs/decisions.md:231` puts provider enrichment on
//! the device pull path out of scope and that ruling stands, and the design
//! axiom bans a count of what the user has not done by name.
//!
//! ## Three things it must not get wrong
//!
//! **One matcher.** With no ISBN the book is matched against provider results
//! through [`crate::matching`] — the same `Prepared`/`compare`, the same
//! `AUTO_MATCH` and `CANDIDATE_MIN` bands a sidecar, a Goodreads row and a
//! calibre row go through. `docs/decisions.md` says "do not invent a second
//! matcher" under **Files** and it holds wherever books are matched.
//!
//! **The direction is the other way round, and that is the correction this item
//! forced.** Every other caller of the matcher asks *which library book is this
//! foreign row*, and pays for a scan of the whole shelf to answer it —
//! `calibre.rs` records that asking for the auto-match and the candidate band
//! separately read the shelf twice per row. Enrichment already knows the book;
//! what it does not know is which of a handful of provider results is the same
//! edition. So `Prepared` is built **once from the book we hold** and compared
//! against the search results, and the library is never scanned at all. The
//! shelf-scan trap does not bind here; the per-book cost is a provider fan-out,
//! which is why there is deliberately no bulk entry point.
//!
//! **Attribution is the whole reason item 29 went first.** A provider record is
//! *partial*, so it merges through `MERGE_RULES`' no-clobber and not the
//! device's straight assignment — but "which provider" has no single answer,
//! because `search`'s merge is field-wise (OpenLibrary wins the ISBNs and the
//! page count, Google Books wins the description and the language). Enriching
//! provider-by-provider with one `enrich_book` each would attribute honestly and
//! **lose that preference**, since whichever ran first would win every field it
//! spoke to — a second dialect of a merge that already exists. So the federated
//! merge keeps its values and now also reports its per-field claims
//! (`search::FieldClaims`), and one `enrich_book_attributed` writes the record
//! and stamps each field with the provider that actually supplied it.
//!
//! ## What it refuses
//!
//! Below `AUTO_MATCH` nothing is written and the candidates come back, the
//! refusal-with-a-next-move shape `ko pull` and `calibre import` already have.
//! The override is **not a flag on this command**: the user who has looked and
//! decided says so with `Engine::set_book_fields` — an ISBN, or a corrected
//! title — and runs this again. An `--accept 2` would index into a candidate
//! list produced by a *previous* provider search that this run cannot reproduce,
//! which is the one thing `ko link <path> <book>` and `calibre import --new` do
//! not do: their overrides name something durable.
//!
//! A field whose provenance is `user` is never overwritten. That guard lives in
//! the SQL `merge_set` generates and in `field_provenance::stamp`, from one
//! predicate; this module does not re-spell it. What it does is *report* it —
//! [`EnrichReport::held`] names the field, what the provider offered and what
//! was kept, because a field silently not updated is indistinguishable from a
//! field the provider had nothing for.

use std::collections::HashSet;

use crate::book::Book;
use crate::diagnostic::Diagnostic;
use crate::error::{EngineError, Result};
use crate::matching::{CANDIDATE_MIN, Prepared, Query};
use crate::providers::{MetadataProvider, ProviderBook, ProviderId, SearchRequest};
use crate::search::{self, FieldClaims};
use crate::storage::{Source, Storage, field_pair, field_value, merge_columns};

/// How the record that was written was found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnrichMatch {
    /// The book's own ISBN. Identity, not similarity — no band applies and no
    /// candidate is offered, because there is nothing to be unsure about.
    Isbn,
    /// A search result at or above `AUTO_MATCH`, carrying its score.
    Title(f64),
}

/// A provider record that looks like this book, but not enough to write from
/// without asking.
///
/// Carries the whole [`Book`] rather than a title and a score: the user is being
/// asked to recognise an *edition*, and the year, the publisher and the page
/// count are what tell two editions of one title apart. The ISBN is what makes
/// the choice actionable — it is the durable name the next move uses.
#[derive(Debug, Clone)]
pub struct EnrichCandidate {
    pub book: Book,
    pub score: f64,
    /// Which providers contributed to this record. Two agreeing is a signal the
    /// user can weigh.
    pub sources: Vec<ProviderId>,
}

/// What happened, at the granularity a frontend branches on.
#[derive(Debug, Clone)]
pub enum EnrichOutcome {
    /// A record was found and merged.
    Enriched(EnrichMatch),
    /// Nothing was written. `candidates` is the band — empty when results came
    /// back but none of them looked like this book at all, which is a different
    /// thing from [`EnrichOutcome::NothingFound`] and reads differently.
    Refused { candidates: Vec<EnrichCandidate> },
    /// Every provider answered, and none of them knows this book.
    NothingFound,
    /// Nothing came back and **somebody was never heard from**: a provider
    /// failed or timed out, so nothing here is an answer about the book.
    ///
    /// Found by the first hand-run, where OpenLibrary timed out and Google
    /// Books returned a keyless 429 and this reported *no provider knows this
    /// book* — a dead network wearing the shape of a fact, which is the one
    /// thing `docs/decisions.md`'s degradation rule exists to stop. It leans
    /// deliberately: one provider failing while the other returns nothing is
    /// still this, because "none of them knows it" cannot be said when one of
    /// them did not speak. Under-claiming has a next move (try again);
    /// over-claiming ends the conversation.
    NoAnswer,
    /// There is nothing to ask with: no ISBN, and no title to match on.
    /// Absence reported, never a guess.
    Unaskable,
}

/// One field that changed, and who is now answerable for it.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    pub field: &'static str,
    /// `None` when the field moved but no provider claimed it, which the merge
    /// should make impossible and which is reported rather than asserted.
    pub source: Option<Source>,
    /// What was there. `None` means the field was empty — the ordinary case for
    /// the books this item exists for.
    pub before: Option<String>,
    pub after: String,
}

/// One field a provider spoke to and was not allowed to write.
///
/// The only reason today is that the user owns it. It is a struct with a
/// `reason` rather than a bare list because the *offered* value is the part that
/// cannot be recovered any other way: it is gone the moment this call returns.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldField {
    pub field: &'static str,
    pub offered: String,
    pub offered_by: Option<Source>,
    /// What is kept instead. Always `Some` in practice — the user cannot own a
    /// field they never set — but the type does not know that.
    pub kept: Option<String>,
}

/// What one enrichment did.
///
/// Names every field **filled** and every field **held back**, so a frontend can
/// show what changed without re-querying, and so "the provider had nothing" and
/// "the provider had something and we refused it" are never the same silence.
#[derive(Debug, Clone)]
pub struct EnrichReport {
    pub book_id: i64,
    pub outcome: EnrichOutcome,
    pub filled: Vec<FieldChange>,
    pub held: Vec<HeldField>,
    /// A cover downloaded by this run. `None` both when the book already had one
    /// and when no record offered a URL.
    pub cover: Option<std::path::PathBuf>,
    /// Providers that failed or timed out. A dead provider degrades the answer;
    /// it never fails the call.
    pub warnings: Vec<Diagnostic>,
}

impl EnrichReport {
    fn empty(book_id: i64, outcome: EnrichOutcome) -> Self {
        Self {
            book_id,
            outcome,
            filled: Vec::new(),
            held: Vec::new(),
            cover: None,
            warnings: Vec::new(),
        }
    }

    /// Did anything at all change? `filled` alone under-reports: a run that
    /// downloaded only a cover changed the book.
    pub fn wrote_anything(&self) -> bool {
        !self.filled.is_empty() || self.cover.is_some()
    }

    /// The source this run attributed a field to, if it filled it. Used to
    /// attribute a downloaded cover file to whoever supplied its URL.
    pub(crate) fn source_of(&self, field: &str) -> Option<Source> {
        self.filled
            .iter()
            .find(|c| c.field == field)
            .and_then(|c| c.source)
    }
}

/// Ask every provider about one ISBN, keeping what each of them said.
///
/// `Engine::lookup_isbn` merges and returns a bare `Book`, which is the right
/// shape for `rb add` and the wrong one here: the merge is where the attribution
/// is decided, so it has to happen somewhere that can keep the answer. The
/// diagnostics are collected rather than only logged, which is the other half —
/// `lookup_isbn` has no channel for them and says so in its own comment.
pub(crate) async fn lookup_isbn_raw(
    providers: &[Box<dyn MetadataProvider>],
    isbn: &str,
) -> (Vec<ProviderBook>, Vec<Diagnostic>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    for p in providers.iter() {
        match p.by_isbn(isbn).await {
            Ok(Some(pb)) => found.push(pb),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    provider = %p.id(),
                    error = %e,
                    "provider lookup failed; continuing with the others"
                );
                warnings.push(Diagnostic::provider_failed(p.id(), &e));
            }
        }
    }
    (found, warnings)
}

/// The metadata half of [`crate::Engine::enrich_book_from_providers`].
///
/// A free function over `&Storage` and a provider slice, not a method: that is
/// what lets the whole of it — the ISBN path, the band, the refusal, the user
/// guard — be tested against a mock `MetadataProvider` with no network and no
/// `Engine`, which `CLAUDE.md` requires and which a method on the facade would
/// have made impossible without one.
///
/// The cover is deliberately not here. Downloading needs the shared HTTP client
/// and the images directory, both of which live on the facade; keeping this half
/// pure metadata is what keeps it testable.
pub(crate) async fn enrich_metadata(
    storage: &Storage,
    providers: &[Box<dyn MetadataProvider>],
    book_id: i64,
) -> Result<EnrichReport> {
    let before = storage
        .get_book(book_id)
        .await?
        .ok_or_else(|| EngineError::NotFound(format!("book id {book_id}")))?;

    // ISBN first, and there is no fallback from it to a title search: an ISBN is
    // an identity, so "no provider knows this ISBN" is an answer about *this
    // edition*. Falling through to a title match would answer a different
    // question — some edition of this work — and write its page count over an
    // edition-specific one, silently.
    let isbn = before
        .isbn_13
        .as_deref()
        .or(before.isbn_10.as_deref())
        .map(str::to_string);

    let (found, mut warnings, outcome) = match isbn {
        Some(isbn) => {
            let (raw, warnings) = lookup_isbn_raw(providers, &isbn).await;
            match search::merge_provider_books_claimed(raw) {
                Some(found) => (
                    Some(found),
                    warnings,
                    EnrichOutcome::Enriched(EnrichMatch::Isbn),
                ),
                None => {
                    let outcome = silence(&warnings);
                    return Ok(EnrichReport {
                        warnings,
                        ..EnrichReport::empty(book_id, outcome)
                    });
                }
            }
        }
        None => match by_title(providers, &before).await? {
            TitleSearch::Unaskable => {
                return Ok(EnrichReport::empty(book_id, EnrichOutcome::Unaskable));
            }
            TitleSearch::Refused {
                candidates,
                any_results,
                warnings,
            } => {
                let outcome = if any_results {
                    EnrichOutcome::Refused { candidates }
                } else {
                    silence(&warnings)
                };
                return Ok(EnrichReport {
                    warnings,
                    ..EnrichReport::empty(book_id, outcome)
                });
            }
            TitleSearch::Matched {
                found,
                score,
                warnings,
            } => (
                Some(found),
                warnings,
                EnrichOutcome::Enriched(EnrichMatch::Title(score)),
            ),
        },
    };

    let Some((record, claims)) = found else {
        return Ok(EnrichReport {
            warnings,
            ..EnrichReport::empty(book_id, EnrichOutcome::NothingFound)
        });
    };

    // What the user owns, read *before* the write. The guard itself is in the
    // generated SQL; this is the reporting half, and it needs the stored value
    // while it is still the stored value.
    let owned: HashSet<String> = storage
        .field_provenance(book_id)
        .await?
        .into_iter()
        .filter(|f| f.source == Source::User.as_str())
        .map(|f| f.field)
        .collect();

    let mut held = Vec::new();
    let mut claim_list: Vec<(&'static str, Source)> = Vec::new();
    for field in merge_columns() {
        let Some(offered) = field_value(&record, field) else {
            continue;
        };
        if let Some(source) = claims.source_of(field) {
            // Every claim goes through, user-owned fields included: `stamp`
            // already refuses those, and filtering here would be the guard
            // spelled a second time in a caller — which is exactly the shape
            // item 29 moved into the generated SQL to stop.
            claim_list.push((field, source.into()));
        }
        let kept = field_value(&before, field);
        // **Owning half a pair holds the whole pair** (`Rule::pair`), so the
        // report has to read ownership the same way the merge clause does.
        // Otherwise a `series_index` held back because the user named the
        // series is a field that silently did not update — precisely what this
        // list exists to make impossible.
        let held_back =
            owned.contains(field) || field_pair(field).is_some_and(|p| owned.contains(p));
        if held_back && kept.as_deref() != Some(offered.as_str()) {
            held.push(HeldField {
                field,
                offered,
                offered_by: claims.source_of(field).map(Source::from),
                kept,
            });
        }
    }

    storage
        .enrich_book_attributed(book_id, &record, &claim_list)
        .await?;

    let after = storage
        .get_book(book_id)
        .await?
        .ok_or_else(|| EngineError::NotFound(format!("book id {book_id}")))?;
    let filled = changes(&before, &after, &claims);

    warnings.shrink_to_fit();
    Ok(EnrichReport {
        book_id,
        outcome,
        filled,
        held,
        cover: None,
        warnings,
    })
}

/// What an empty answer means, which depends on whether anyone was heard from.
///
/// "No provider knows this book" is a claim about the book. It can only be made
/// when every provider actually answered — otherwise the honest report is that
/// we did not find out, and the difference is the whole reason `Diagnostic`
/// exists rather than a `Vec<String>` nobody branches on.
fn silence(warnings: &[Diagnostic]) -> EnrichOutcome {
    if warnings.is_empty() {
        EnrichOutcome::NothingFound
    } else {
        EnrichOutcome::NoAnswer
    }
}

/// Every merge column whose value moved, with the provider now answerable.
///
/// A diff of the stored row against itself, rather than a prediction made from
/// the record: the merge is three shapes of clause and a user guard, and a
/// report that guessed at its outcome would be a fourth statement of the rule
/// that could disagree with the other three.
fn changes(before: &Book, after: &Book, claims: &FieldClaims) -> Vec<FieldChange> {
    merge_columns()
        .filter_map(|field| {
            let before_value = field_value(before, field);
            let after_value = field_value(after, field)?;
            if before_value.as_deref() == Some(after_value.as_str()) {
                return None;
            }
            Some(FieldChange {
                field,
                source: claims.source_of(field).map(Source::from),
                before: before_value,
                after: after_value,
            })
        })
        .collect()
}

/// The three answers a title search can give, before any of them touches the
/// database.
///
/// `large_enum_variant` is allowed rather than boxed: exactly one of these is
/// built per call and destructured in the next statement, so the size is one
/// stack move on a path that has just awaited a network fan-out. A `Box` here
/// would trade a real allocation for a cost that is not being paid.
#[allow(clippy::large_enum_variant)]
enum TitleSearch {
    Unaskable,
    Refused {
        candidates: Vec<EnrichCandidate>,
        /// Whether any provider returned anything at all, which is what
        /// separates "nothing found" from "found things, none of them this
        /// book". Both write nothing; they do not read the same to a user.
        any_results: bool,
        warnings: Vec<Diagnostic>,
    },
    Matched {
        found: (Book, FieldClaims),
        score: f64,
        warnings: Vec<Diagnostic>,
    },
}

/// Search on what the book says about itself, and score the answers with the
/// one matcher.
///
/// The request carries the **first** author only. `SearchRequest::author` is one
/// field and `rank` scores it as the best match over the record's authors, so
/// joining several would score worse than either alone against every provider
/// result; the veto side of `matching` still sees the whole list, which is where
/// a second author actually matters.
async fn by_title(providers: &[Box<dyn MetadataProvider>], before: &Book) -> Result<TitleSearch> {
    let query = Query::new(before.title.as_deref(), &before.authors);
    // Built once, from the book we hold. This is the whole scan: there are a
    // handful of provider results, not a library.
    let Some(prepared) = Prepared::new(&query) else {
        return Ok(TitleSearch::Unaskable);
    };

    let req = SearchRequest {
        title: before.title.clone(),
        author: before.authors.first().cloned(),
        ..Default::default()
    };
    let (results, warnings) = search::federated_search_claimed(providers, &req).await?;
    let any_results = !results.is_empty();
    // `compare` returning `None` is the half that matters: a coincidence of
    // letters is absent rather than present with a plausible number, so a
    // result that survives here is one the matcher is willing to talk about.
    let mut scored: Vec<(usize, f64, bool)> = results
        .iter()
        .enumerate()
        .filter_map(|(i, (r, _))| prepared.compare(&r.book).map(|v| (i, v.score, v.can_auto)))
        .collect();
    // Ties break on the federated ranking's own order, which is deterministic —
    // provider records have no id to fall back on the way `scores_for`'s library
    // books do.
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));

    // One pass over the results, taking each out by index in score order. `nth`
    // on the iterator would consume everything before it and could not then
    // reach the second candidate.
    let mut results: Vec<Option<(search::RankedResult, FieldClaims)>> =
        results.into_iter().map(Some).collect();
    let take = |results: &mut Vec<Option<(search::RankedResult, FieldClaims)>>, i: usize| {
        results[i].take().expect("each index is visited once")
    };

    if let Some(&(i, score, true)) = scored.first() {
        let (result, claims) = take(&mut results, i);
        return Ok(TitleSearch::Matched {
            found: (result.book, claims),
            score,
            warnings,
        });
    }

    let candidates = scored
        .iter()
        .filter(|(_, score, can_auto)| !can_auto && *score >= CANDIDATE_MIN)
        .map(|&(i, score, _)| {
            let (result, _) = take(&mut results, i);
            EnrichCandidate {
                book: result.book,
                score,
                sources: result.sources,
            }
        })
        .collect();

    Ok(TitleSearch::Refused {
        candidates,
        any_results,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// A provider that answers from a script. `CLAUDE.md`: the fan-out is tested
    /// with a mock `MetadataProvider` and there is no network in tests, ever.
    struct Fake {
        id: ProviderId,
        by_isbn: Option<Book>,
        results: Vec<Book>,
        fails: bool,
    }

    impl Fake {
        fn new(id: ProviderId) -> Self {
            Fake {
                id,
                by_isbn: None,
                results: Vec::new(),
                fails: false,
            }
        }
        fn isbn(mut self, book: Book) -> Self {
            self.by_isbn = Some(book);
            self
        }
        fn finds(mut self, books: Vec<Book>) -> Self {
            self.results = books;
            self
        }
        fn broken(mut self) -> Self {
            self.fails = true;
            self
        }
        fn boxed(self) -> Box<dyn MetadataProvider> {
            Box::new(self)
        }
    }

    #[async_trait]
    impl MetadataProvider for Fake {
        fn id(&self) -> ProviderId {
            self.id
        }
        async fn search(&self, _req: &SearchRequest) -> Result<Vec<ProviderBook>> {
            if self.fails {
                return Err(EngineError::Provider {
                    provider: self.id,
                    message: "upstream exploded".into(),
                });
            }
            Ok(self
                .results
                .iter()
                .cloned()
                .enumerate()
                .map(|(position, book)| ProviderBook {
                    book,
                    provider: self.id,
                    position,
                })
                .collect())
        }
        async fn by_isbn(&self, _isbn: &str) -> Result<Option<ProviderBook>> {
            if self.fails {
                return Err(EngineError::Provider {
                    provider: self.id,
                    message: "upstream exploded".into(),
                });
            }
            Ok(self.by_isbn.clone().map(|book| ProviderBook {
                book,
                provider: self.id,
                position: 0,
            }))
        }
    }

    /// The book this whole item exists for: created from a sidecar, so no ISBN,
    /// no cover, no description and no page count.
    async fn sidecar_seeded(title: &str, authors: &[&str]) -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some(title.into()),
                    authors: authors.iter().map(|a| (*a).to_string()).collect(),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        (s, id)
    }

    fn record(title: &str, authors: &[&str]) -> Book {
        Book {
            title: Some(title.into()),
            authors: authors.iter().map(|a| (*a).to_string()).collect(),
            ..Default::default()
        }
    }

    fn filled(report: &EnrichReport, field: &str) -> Option<String> {
        report
            .filled
            .iter()
            .find(|c| c.field == field)
            .map(|c| c.after.clone())
    }

    /// **The test that decides the design of this item.**
    ///
    /// Both providers answer about the same ISBN and disagree about the page
    /// count and the description. `search`'s merge is field-wise — OpenLibrary
    /// wins the page count, Google Books wins the description — and the
    /// provenance has to follow the *value*, per field.
    ///
    /// Enriching provider-by-provider with one `enrich_book` each would
    /// attribute honestly and get the **values** wrong: no-clobber means
    /// whichever ran first wins every field it spoke to, so the description
    /// would be OpenLibrary's. That is a second dialect of a merge that already
    /// exists, and this assertion is what forbids it.
    #[tokio::test]
    async fn each_field_is_attributed_to_the_provider_that_actually_supplied_it() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    isbn_13: Some("9781455563937".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .isbn(Book {
                    page_count: Some(490),
                    description: Some("the openlibrary blurb".into()),
                    publisher: Some("Grand Central".into()),
                    ..record("Pachinko", &["Min Jin Lee"])
                })
                .boxed(),
            Fake::new(ProviderId::GoogleBooks)
                .isbn(Book {
                    page_count: Some(512),
                    description: Some("the google books blurb".into()),
                    language: Some("en".into()),
                    ..record("Pachinko", &["Min Jin Lee"])
                })
                .boxed(),
        ];

        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        assert!(matches!(
            report.outcome,
            EnrichOutcome::Enriched(EnrichMatch::Isbn)
        ));

        let book = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(
            book.page_count,
            Some(490),
            "OpenLibrary wins the page count"
        );
        assert_eq!(
            book.description.as_deref(),
            Some("the google books blurb"),
            "Google Books wins the description"
        );

        assert_eq!(
            s.field_source(id, "page_count").await.unwrap().as_deref(),
            Some("openlibrary")
        );
        assert_eq!(
            s.field_source(id, "description").await.unwrap().as_deref(),
            Some("googlebooks")
        );
        // A field only one of them supplied is claimed by that one, not by
        // whichever answered first.
        assert_eq!(
            s.field_source(id, "language").await.unwrap().as_deref(),
            Some("googlebooks")
        );
        assert_eq!(
            s.field_source(id, "publisher").await.unwrap().as_deref(),
            Some("openlibrary")
        );

        // …and the report says the same thing without a second query.
        assert_eq!(filled(&report, "page_count").as_deref(), Some("490"));
        let page = report
            .filled
            .iter()
            .find(|c| c.field == "page_count")
            .unwrap();
        assert_eq!(page.source, Some(Source::OpenLibrary));
        assert_eq!(page.before, None, "the field was empty");
    }

    /// No ISBN is the ordinary case for a sidecar-seeded book, and the match is
    /// made by the one matcher rather than by trusting the search ranking.
    #[tokio::test]
    async fn a_book_with_no_isbn_is_matched_by_title_and_enriched() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![Book {
                    page_count: Some(490),
                    isbn_13: Some("9781455563937".into()),
                    ..record("Pachinko", &["Min Jin Lee"])
                }])
                .boxed(),
        ];

        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        match report.outcome {
            EnrichOutcome::Enriched(EnrichMatch::Title(score)) => {
                assert!(score >= crate::matching::AUTO_MATCH, "score {score}")
            }
            other => panic!("expected a title match, got {other:?}"),
        }
        let book = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(book.page_count, Some(490));
        assert_eq!(book.isbn_13.as_deref(), Some("9781455563937"));
    }

    /// Below `AUTO_MATCH`: **nothing is written**, and the candidates come back
    /// so the refusal has a next move. `Pachinko` against `Pachinko: A Novel of
    /// Korea and Japan` is the band case `matching.rs` pins by name.
    #[tokio::test]
    async fn a_near_miss_writes_nothing_and_comes_back_as_a_candidate() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![Book {
                    page_count: Some(490),
                    isbn_13: Some("9781455563937".into()),
                    ..record("Pachinko: A Novel of Korea and Japan", &["Min Jin Lee"])
                }])
                .boxed(),
        ];

        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        match &report.outcome {
            EnrichOutcome::Refused { candidates } => {
                assert_eq!(candidates.len(), 1);
                assert!(candidates[0].score >= CANDIDATE_MIN);
                assert!(candidates[0].score < crate::matching::AUTO_MATCH);
                assert_eq!(
                    candidates[0].book.isbn_13.as_deref(),
                    Some("9781455563937"),
                    "the candidate carries the durable name the next move uses"
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert!(report.filled.is_empty());
        let book = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(book.page_count, None, "a refusal writes nothing");
        assert!(s.field_provenance(id).await.unwrap().is_empty());
    }

    /// A coincidence of letters is **absent**, not a low-scoring candidate —
    /// that is the half of `matching.rs` that matters, and a refusal with an
    /// empty band reads differently from one with something to show.
    #[tokio::test]
    async fn an_unrelated_result_is_not_offered_at_all() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![record("Pinocchio", &["Carlo Collodi"])])
                .boxed(),
        ];

        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        match &report.outcome {
            EnrichOutcome::Refused { candidates } => assert!(candidates.is_empty()),
            other => panic!("expected an empty refusal, got {other:?}"),
        }
    }

    /// Nothing came back at all. Distinct from a refusal, because "no provider
    /// knows this book" and "none of these is certainly it" are different facts
    /// with different next moves.
    #[tokio::test]
    async fn nothing_found_is_not_a_refusal() {
        let (s, id) = sidecar_seeded("An Unpublished Manuscript", &[]).await;
        let providers = vec![Fake::new(ProviderId::OpenLibrary).boxed()];
        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        assert!(matches!(report.outcome, EnrichOutcome::NothingFound));

        // Same for the ISBN path: an ISBN nobody knows is an answer about this
        // edition, and there is deliberately no fall-through to a title search.
        let s2 = Storage::connect("sqlite::memory:").await.unwrap();
        let id2 = s2
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    isbn_13: Some("9781455563937".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![record("Pachinko", &["Min Jin Lee"])])
                .boxed(),
        ];
        let report = enrich_metadata(&s2, &providers, id2).await.unwrap();
        assert!(
            matches!(report.outcome, EnrichOutcome::NothingFound),
            "an unknown ISBN must not fall through to the title search: {:?}",
            report.outcome
        );
    }

    /// **A dead network is not an answer about the book.**
    ///
    /// Found by the first hand-run: OpenLibrary timed out, Google Books returned
    /// a keyless 429, and this reported "no provider knows this book" — which is
    /// a claim about the book made out of two failures to reach anyone. It
    /// leans, on purpose: one provider failing while the other returns nothing
    /// is still `NoAnswer`, because "none of them knows it" cannot be said when
    /// one of them did not speak.
    #[tokio::test]
    async fn every_provider_falling_over_is_no_answer_not_nothing_found() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        let both_down = vec![
            Fake::new(ProviderId::OpenLibrary).broken().boxed(),
            Fake::new(ProviderId::GoogleBooks).broken().boxed(),
        ];
        let report = enrich_metadata(&s, &both_down, id).await.unwrap();
        assert!(
            matches!(report.outcome, EnrichOutcome::NoAnswer),
            "{:?}",
            report.outcome
        );
        assert_eq!(report.warnings.len(), 2);
        assert!(report.filled.is_empty());

        // One down, one answering with nothing: still not a fact about the book.
        let half = vec![
            Fake::new(ProviderId::OpenLibrary).broken().boxed(),
            Fake::new(ProviderId::GoogleBooks).boxed(),
        ];
        let report = enrich_metadata(&s, &half, id).await.unwrap();
        assert!(matches!(report.outcome, EnrichOutcome::NoAnswer));

        // The ISBN path takes the same care, and it is the path where a wrong
        // "nobody knows this" would send the user editing a correct ISBN.
        let s2 = Storage::connect("sqlite::memory:").await.unwrap();
        let id2 = s2
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    isbn_13: Some("9781455563937".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let providers = vec![Fake::new(ProviderId::OpenLibrary).broken().boxed()];
        let report = enrich_metadata(&s2, &providers, id2).await.unwrap();
        assert!(matches!(report.outcome, EnrichOutcome::NoAnswer));
    }

    /// Nothing to ask with. Absence is reported, never guessed at.
    #[tokio::test]
    async fn a_book_with_no_isbn_and_no_title_is_unaskable() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&Book::default(), None).await.unwrap();
        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![record("Anything At All", &[])])
                .boxed(),
        ];
        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        assert!(matches!(report.outcome, EnrichOutcome::Unaskable));
        assert!(report.filled.is_empty());
    }

    /// **The rule item 29 exists for, exercised by its first real caller.**
    ///
    /// The value guard is in the generated SQL and the stamp guard is in
    /// `field_provenance::stamp`; what this item owes is that the held-back
    /// field is *reported* with both values. A field silently not updated is
    /// indistinguishable from a field the provider had nothing for.
    #[tokio::test]
    async fn a_user_owned_field_is_held_back_and_the_report_says_what_was_offered() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        // The user counted the pages themselves.
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

        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![Book {
                    page_count: Some(490),
                    description: Some("a sweeping saga".into()),
                    ..record("Pachinko", &["Min Jin Lee"])
                }])
                .boxed(),
        ];
        let report = enrich_metadata(&s, &providers, id).await.unwrap();

        let book = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(book.page_count, Some(496), "the correction survives");
        assert_eq!(
            s.field_source(id, "page_count").await.unwrap().as_deref(),
            Some("user"),
            "and the provider does not take the claim either"
        );
        // The field it had nothing to correct still lands.
        assert_eq!(book.description.as_deref(), Some("a sweeping saga"));

        assert_eq!(report.held.len(), 1, "{:?}", report.held);
        let held = &report.held[0];
        assert_eq!(held.field, "page_count");
        assert_eq!(held.offered, "490");
        assert_eq!(held.kept.as_deref(), Some("496"));
        assert_eq!(held.offered_by, Some(Source::OpenLibrary));
        assert!(
            !report.filled.iter().any(|c| c.field == "page_count"),
            "a held field is not also reported as filled"
        );
    }

    /// **A held *pair* is reported as held, not silently skipped.**
    ///
    /// The user names the series and never says which number it is. A provider
    /// that knows a different series then offers an index, and the merge refuses
    /// it — because "Dune" plus "#2 of Dune Chronicles" is a row that
    /// contradicts itself. The report has to say so: an index that quietly did
    /// not land looks exactly like a provider that had none, which is the state
    /// `held` exists to remove.
    #[tokio::test]
    async fn an_index_held_because_the_user_owns_the_series_is_reported() {
        let (s, id) = sidecar_seeded("Dune Messiah", &["Frank Herbert"]).await;
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

        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![Book {
                    series: Some("Dune Chronicles".into()),
                    series_index: Some(2.0),
                    ..record("Dune Messiah", &["Frank Herbert"])
                }])
                .boxed(),
        ];
        let report = enrich_metadata(&s, &providers, id).await.unwrap();

        let book = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(book.series.as_deref(), Some("Dune"));
        assert_eq!(book.series_index, None);

        let mut held: Vec<&str> = report.held.iter().map(|h| h.field).collect();
        held.sort_unstable();
        assert_eq!(held, vec!["series", "series_index"], "{:?}", report.held);
        let index = report
            .held
            .iter()
            .find(|h| h.field == "series_index")
            .unwrap();
        assert_eq!(index.offered, "#2");
        assert_eq!(
            index.kept, None,
            "nothing was kept — the field is empty and stays empty"
        );
        assert!(
            !report.filled.iter().any(|c| c.field == "series_index"),
            "a held field is not also reported as filled"
        );
    }

    /// A provider offering exactly what the user already holds is **not** held
    /// back — nothing was withheld, so saying so would be noise on every run.
    #[tokio::test]
    async fn agreeing_with_the_user_is_not_a_held_field() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        s.enrich_book(
            id,
            &Book {
                page_count: Some(490),
                ..Default::default()
            },
            Some(Source::User),
        )
        .await
        .unwrap();
        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![Book {
                    page_count: Some(490),
                    ..record("Pachinko", &["Min Jin Lee"])
                }])
                .boxed(),
        ];
        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        assert!(report.held.is_empty(), "{:?}", report.held);
    }

    /// Running it twice fills nothing the second time. Not an incidental
    /// property: the report is what a frontend renders as "what changed", so a
    /// second run reporting sixteen changes would be a screen full of noise
    /// claiming work that did not happen.
    #[tokio::test]
    async fn enriching_twice_reports_nothing_the_second_time() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        let make = || {
            vec![
                Fake::new(ProviderId::OpenLibrary)
                    .finds(vec![Book {
                        page_count: Some(490),
                        description: Some("a sweeping saga".into()),
                        ..record("Pachinko", &["Min Jin Lee"])
                    }])
                    .boxed(),
            ]
        };

        let first = enrich_metadata(&s, &make(), id).await.unwrap();
        assert!(!first.filled.is_empty());
        assert!(first.wrote_anything());

        let second = enrich_metadata(&s, &make(), id).await.unwrap();
        assert!(
            second.filled.is_empty(),
            "second run reported changes: {:?}",
            second.filled
        );
        assert!(!second.wrote_anything());
        assert!(second.held.is_empty());
    }

    /// One provider down must degrade, never abort — and the diagnostic reaches
    /// the report rather than only the log, which is what `lookup_isbn` cannot
    /// do and says so.
    #[tokio::test]
    async fn a_provider_that_fails_degrades_to_a_warning() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    isbn_13: Some("9781455563937".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let providers = vec![
            Fake::new(ProviderId::OpenLibrary).broken().boxed(),
            Fake::new(ProviderId::GoogleBooks)
                .isbn(Book {
                    page_count: Some(512),
                    ..record("Pachinko", &["Min Jin Lee"])
                })
                .boxed(),
        ];

        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.warnings[0].provider(), Some(ProviderId::OpenLibrary));
        assert_eq!(filled(&report, "page_count").as_deref(), Some("512"));
        assert_eq!(
            s.field_source(id, "page_count").await.unwrap().as_deref(),
            Some("googlebooks"),
            "the survivor is credited, not the one that fell over"
        );
    }

    /// A provider replacing a value we already had says so in both directions:
    /// the old value is in the report, and the claim moves.
    #[tokio::test]
    async fn a_replaced_value_reports_what_it_replaced() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        s.enrich_book(
            id,
            &Book {
                publisher: Some("Unknown".into()),
                ..Default::default()
            },
            Some(Source::KOReader),
        )
        .await
        .unwrap();

        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![Book {
                    publisher: Some("Grand Central".into()),
                    ..record("Pachinko", &["Min Jin Lee"])
                }])
                .boxed(),
        ];
        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        let change = report
            .filled
            .iter()
            .find(|c| c.field == "publisher")
            .expect("publisher changed");
        assert_eq!(change.before.as_deref(), Some("Unknown"));
        assert_eq!(change.after, "Grand Central");
        assert_eq!(change.source, Some(Source::OpenLibrary));
        assert_eq!(
            s.field_source(id, "publisher").await.unwrap().as_deref(),
            Some("openlibrary")
        );
    }

    /// Enrichment must never invent a book. A missing one is a real error, not
    /// an empty report — every other outcome here means "we looked".
    #[tokio::test]
    async fn a_book_that_does_not_exist_is_an_error() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let providers: Vec<Box<dyn MetadataProvider>> = Vec::new();
        assert!(matches!(
            enrich_metadata(&s, &providers, 9999).await,
            Err(EngineError::NotFound(_))
        ));
    }

    /// The library is never scanned. This is the correction the item forced: the
    /// matcher is pointed the other way round from every other caller, so the
    /// shelf-scan cost `calibre.rs` records does not arise — enriching one book
    /// of a thousand compares against a handful of provider results, not against
    /// a thousand rows.
    #[tokio::test]
    async fn enriching_one_book_does_not_read_the_shelf() {
        let (s, id) = sidecar_seeded("Pachinko", &["Min Jin Lee"]).await;
        for n in 0..50 {
            s.upsert_book(&record(&format!("Decoy {n}"), &["Nobody"]), None)
                .await
                .unwrap();
        }
        let providers = vec![
            Fake::new(ProviderId::OpenLibrary)
                .finds(vec![Book {
                    page_count: Some(490),
                    ..record("Pachinko", &["Min Jin Lee"])
                }])
                .boxed(),
        ];

        let report = enrich_metadata(&s, &providers, id).await.unwrap();
        assert!(matches!(
            report.outcome,
            EnrichOutcome::Enriched(EnrichMatch::Title(_))
        ));
        // The decoys are untouched: a match against the *library* would have had
        // to consider them, and one of fifty scoring into the band is the
        // extreme-value draw `matching.rs` was rewritten over.
        assert_eq!(s.get_book(id).await.unwrap().unwrap().page_count, Some(490));
        for book in s
            .list_books(100, crate::storage::BookSort::Title)
            .await
            .unwrap()
        {
            if book
                .title
                .as_deref()
                .is_some_and(|t| t.starts_with("Decoy"))
            {
                assert_eq!(book.page_count, None, "{:?} was written to", book.title);
            }
        }
    }
}
