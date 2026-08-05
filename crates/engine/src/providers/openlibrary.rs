use async_trait::async_trait;
use futures::future::try_join_all;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{
    MetadataProvider, ProviderBook, ProviderId, SearchRequest, normalize_language,
    normalize_subjects, to_marc_language, year_of_date,
};
use crate::book::{Book, normalize_isbn};
use crate::error::{EngineError, Result};

pub struct OpenLibraryProvider {
    client: Client,
}

impl OpenLibraryProvider {
    pub fn new(client: Client) -> Self {
        OpenLibraryProvider { client }
    }
}

// ---- /search.json ----------------------------------------------------------

#[derive(Deserialize, Debug)]
struct SearchResp {
    docs: Option<Vec<Works>>,
}

#[derive(Deserialize, Debug, Default)]
struct Works {
    title: Option<String>,
    author_name: Option<Vec<String>>,
    first_publish_year: Option<i64>,
    cover_edition_key: Option<String>,
    key: Option<String>,
    language: Option<Vec<String>>,
    isbn: Option<Vec<String>>,
    first_sentence: Option<Vec<String>>,
    publisher: Option<Vec<String>>,
    number_of_pages_median: Option<i64>,
}

impl Works {
    /// `-L`, not `-M` (item 20c).
    ///
    /// OpenLibrary publishes three sizes off one key: `-S` (~40px), `-M`
    /// (~180px) and `-L` (whatever the scan actually is, usually 500px+). `-M`
    /// was picked when the only consumer was a terminal drawing a book in
    /// Unicode half-blocks, where 180px is already more than the framebuffer
    /// can show. A cover-forward detail page on a 2× display is not that
    /// consumer, and there is no fourth size to ask for.
    ///
    /// The shelf does not pay for it: `images::store_cover` writes a downscaled
    /// tier beside anything over `THUMB_MAX` and `Book::shelf_cover_path` is
    /// what a grid loads. Asking for the large size *without* that tier would
    /// make every list strictly heavier than it was, which is why the two
    /// halves of 20c are one change and not two.
    fn cover_url(&self) -> Option<String> {
        self.cover_edition_key
            .as_deref()
            .map(|k| format!("https://covers.openlibrary.org/b/olid/{k}-L.jpg"))
    }

    fn to_book(&self) -> Book {
        // Scan the isbn list for the first valid 10 and first valid 13.
        let mut isbn_10 = None;
        let mut isbn_13 = None;
        for raw in self.isbn.iter().flatten() {
            if let Some(norm) = normalize_isbn(raw) {
                match norm.len() {
                    10 if isbn_10.is_none() => isbn_10 = Some(norm),
                    13 if isbn_13.is_none() => isbn_13 = Some(norm),
                    _ => {}
                }
            }
            if isbn_10.is_some() && isbn_13.is_some() {
                break;
            }
        }
        Book {
            title: self.title.clone(),
            authors: self.author_name.clone().unwrap_or_default(),
            publish_year: self.first_publish_year,
            language: self
                .language
                .as_ref()
                .and_then(|v| v.first())
                .map(|l| normalize_language(l)),
            isbn_10,
            isbn_13,
            openlibrary_key: self.key.clone(),
            publisher: self.publisher.as_ref().and_then(|v| v.first().cloned()),
            page_count: self.number_of_pages_median,
            first_sentence: self
                .first_sentence
                .as_ref()
                .and_then(|v| v.first().cloned()),
            cover_url: self.cover_url(),
            ..Default::default()
        }
    }
}

fn build_search_url(req: &SearchRequest) -> Result<String> {
    let mut params: Vec<(&str, String)> = Vec::new();
    let mut q_parts: Vec<String> = Vec::new();

    if let Some(q) = &req.query {
        q_parts.push(q.clone());
    }
    if let Some(t) = &req.translator {
        // No translator facet; contributor is the closest solr field.
        q_parts.push(format!("contributor:\"{t}\""));
    }
    if !q_parts.is_empty() {
        params.push(("q", q_parts.join(" ")));
    }
    if let Some(t) = &req.title {
        params.push(("title", t.clone()));
    }
    if let Some(a) = &req.author {
        params.push(("author", a.clone()));
    }
    if let Some(p) = &req.publisher {
        params.push(("publisher", p.clone()));
    }
    if let Some(l) = &req.language {
        params.push(("lang", to_marc_language(l)));
    }
    if let Some(i) = &req.isbn {
        params.push(("q", format!("isbn:{i}")));
    }

    let limit = if req.limit == 0 { 30 } else { req.limit };
    let mut url = Url::parse_with_params("https://openlibrary.org/search.json", &params)?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string())
        .append_pair(
            "fields",
            "key,title,author_name,isbn,language,first_sentence,first_publish_year,\
             cover_edition_key,publisher,number_of_pages_median",
        );
    Ok(url.into())
}

