use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{
    MetadataProvider, ProviderBook, ProviderId, SearchRequest, normalize_subjects, year_of_date,
};
use crate::book::{Book, normalize_isbn};
use crate::error::{EngineError, Result};

/// Redact `key=...` query params so an API key can never leak through error
/// messages, warnings, or logs (reqwest errors embed the full request URL).
pub fn scrub_key(msg: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"key=[^&\s]+").expect("static regex"))
        .replace_all(msg, "key=REDACTED")
        .into_owned()
}

fn scrubbed(e: reqwest::Error) -> EngineError {
    EngineError::Provider {
        provider: ProviderId::GoogleBooks,
        message: scrub_key(&e.to_string()),
    }
}

/// Live-check an API key against the volumes endpoint. `Ok(())` means Google
/// accepted it; `Err` carries a human-readable reason (invalid key, quota,
/// network down). Never includes the key itself.
pub async fn verify_key(key: &str) -> std::result::Result<(), String> {
    let client = Client::new();
    let mut url = Url::parse("https://www.googleapis.com/books/v1/volumes").expect("static url");
    url.query_pairs_mut()
        .append_pair("q", "the")
        .append_pair("maxResults", "1")
        .append_pair("key", key);
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| scrub_key(&format!("network error: {e}")))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let body = resp.text().await.unwrap_or_default();
    let reason = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| status.to_string());
    Err(scrub_key(&reason))
}

/// The live endpoint. Overridable only so tests can point at a local server —
/// see [`GoogleBooksProvider::with_base`].
const DEFAULT_BASE: &str = "https://www.googleapis.com/books/v1/volumes";

pub struct GoogleBooksProvider {
    client: Client,
    api_key: Option<String>,
    base: String,
}

impl GoogleBooksProvider {
    pub fn new(client: Client, api_key: Option<String>) -> Self {
        GoogleBooksProvider {
            client,
            api_key,
            base: DEFAULT_BASE.to_string(),
        }
    }

    /// Point the provider at a different base URL.
    ///
    /// The only reason this exists is to let tests exercise real HTTP status
    /// handling (404/429/500, truncated bodies) against a local server, which
    /// no amount of pure-function testing can reach.
    #[doc(hidden)]
    pub fn with_base(client: Client, api_key: Option<String>, base: impl Into<String>) -> Self {
        GoogleBooksProvider {
            client,
            api_key,
            base: base.into(),
        }
    }
}

