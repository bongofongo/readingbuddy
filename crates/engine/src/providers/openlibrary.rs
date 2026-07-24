use async_trait::async_trait;
use futures::future::try_join_all;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{
    MetadataProvider, ProviderBook, ProviderId, SearchRequest, normalize_language,
    to_marc_language, year_of_date,
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
    fn cover_url(&self) -> Option<String> {
        self.cover_edition_key
            .as_deref()
            .map(|k| format!("https://covers.openlibrary.org/b/olid/{k}-M.jpg"))
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

async fn edition_to_book(edition: EditionJson, client: &Client) -> Result<Book> {
    // Resolve author keys to names concurrently.
    let keys = edition.authors.unwrap_or_default();
    let authors: Vec<String> = try_join_all(keys.iter().map(|k| author_of_key(k, client)))
        .await?
        .into_iter()
        .flatten()
        .collect();

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

    Ok(Book {
        title: edition.title,
        authors,
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
        let edition: EditionJson = serde_json::from_str(&text)
            .map_err(|e| EngineError::Other(format!("openlibrary edition decode: {e}")))?;
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
            "covers": [8309121]
        }"#;
        let e: EditionJson = serde_json::from_str(json).unwrap();
        assert_eq!(e.number_of_pages, Some(490));
        assert_eq!(e.publish_date.as_deref(), Some("Aug 25, 2017"));
    }
}