// ---- /isbn/{isbn}.json (edition lookup) ------------------------------------

#[derive(Deserialize, Debug)]
struct Key {
    key: Option<String>,
}

#[derive(Deserialize, Debug)]
struct AuthorJson {
    name: Option<String>,
}

#[derive(Deserialize, Debug)]
struct EditionJson {
    #[serde(alias = "author")]
    authors: Option<Vec<Key>>,
    title: Option<String>,
    isbn_10: Option<Vec<String>>,
    isbn_13: Option<Vec<String>>,
    publish_date: Option<String>,
    publishers: Option<Vec<String>>,
    number_of_pages: Option<i64>,
    pagination: Option<String>,
    key: Option<String>,
    languages: Option<Vec<Key>>,
    covers: Option<Vec<i64>>,
    /// Free text, one entry per series the *edition* belongs to:
    /// `["Dune Chronicles #2"]`, `["Penguin classics"]`. Only the first is
    /// read — a book in two series is a shelving question and this column is a
    /// scalar pair.
    series: Option<Vec<String>>,
    /// The work(s) this edition realises. Subjects live on the **work**, never
    /// on the edition, which is why they cost a second request.
    works: Option<Vec<Key>>,
}

/// The slice of `/works/{key}.json` this reads. Nothing else on a work is
/// wanted: title, description and covers are edition-level facts here and the
/// edition record already answered them.
#[derive(Deserialize, Debug)]
struct WorkJson {
    subjects: Option<Vec<String>>,
}

/// Split `"Dune Chronicles #2"` into a name and a position.
///
/// **Two separators and no more.** OpenLibrary's `series` is free text and
/// carries everything from `Dune Chronicles #2` to `Penguin classics` to
/// `Bd. 3 : Der Zauberberg`, and a parser that keeps guessing eventually reads
/// a number that is not an index — a wrong "#2" is a claim about *which book
/// this is*, and nothing downstream can tell it from a right one. So: a
/// trailing `#N` or `; N`, and otherwise the whole string is the name and the
/// index is absent. Absence is the honest answer and the pair guard means it
/// stays absent rather than being filled from somewhere else.
fn split_series(raw: &str) -> Option<(String, Option<f64>)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    for sep in ['#', ';'] {
        if let Some((name, tail)) = raw.rsplit_once(sep) {
            let name = name.trim_end_matches([',', ' ', ':']).trim();
            if let Ok(index) = tail.trim().parse::<f64>()
                && index.is_finite()
                && !name.is_empty()
            {
                return Some((name.to_string(), Some(index)));
            }
        }
    }
    Some((raw.to_string(), None))
}

async fn author_of_key(key: &Key, client: &Client) -> Result<Option<String>> {
    let Some(k) = key.key.as_deref() else {
        return Ok(None);
    };
    let url = format!("https://openlibrary.org/{k}.json");
    let res = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let author: AuthorJson = serde_json::from_str(&res)?;
    Ok(author.name)
}

/// Subjects, from the work this edition realises.
///
/// **One extra request per book looked up**, and that is the cost of this
/// field: `by_isbn` was 1 + *authors* requests and is now 2 + *authors*.
/// OpenLibrary's edition record does not carry subjects at all — they are a
/// property of the *work* — so there is no cheaper form of the question. It is
/// issued concurrently with the author lookups, so it costs a request rather
/// than a round trip, and `search.json` deliberately does **not** ask for its
/// `subject` facet: that would enlarge every response of every search by the
/// noisiest form of this field for data only a saved book has any use for.
///
/// **A failure here is not a failure of the lookup.** The edition record has
/// already answered the question that was asked; subjects are additive. So this
/// degrades to none rather than propagating, which is the same rule the
/// federated search applies one level up — and unlike the author resolution
/// below, whose result is part of identity.
async fn subjects_of_work(key: &Key, client: &Client) -> Vec<String> {
    let Some(k) = key.key.as_deref() else {
        return Vec::new();
    };
    let url = format!("https://openlibrary.org/{k}.json");
    let fetched = async {
        let text = client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let work: WorkJson = serde_json::from_str(&text)?;
        Ok::<_, EngineError>(work.subjects.unwrap_or_default())
    }
    .await;
    match fetched {
        Ok(subjects) => normalize_subjects(subjects),
        Err(e) => {
            tracing::debug!(provider = %ProviderId::OpenLibrary, work = %k, detail = %e,
                "work subjects unavailable; the edition still answered");
            Vec::new()
        }
    }
}

