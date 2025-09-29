use std::fs;
use time::OffsetDateTime;
use super::gen_lib::{get_user_input,image_from_url};
use sqlx::{sqlite::{SqlitePoolOptions, SqlitePool}, types::Json, Row};
use anyhow::{Result, bail};

fn to_i64(opt: Option<u32>) -> Option<i64> {
    opt.map(|v| v as i64)
}

// Everything the user should be interacting with. 
// Struct, the information of which should be saved persistently.
#[derive(sqlx::FromRow, Debug)]
pub struct Book {
    pub title : Option<String>,
    pub authors : Option<Vec<String>>,
    pub cover_url : Option<String>,
    pub cover_path : Option<String>,
    pub pagination : Option<u32>,
    pub description : Option<String>,
    pub first_sentence : Option<String>,
    pub language : Option<String>,
    pub isbn_10 : Option<i64>,
    pub isbn_13 : Option<i64>,
    pub openlibrary_key : Option<String>,
    pub publish_year : Option<u32>,
    pub current_page : Option<u32>,
    pub finished : Option<bool>,
    pub date_started : Option<u32>,
    pub last_modified : OffsetDateTime,
    pub created_at : OffsetDateTime,
}

impl Book {
    // pub fn new() -> Self {
    //     Book { 
    //         title : None,
    //         authors : Some(Vec::new()), 
    //         cover_url : None,
    //         cover_path : None,
    //         pagination : None,
    //         description : None,
    //         first_sentence : None,
    //         language : None,
    //         isbn_10 : None,
    //         isbn_13 : None,
    //         openlibrary_key : None,
    //         publish_year : None,
    //         current_page : None,
    //         finished : None,
    //         date_started : None,
    //         last_modified : OffsetDateTime::now_utc(),
    //         created_at : OffsetDateTime::now_utc(),
    //     }
    // }
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
            First Sentence\tLanguage\tISBN_10\tISBN_13\tPage Count\tOpenLibrary Key:\n";
            let answer2: String = get_user_input(prompt2)?;

            let prompt3 = format!("Enter new {}: ", &answer2);
            let decision: String = get_user_input(&prompt3)?;