#[derive(Deserialize, Debug)]
struct VolumesResp {
    items: Option<Vec<Volume>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Volume {
    id: Option<String>,
    volume_info: Option<VolumeInfo>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct VolumeInfo {
    title: Option<String>,
    subtitle: Option<String>,
    authors: Option<Vec<String>>,
    publisher: Option<String>,
    published_date: Option<String>,
    description: Option<String>,
    industry_identifiers: Option<Vec<IndustryId>>,
    page_count: Option<i64>,
    language: Option<String>,
    image_links: Option<ImageLinks>,
    /// What Google says the book is about (migration `0013`).
    ///
    /// BISAC-shaped paths — `Fiction / Literary`, `History / Asia / Korea` —
    /// and usually one or two of them, unlike OpenLibrary's long headings list.
    /// Stored as published; see `normalize_subjects` for why they are not
    /// mapped onto anything.
    categories: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct IndustryId {
    #[serde(rename = "type")]
    kind: String,
    identifier: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ImageLinks {
    thumbnail: Option<String>,
    small_thumbnail: Option<String>,
}

impl Volume {
    fn to_book(&self) -> Book {
        let info = self.volume_info.as_ref();
        let mut book = Book {
            googlebooks_id: self.id.clone(),
            ..Default::default()
        };
        let Some(info) = info else { return book };

        book.title = match (&info.title, &info.subtitle) {
            (Some(t), Some(s)) => Some(format!("{t}: {s}")),
            (Some(t), None) => Some(t.clone()),
            _ => None,
        };
        book.authors = info.authors.clone().unwrap_or_default();
        book.publisher = info.publisher.clone();
        book.publish_year = info.published_date.as_deref().and_then(year_of_date);
        book.description = info.description.clone();
        book.page_count = info.page_count;
        book.language = info.language.as_ref().map(|l| l.to_lowercase());
        book.subjects = normalize_subjects(info.categories.iter().flatten());
        // **No series.** `volumeInfo` has no documented series field: the
        // `seriesInfo` block some volumes carry is absent from the API
        // reference, so the only fixture possible is one written from memory —
        // which would prove that we agree with ourselves and nothing else, the
        // same argument `tests/fixtures/goodreads/README.md` makes for keeping
        // recorded samples recorded. OpenLibrary's edition record carries the
        // field outright, and that is where series comes from.
        book.cover_url = info.image_links.as_ref().and_then(|l| {
            l.thumbnail
                .clone()
                .or_else(|| l.small_thumbnail.clone())
                // Covers must be fetchable outside a browser session.
                .map(|u| u.replace("http://", "https://"))
        });
        for iid in info.industry_identifiers.iter().flatten() {
            if let Some(norm) = normalize_isbn(&iid.identifier) {
                match iid.kind.as_str() {
                    "ISBN_10" if book.isbn_10.is_none() => book.isbn_10 = Some(norm),
                    "ISBN_13" if book.isbn_13.is_none() => book.isbn_13 = Some(norm),
                    _ => {}
                }
            }
        }
        book
    }
}

fn build_query(req: &SearchRequest) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(q) = &req.query {
        parts.push(q.clone());
    }
    if let Some(t) = &req.title {
        parts.push(format!("intitle:\"{t}\""));
    }
    if let Some(a) = &req.author {
        parts.push(format!("inauthor:\"{a}\""));
    }
    if let Some(tr) = &req.translator {
        // Google has no translator facet; translators usually appear among
        // authors/contributors, so fold into inauthor.
        parts.push(format!("inauthor:\"{tr}\""));
    }
    if let Some(p) = &req.publisher {
        parts.push(format!("inpublisher:\"{p}\""));
    }
    if let Some(i) = &req.isbn {
        parts.push(format!("isbn:{i}"));
    }
    parts.join(" ")
}

fn build_url(base: &str, req: &SearchRequest, api_key: Option<&str>) -> Result<String> {
    let limit = if req.limit == 0 {
        30
    } else {
        req.limit.min(40)
    };
    let mut url = Url::parse(base)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("q", &build_query(req));
        qp.append_pair("maxResults", &limit.to_string());
        qp.append_pair("printType", "books");
        if let Some(l) = &req.language {
            qp.append_pair("langRestrict", l);
        }
        if let Some(k) = api_key {
            qp.append_pair("key", k);
        }
    }
    Ok(url.into())
}

#[async_trait]
impl MetadataProvider for GoogleBooksProvider {
    fn id(&self) -> ProviderId {
        ProviderId::GoogleBooks
    }

    async fn search(&self, req: &SearchRequest) -> Result<Vec<ProviderBook>> {
        let url = build_url(&self.base, req, self.api_key.as_deref())?;
        let text = self
            .client
            .get(url)
            .send()
            .await
            .map_err(scrubbed)?
            .error_for_status()
            .map_err(scrubbed)?
            .text()
            .await
            .map_err(scrubbed)?;
        let resp: VolumesResp = serde_json::from_str(&text)?;
        let mut out: Vec<ProviderBook> = resp
            .items
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(position, v)| ProviderBook {
                book: v.to_book(),
                provider: ProviderId::GoogleBooks,
                position,
            })
            .collect();
        // langRestrict can be flaky; also filter year client-side if requested.
        if let Some(y) = req.year {
            out.retain(|pb| pb.book.publish_year.is_none() || pb.book.publish_year == Some(y));
        }
        Ok(out)
    }

    async fn by_isbn(&self, isbn: &str) -> Result<Option<ProviderBook>> {
        let req = SearchRequest {
            isbn: Some(isbn.to_string()),
            limit: 1,
            ..Default::default()
        };
        Ok(self.search(&req).await?.into_iter().next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_composition() {
        let req = SearchRequest {
            title: Some("The Trial".into()),
            author: Some("Kafka".into()),
            translator: Some("Breon Mitchell".into()),
            publisher: Some("Schocken".into()),
            ..Default::default()
        };
        let q = build_query(&req);
        assert!(q.contains("intitle:\"The Trial\""));
        assert!(q.contains("inauthor:\"Kafka\""));
        assert!(q.contains("inauthor:\"Breon Mitchell\""));
        assert!(q.contains("inpublisher:\"Schocken\""));
    }

    #[test]
    fn scrub_redacts_api_keys() {
        let msg = "HTTP status client error (429) for url \
                   (https://www.googleapis.com/books/v1/volumes?q=dune&key=AIzaSySECRET123&maxResults=15)";
        let s = scrub_key(msg);
        assert!(!s.contains("AIzaSySECRET123"));
        assert!(s.contains("key=REDACTED"));
        assert!(s.contains("q=dune")); // rest of the URL survives
    }

    #[test]
    fn key_appended_only_when_set() {
        let req = SearchRequest {
            query: Some("dune".into()),
            ..Default::default()
        };
        let without = build_url(DEFAULT_BASE, &req, None).unwrap();
        assert!(!without.contains("key="));
        let with = build_url(DEFAULT_BASE, &req, Some("SECRET")).unwrap();
        assert!(with.contains("key=SECRET"));
    }

    /// Pins the exact query string for a fully-populated request. The
    /// `intitle:`/`inauthor:`/`inpublisher:` mapping is the thing that breaks
    /// silently when someone edits `build_query` — the search still returns
    /// *something*, just the wrong something.
    #[test]
    fn a_full_request_maps_to_an_exact_url() {
        let req = SearchRequest {
            query: Some("dune".into()),
            title: Some("Dune".into()),
            author: Some("Frank Herbert".into()),
            publisher: Some("Ace".into()),
            isbn: Some("9780441013593".into()),
            language: Some("en".into()),
            limit: 10,
            ..Default::default()
        };
        let url = build_url(DEFAULT_BASE, &req, None).unwrap();
        assert_eq!(
            url,
            "https://www.googleapis.com/books/v1/volumes\
             ?q=dune+intitle%3A%22Dune%22+inauthor%3A%22Frank+Herbert%22\
             +inpublisher%3A%22Ace%22+isbn%3A9780441013593\
             &maxResults=10&printType=books&langRestrict=en"
        );
    }

    /// The API caps maxResults at 40, and 0 means "unset" rather than "none".
    #[test]
    fn limits_are_clamped_to_what_the_api_accepts() {
        let mk = |limit| SearchRequest {
            query: Some("x".into()),
            limit,
            ..Default::default()
        };
        assert!(
            build_url(DEFAULT_BASE, &mk(0), None)
                .unwrap()
                .contains("maxResults=30")
        );
        assert!(
            build_url(DEFAULT_BASE, &mk(1000), None)
                .unwrap()
                .contains("maxResults=40")
        );
        assert!(
            build_url(DEFAULT_BASE, &mk(7), None)
                .unwrap()
                .contains("maxResults=7")
        );
    }

    #[test]
    fn volume_deserializes_and_converts() {
        let json = r#"{
            "items": [{
                "id": "abc123",
                "volumeInfo": {
                    "title": "Pachinko",
                    "authors": ["Min Jin Lee"],
                    "publisher": "Grand Central Publishing",
                    "publishedDate": "2017-02-07",
                    "description": "A saga.",
                    "industryIdentifiers": [
                        {"type": "ISBN_13", "identifier": "9781455563937"},
                        {"type": "ISBN_10", "identifier": "1455563935"}
                    ],
                    "pageCount": 490,
                    "language": "en",
                    "categories": ["Fiction / Literary", "  Fiction / Literary  ", "", "History"],
                    "imageLinks": {"thumbnail": "http://books.google.com/x.jpg"}
                }
            }]
        }"#;
        let resp: VolumesResp = serde_json::from_str(json).unwrap();
        let b = resp.items.unwrap()[0].to_book();
        assert_eq!(b.googlebooks_id.as_deref(), Some("abc123"));
        assert_eq!(b.isbn_13.as_deref(), Some("9781455563937"));
        assert_eq!(b.publish_year, Some(2017));
        assert_eq!(b.description.as_deref(), Some("A saga."));
        assert!(b.cover_url.unwrap().starts_with("https://"));
        // Trimmed, deduped case-insensitively, empties dropped — and otherwise
        // exactly what Google published, slashes and all.
        assert_eq!(b.subjects, vec!["Fiction / Literary", "History"]);
        assert_eq!(b.series, None);
    }

    /// A volume with no `categories` says *nothing* about subjects, which the
    /// merge reads as silence rather than as "this book has none".
    #[test]
    fn a_volume_without_categories_claims_no_subjects() {
        let json = r#"{"items": [{"id": "x", "volumeInfo": {"title": "T"}}]}"#;
        let resp: VolumesResp = serde_json::from_str(json).unwrap();
        assert!(resp.items.unwrap()[0].to_book().subjects.is_empty());
    }
}