async fn edition_to_book(edition: EditionJson, client: &Client) -> Result<Book> {
    // Resolve author keys to names concurrently, and ask the work about its
    // subjects in the same flight.
    let keys = edition.authors.unwrap_or_default();
    let work = edition.works.as_ref().and_then(|w| w.first());
    let (authors, subjects) = futures::future::join(
        try_join_all(keys.iter().map(|k| author_of_key(k, client))),
        async {
            match work {
                Some(w) => subjects_of_work(w, client).await,
                None => Vec::new(),
            }
        },
    )
    .await;
    let authors: Vec<String> = authors?.into_iter().flatten().collect();

    let page_count = edition
        .number_of_pages
        .or_else(|| edition.pagination.as_deref().and_then(|p| p.parse().ok()));

    fn first_valid(opt: Option<Vec<String>>) -> Option<String> {
        opt?.iter().find_map(|v| normalize_isbn(v))
    }

    let language = edition
        .languages
        .as_ref()
        .and_then(|v| v.first())
        .and_then(|k| k.key.as_deref())
        .and_then(|k| k.rsplit('/').next().map(normalize_language));

    let (series, series_index) = edition
        .series
        .as_ref()
        .and_then(|v| v.first())
        .and_then(|s| split_series(s))
        .map_or((None, None), |(name, index)| (Some(name), index));

    Ok(Book {
        title: edition.title,
        authors,
        subjects,
        series,
        series_index,
        publish_year: edition.publish_date.as_deref().and_then(year_of_date),
        openlibrary_key: edition.key,
        page_count,
        publisher: edition.publishers.and_then(|v| v.into_iter().next()),
        language,
        cover_url: edition
            .covers
            .as_ref()
            .and_then(|v| v.first())
            .map(|id| format!("https://covers.openlibrary.org/b/id/{id}-M.jpg")),
        isbn_10: first_valid(edition.isbn_10),
        isbn_13: first_valid(edition.isbn_13),
        ..Default::default()
    })
}

