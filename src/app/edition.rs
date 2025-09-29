use super::books::Book;
use anyhow::{Result, anyhow};
use time::OffsetDateTime;
use reqwest::Client;
use futures::future::try_join_all;

pub async fn ol_edition_of_isbn(isbn: &str, client: &Client) -> Result<String> {
    let url: String = format!("https://openlibrary.org/isbn/{}.json", isbn);
    let res = client.get(url).send().await?
            .error_for_status()?.text().await?;
    Ok(res)
}

async fn keys_to_authors(keys: Option<Vec<Key>>, client: &Client) -> Result<Vec<String>> {
    async fn f(k: &Key, client: &Client) -> Result<String> {
        let resp = k.ol_author_of_key(client).await?
            .ok_or(anyhow!("[keys_to_authors no author name]"))?;
        let author: AuthorJson = serde_json::from_str(&resp)?;
        let name = author.name.ok_or(anyhow!("[keys_to_authors] no author name to deserialize"))?;
        Ok(name)
    }

    let keys = keys.unwrap_or_default();
    let authors = try_join_all(keys.iter().map(|k| f(k, client))).await?;
    Ok(authors)
}

pub async fn edition_to_book(edition: EditionJson, client: &Client) -> Result<Book> {
    let authors = Some(keys_to_authors(edition.authors, client).await?);
    let pagination: Option<u32> = edition.pagination.and_then(|k| k.parse::<u32>().ok());
    let publish_year: Option<u32> = edition.publish_date.and_then(|k| k.parse::<u32>().ok());

    fn unwrap_isbn(opt: Option<Vec<String>>) -> Option<i64> {
        let vec = opt?;
        vec.first().and_then(|v| v.parse::<i64>().ok())
    }

    Ok(Book {
        title: edition.title,
        authors,
        publish_year, 
        openlibrary_key: edition.key,
        pagination,
        language: None,

        cover_url: None,
        cover_path: None,
        description: None,
        first_sentence: None,

        isbn_10: unwrap_isbn(edition.isbn_10),
        isbn_13: unwrap_isbn(edition.isbn_13),

        finished: None,
        date_started: None,
        current_page: None,

        last_modified: OffsetDateTime::now_utc(),
        created_at: OffsetDateTime::now_utc(),
    })
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct Key {
    key : Option<String>
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct Created {
    value: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct AuthorJson {
    name: Option<String>
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct EditionJson {
    #[serde(alias = "author")]
    authors: Option<Vec<Key>>,
    title: Option<String>,
    isbn_10: Option<Vec<String>>,
    isbn_13: Option<Vec<String>>,
    publish_date: Option<String>,
    publishers: Option<Vec<String>>,
    full_title: Option<String>,
    pagination: Option<String>,
    works: Option<Vec<Key>>,
    key: Option<String>,
    created: Option<Created>,
}

impl Key {
    async fn ol_author_of_key(&self, client: &Client) -> Result<Option<String>> {
        let Some(key) = self.key.as_ref() else { return Ok(None) };
        let url: String = format!("https://openlibrary.org/{}.json", &key);
        let res = client.get(&url).send().await?
                .error_for_status()?
                .text().await?;

        Ok(Some(res))
    }
}



