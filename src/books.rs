use std::{fmt, fs};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use crate::gen_lib::{get_user_input,image_from_url};
use sqlx::{sqlite::{SqlitePoolOptions, SqlitePool}, types::Json, Row};
use anyhow::{Result, bail};

fn to_i64(opt: Option<u32>) -> Option<i64> {
    opt.map(|v| v as i64)
}

// Everything the user should be interacting with. 
// Struct, the information of which should be saved persistently.
#[derive(sqlx::FromRow)]
pub struct Book {
    pub title : Option<String>,
    pub author : Option<Vec<String>>,
    pub cover_url : Option<String>,
    pub cover_path : Option<String>,
    pub total_pages : Option<u32>,
    pub description : Option<String>,
    pub first_sentence : Option<String>,
    pub language : Option<String>,
    pub isbn : Option<i64>,
    pub openlibrary_key : Option<String>,
    pub first_publish_year : Option<u32>,
    pub current_page : Option<u32>,
    pub finished : Option<bool>,
    pub date_started : Option<u32>,
    pub last_modified : OffsetDateTime,
    pub created_at : OffsetDateTime,
}

impl Book {
    pub fn new() -> Self {
        Book { 
            title : None,
            author : None, 
            cover_url : None,
            cover_path : None,
            total_pages : None,
            description : None,
            first_sentence : None,
            language : None,
            isbn : None,
            openlibrary_key : None,
            first_publish_year : None,
            current_page : None,
            finished : None,
            date_started : None,
            last_modified : OffsetDateTime::now_utc(),
            created_at : OffsetDateTime::now_utc(),
        }
    }
    pub async fn download_image(&mut self, path: &str) -> Result<()> {
        match &self.cover_url {
            Some(url) => {
                self.cover_path = image_from_url(url, path).await.ok();
            },
            None => match &self.cover_path {
                Some(fp) => println!("{}", &fp),
                _ => bail!("[download_image] path not found")
            },
        };
        Ok(())
    }

    pub fn poll_user(&mut self) -> Result<()> {
        loop {
            self.last_modified = OffsetDateTime::now_utc();
            let prompt1: &str = "Is there anything you'd like to change? y/n: ";
            let answer1: String = get_user_input(prompt1)?;
            if answer1.as_str() == "n" { return Ok(()) };

            let prompt2: &str = "Choose from the following options:\n\
            Title\tAuthor\tCoverURL\tCoverPath\tYear\tDescription\n\
            First Sentence\tLanguage\tISBN\tPage Count\tOpenLibrary Key:\n";
            let answer2: String = get_user_input(prompt2)?;

            let prompt3 = format!("Enter new {}: ", &answer2);
            let decision: String = get_user_input(&prompt3)?;

            match answer2.to_lowercase().as_str() {
                "title" => self.title = Some(decision),
                "author" => self.author = Some(vec![decision]),
                "coverpath" => self.cover_path = Some(decision),
                "description" => self.description = Some(decision),
                "first sentence" => self.first_sentence = Some(decision),
                "language" => self.language = Some(decision),
                "openlibrary key" => self.openlibrary_key = Some(decision),
                "coverurl" => self.cover_url = Some(decision),
                "isbn" => {
                    let isbn = decision.parse::<i64>()?;
                    self.isbn = Some(isbn)
                },
                "year" => {
                    let year: u32 = decision.parse::<u32>()?;
                    self.first_publish_year = Some(year)
                },
                "page count" => {
                    let pages: u32 = decision.parse::<u32>()?;
                    self.total_pages = Some(pages)
                },
                _ => return Err(InvalidInputError.into())
            };
            println!("{:#?}", self);
        }
    }