            match answer2.to_lowercase().as_str() {
                "title" => self.title = Some(decision),
                "authors" => self.authors =
                    Some(decision.split(',').map(|v| v.to_string()).collect::<Vec<String>>()),
                "coverpath" => self.cover_path = Some(decision),
                "description" => self.description = Some(decision),
                "first sentence" => self.first_sentence = Some(decision),
                "language" => self.language = Some(decision),
                "openlibrary key" => self.openlibrary_key = Some(decision),
                "coverurl" => self.cover_url = Some(decision),
                "isbn_10" => {
                    let isbn_10 = decision.parse::<i64>()?;
                    self.isbn_10 = Some(isbn_10)
                },
                "isbn_13" => {
                    let isbn_13 = decision.parse::<i64>()?;
                    self.isbn_13 = Some(isbn_13)
                },
                "year" => {
                    let year: u32 = decision.parse::<u32>()?;
                    self.publish_year = Some(year)
                },
                "page count" => {
                    let pages: u32 = decision.parse::<u32>()?;
                    self.pagination = Some(pages)
                },
                _ => bail!("[poll_user] input didn't parse")
            };
            println!("{:#?}", self);
        }
    }

    pub async fn db_upsert_book(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let author_json: Option<Json<&Vec<String>>> = self.authors.as_ref().map(Json);
        let set_clause = r#"
            title              = COALESCE(excluded.title,              books.title),
            authors            = COALESCE(excluded.authors,            books.authors),
            cover_url          = COALESCE(excluded.cover_url,          books.cover_url),
            cover_path         = COALESCE(excluded.cover_path,         books.cover_path),
            pagination         = COALESCE(excluded.pagination,         books.pagination),
            description        = COALESCE(excluded.description,        books.description),
            first_sentence     = COALESCE(excluded.first_sentence,     books.first_sentence),
            language           = COALESCE(excluded.language,           books.language),
            isbn_10            = COALESCE(excluded.isbn_10,            books.isbn_10),
            isbn_13            = COALESCE(excluded.isbn_13,            books.isbn_13),
            openlibrary_key    = COALESCE(excluded.openlibrary_key,    books.openlibrary_key),
            publish_year       = COALESCE(excluded.publish_year,       books.publish_year),
            current_page       = COALESCE(excluded.current_page,       books.current_page),
            finished           = COALESCE(excluded.finished,           books.finished),
            date_started       = COALESCE(excluded.date_started,       books.date_started),
            last_modified      = ?
            "#;

        let sql = if self.isbn_10.is_some() {
            format!(r#" INSERT INTO books (
                title, authors, cover_url, cover_path, pagination, description,
                first_sentence, language, isbn_10, isbn_13, openlibrary_key,
                publish_year, current_page, finished, date_started, last_modified, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(isbn_10) DO UPDATE SET
            {set_clause} "#)
        } else if self.isbn_13.is_some() {
            format!(r#" INSERT INTO books (
                title, authors, cover_url, cover_path, pagination, description,
                first_sentence, language, isbn_10, isbn_13, openlibrary_key,
                publish_year, current_page, finished, date_started, last_modified, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(isbn_13) DO UPDATE SET {set_clause} "#)
        } else {
            r#" INSERT INTO books (
                title, authors, cover_url, cover_path, pagination, description,
                first_sentence, language, isbn_10, isbn_13, openlibrary_key,
                publish_year, current_page, finished, date_started, last_modified, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) "#
            .to_string()
        };

        sqlx::query(&sql)
            .bind(self.title.as_ref())
            .bind(author_json.as_ref())
            .bind(self.cover_url.as_ref())
            .bind(self.cover_path.as_ref())
            .bind(to_i64(self.pagination))
            .bind(self.description.as_ref())
            .bind(self.first_sentence.as_ref())
            .bind(self.language.as_ref())
            .bind(self.isbn_10)
            .bind(self.isbn_13)
            .bind(self.openlibrary_key.as_ref())
            .bind(to_i64(self.publish_year))
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
              title, authors, cover_url, cover_path, pagination, description,
              first_sentence, language, isbn_10, isbn_13, openlibrary_key, publish_year,
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
            let authors: Option<Json<Vec<String>>> = row.try_get("authors")?;
            let pagination: Option<i64>         = row.try_get("pagination")?;
            let publish_year: Option<i64>  = row.try_get("publish_year")?;
            let current_page: Option<i64>        = row.try_get("current_page")?;
            let date_started: Option<i64>        = row.try_get("date_started")?;
            let lm_i64: i64 = row.try_get("last_modified")?;
            let ca_i64: i64 = row.try_get("created_at")?;
            let b = Book {
                title:              row.try_get("title")?,
                authors:            authors.map(|Json(v)| v),
                cover_url:          row.try_get("cover_url")?,
                cover_path:         row.try_get("cover_path")?,
                pagination:        pagination.map(|v| v as u32),
                description:        row.try_get("description")?,
                first_sentence:     row.try_get("first_sentence")?,
                language:           row.try_get("language")?,
                isbn_10:               row.try_get("isbn_10")?,            // Option<i64>
                isbn_13:               row.try_get("isbn_13")?,            // Option<i64>
                openlibrary_key:    row.try_get("openlibrary_key")?,
                publish_year: publish_year.map(|v| v as u32),
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
        if let Some(isbn_10) = self.isbn_10.as_ref() {
            sqlx::query("DELETE from books WHERE isbn_10=?") 
                .bind(isbn_10)
                .execute(pool)
                .await?;
            if let Some(path) = self.cover_path.as_ref() {
                fs::remove_file(path)?;
            }
            return Ok(())
        } else if let Some(isbn_13) = self.isbn_13.as_ref() {
            sqlx::query("DELETE from books WHERE isbn_13=?") 
                .bind(isbn_13)
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