#[async_trait]
impl MetadataProvider for OpenLibraryProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenLibrary
    }

    async fn search(&self, req: &SearchRequest) -> Result<Vec<ProviderBook>> {
        let url = build_search_url(req)?;
        let text = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let resp: SearchResp = serde_json::from_str(&text)?;
        Ok(resp
            .docs
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(position, w)| ProviderBook {
                book: w.to_book(),
                provider: ProviderId::OpenLibrary,
                position,
            })
            .collect())
    }

    async fn by_isbn(&self, isbn: &str) -> Result<Option<ProviderBook>> {
        let url = format!("https://openlibrary.org/isbn/{isbn}.json");
        let resp = self.client.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let text = resp.error_for_status()?.text().await?;
        let edition: EditionJson =
            serde_json::from_str(&text).map_err(|e| EngineError::Provider {
                provider: ProviderId::OpenLibrary,
                message: format!("edition decode: {e}"),
            })?;
        let book = edition_to_book(edition, &self.client).await?;
        Ok(Some(ProviderBook {
            book,
            provider: ProviderId::OpenLibrary,
            position: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_url_maps_fields() {
        let req = SearchRequest {
            title: Some("The Trial".into()),
            author: Some("Kafka".into()),
            publisher: Some("Schocken".into()),
            translator: Some("Breon Mitchell".into()),
            language: Some("en".into()),
            limit: 10,
            ..Default::default()
        };
        let url = build_search_url(&req).unwrap();
        assert!(url.contains("title=The+Trial"));
        assert!(url.contains("author=Kafka"));
        assert!(url.contains("publisher=Schocken"));
        assert!(url.contains("lang=eng"));
        assert!(url.contains("contributor"));
        assert!(url.contains("limit=10"));
    }

    #[test]
    fn works_doc_deserializes_and_converts() {
        let json = r#"{
            "docs": [{
                "title": "Pachinko",
                "author_name": ["Min Jin Lee"],
                "first_publish_year": 2017,
                "cover_edition_key": "OL26389265M",
                "key": "/works/OL17553231W",
                "language": ["eng", "kor"],
                "isbn": ["9781455563937", "1455563935", "bogus"],
                "publisher": ["Grand Central"],
                "number_of_pages_median": 490
            }]
        }"#;
        let resp: SearchResp = serde_json::from_str(json).unwrap();
        let b = resp.docs.unwrap()[0].to_book();
        assert_eq!(b.title.as_deref(), Some("Pachinko"));
        assert_eq!(b.isbn_13.as_deref(), Some("9781455563937"));
        assert_eq!(b.isbn_10.as_deref(), Some("1455563935"));
        assert_eq!(b.language.as_deref(), Some("en"));
        assert_eq!(b.page_count, Some(490));
        assert!(b.cover_url.unwrap().contains("OL26389265M"));
    }

    /// The two shapes that are read, and the many that are deliberately not.
    ///
    /// The `None` cases matter more than the `Some` ones: an index parsed out of
    /// `Bd. 3 : Der Zauberberg` would be a confident claim about which book this
    /// is, and the pair guard cannot help with a wrong value — only with a
    /// missing one.
    #[test]
    fn series_text_is_split_only_where_it_is_unambiguous() {
        let cases = [
            ("Dune Chronicles #2", Some(("Dune Chronicles", Some(2.0)))),
            ("Dune Chronicles, #2", Some(("Dune Chronicles", Some(2.0)))),
            ("Discworld ; 5", Some(("Discworld", Some(5.0)))),
            // Novellas are numbered like this, which is why the column is REAL.
            ("Wayfarers #1.5", Some(("Wayfarers", Some(1.5)))),
            ("  Penguin classics  ", Some(("Penguin classics", None))),
            // A number that is not an index, and a separator that is not one.
            (
                "Bd. 3 : Der Zauberberg",
                Some(("Bd. 3 : Der Zauberberg", None)),
            ),
            (
                "The Lord of the Rings, Part 1",
                Some(("The Lord of the Rings, Part 1", None)),
            ),
            ("#3", Some(("#3", None))),
            ("   ", None),
        ];
        for (raw, want) in cases {
            let got = split_series(raw);
            let want = want.map(|(n, i)| (n.to_string(), i));
            assert_eq!(got, want, "{raw:?}");
        }
    }

    /// An edition with no work and no authors reaches no network at all, which
    /// is what makes the series half testable under the no-network rule. The
    /// subjects half needs `/works/`, and there is no test double for it: the
    /// URL is hardcoded, exactly as `author_of_key`'s is.
    #[tokio::test]
    async fn an_edition_carries_its_series_and_no_subjects_of_its_own() {
        let json = r#"{
            "title": "Dune Messiah",
            "isbn_13": ["9780441013593"],
            "series": ["Dune Chronicles #2"]
        }"#;
        let edition: EditionJson = serde_json::from_str(json).unwrap();
        let book = edition_to_book(edition, &Client::new()).await.unwrap();
        assert_eq!(book.series.as_deref(), Some("Dune Chronicles"));
        assert_eq!(book.series_index, Some(2.0));
        assert!(
            book.subjects.is_empty(),
            "subjects are a property of the work, never of the edition"
        );
    }

    #[test]
    fn edition_json_deserializes() {
        let json = r#"{
            "title": "Pachinko",
            "authors": [{"key": "/authors/OL7247542A"}],
            "isbn_10": ["1455563935"],
            "isbn_13": ["9781455563937"],
            "publish_date": "Aug 25, 2017",
            "publishers": ["Grand Central Publishing"],
            "number_of_pages": 490,
            "key": "/books/OL26389265M",
            "languages": [{"key": "/languages/eng"}],
            "covers": [8309121],
            "works": [{"key": "/works/OL17553231W"}]
        }"#;
        let e: EditionJson = serde_json::from_str(json).unwrap();
        assert_eq!(e.number_of_pages, Some(490));
        assert_eq!(e.publish_date.as_deref(), Some("Aug 25, 2017"));
        // The key the second request is made against. No work key, no request.
        assert_eq!(
            e.works.unwrap()[0].key.as_deref(),
            Some("/works/OL17553231W")
        );
    }
}