    pub async fn db_upsert_book(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let author_json: Option<Json<&Vec<String>>> = self.author.as_ref().map(Json);
        let set_clause = r#"
            title              = COALESCE(excluded.title,              books.title),
            author             = COALESCE(excluded.author,             books.author),
            cover_url          = COALESCE(excluded.cover_url,          books.cover_url),
            cover_path         = COALESCE(excluded.cover_path,         books.cover_path),
            total_pages        = COALESCE(excluded.total_pages,        books.total_pages),
            description        = COALESCE(excluded.description,        books.description),
            first_sentence     = COALESCE(excluded.first_sentence,     books.first_sentence),
            language           = COALESCE(excluded.language,           books.language),
            isbn               = COALESCE(excluded.isbn,               books.isbn),
            openlibrary_key    = COALESCE(excluded.openlibrary_key,    books.openlibrary_key),
            first_publish_year = COALESCE(excluded.first_publish_year, books.first_publish_year),
            current_page       = COALESCE(excluded.current_page,       books.current_page),
            finished           = COALESCE(excluded.finished,           books.finished),
            date_started       = COALESCE(excluded.date_started,       books.date_started),
            last_modified      = ?
            "#;

        let sql = if self.isbn.is_some() {
            format!(r#" INSERT INTO books (
                title, author, cover_url, cover_path, total_pages, description,
                first_sentence, language, isbn, openlibrary_key,
                first_publish_year, current_page, finished, date_started, last_modified, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(isbn) DO UPDATE SET
            {set_clause} "#)
        } else {
            r#" INSERT INTO books (
                title, author, cover_url, cover_path, total_pages, description,
                first_sentence, language, isbn, openlibrary_key,
                first_publish_year, current_page, finished, date_started, last_modified, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) "#
            .to_string()
        };

        sqlx::query(&sql)
            .bind(self.title.as_ref())
            .bind(author_json.as_ref())
            .bind(self.cover_url.as_ref())
            .bind(self.cover_path.as_ref())
            .bind(to_i64(self.total_pages))
            .bind(self.description.as_ref())
            .bind(self.first_sentence.as_ref())
            .bind(self.language.as_ref())
            .bind(self.isbn)
            .bind(self.openlibrary_key.as_ref())
            .bind(to_i64(self.first_publish_year))
            .bind(to_i64(self.current_page))
            .bind(self.finished)
            .bind(to_i64(self.date_started))
            .bind(self.last_modified.unix_timestamp())
            .bind(self.created_at.unix_timestamp())
            .execute(pool).await?;

        Ok(())
    }
    pub async fn db_add(&self, url: &str) -> Result<(), sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await?;
        self.db_upsert_book(&pool).await?;
        Ok(())
    }

    pub async fn db_read_to_books(limit: i32, pool: &SqlitePool) -> Result<Vec<Book>> {
        let rows = sqlx::query(
            r#" SELECT
              title, author, cover_url, cover_path, total_pages, description,
              first_sentence, language, isbn, openlibrary_key, first_publish_year,
              current_page, finished, date_started, last_modified, created_at
            FROM books
            ORDER BY last_modified DESC
            LIMIT ?; "#
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let mut books: Vec<Book> = Vec::new();
        for row in rows {
            let author: Option<Json<Vec<String>>> = row.try_get("author")?;
            let total_pages: Option<i64>         = row.try_get("total_pages")?;
            let first_publish_year: Option<i64>  = row.try_get("first_publish_year")?;
            let current_page: Option<i64>        = row.try_get("current_page")?;
            let date_started: Option<i64>        = row.try_get("date_started")?;
            let lm_i64: i64 = row.try_get("last_modified")?;
            let ca_i64: i64 = row.try_get("created_at")?;
            let b = Book {
                title:              row.try_get("title")?,
                author:             author.map(|Json(v)| v),
                cover_url:          row.try_get("cover_url")?,
                cover_path:         row.try_get("cover_path")?,
                total_pages:        total_pages.map(|v| v as u32),
                description:        row.try_get("description")?,
                first_sentence:     row.try_get("first_sentence")?,
                language:           row.try_get("language")?,
                isbn:               row.try_get("isbn")?,            // Option<i64>
                openlibrary_key:    row.try_get("openlibrary_key")?,
                first_publish_year: first_publish_year.map(|v| v as u32),
                current_page:       current_page.map(|v| v as u32),
                finished:           row.try_get("finished")?,        // Option<bool> (0/1)
                date_started:       date_started.map(|v| v as u32),
                last_modified: OffsetDateTime::from_unix_timestamp(lm_i64)?,
                created_at: OffsetDateTime::from_unix_timestamp(ca_i64)?,
            };
            books.push(b);
        };
        Ok(books)
    }

    pub async fn db_remove(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        if let Some(isbn) = self.isbn.as_ref() {
            sqlx::query("DELETE from books WHERE isbn=?") 
                .bind(isbn)
                .execute(pool)
                .await?;
            if let Some(path) = self.cover_path.as_ref() {
                fs::remove_file(path)?;
            }
            return Ok(())
        }
        println!("No such book found in database");
        Ok(())
    }
}

impl fmt::Debug for Book {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let none: &str = "None";

        let pages = self.total_pages.map(|n| n.to_string());
        let year = self.first_publish_year.map(|n| n.to_string());
        let isbn = self.isbn.map(|n| n.to_string());

        let last_modified = self.last_modified.format(&Rfc3339).unwrap_or_default();
        let created_at = self.created_at.format(&Rfc3339).unwrap_or_default();

        let author: String = match &self.author {
            Some(v) => v.join(", "),
            None => none.to_string()
        };

        f.debug_struct("Book")
            .field("Title", &self.title.as_deref().unwrap_or(none))
            .field("Author", &author) 
            .field("Page Count", &pages.as_deref().unwrap_or(none))
            .field("Cover URL", &self.cover_url.as_deref().unwrap_or(none))
            .field("Cover Path", &self.cover_path.as_deref().unwrap_or(none))
            .field("First Sentence", &self.first_sentence.as_deref().unwrap_or(none))
            .field("Description", &self.description.as_deref().unwrap_or(none))
            .field("Year", &year.as_deref().unwrap_or(none))
            .field("ISBN", &isbn.as_deref().unwrap_or(none))
            .field("Language", &self.language.as_deref().unwrap_or(none))
            .field("OpenLibrary Key", &self.openlibrary_key.as_deref().unwrap_or(none))
            .field("Last Modified", &last_modified)
            .field("Created at", &created_at)
            .finish()
    }
}

#[derive(Debug)]
pub struct MissingInfoError; 

impl fmt::Display for MissingInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "missing critical book information from openlibrary")
    }
}

impl std::error::Error for MissingInfoError {}


#[derive(Debug)]
pub struct InvalidInputError; 

impl fmt::Display for InvalidInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "didn't understand your input")
    }
}

impl std::error::Error for InvalidInputError {}






