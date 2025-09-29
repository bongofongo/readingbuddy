use anyhow::{Result, bail};
use std::fmt;
use time::OffsetDateTime;
use super::books::Book;

#[derive(serde::Deserialize, Debug)]
pub struct SearchResp {
    // pub q : Option<String>,
    pub docs : Option<Vec<Works>>,
}

impl SearchResp {
    pub fn get_works(&self) -> Result<&Vec<Works>> {
        match self.docs {
            None => bail!("[get_works] couldn't find any docs from openlibrary!"),
            Some(ref vec) => Ok(vec)
        }
    }
}

#[derive(serde::Deserialize)]
pub struct Works {
    pub title : Option<String>,
    pub author_name : Option<Vec<String>>,
    pub publish_year : Option<u32>,
    pub cover_edition_key : Option<String>, 
    pub key : Option<String>,
    pub language : Option<Vec<String>>,
    // pub docs : Option<Vec<SearchResp>>, // if editions work
    pub isbn : Option<Vec<String>>,
    pub edition_key : Option<Vec<String>>,
    pub first_sentence : Option<Vec<String>>
}

impl fmt::Debug for Works {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let none: &str = "None";
        let year = self.publish_year.map(|n| n.to_string());
        let author: String = match &self.author_name {
            Some(v) => v.join(", "),
            None => none.to_string()
        };
        fn first_opt (opt: &Option<Vec<String>>) -> Option<String> {
            opt.as_ref().and_then(|v| v.first().cloned())
        }
        let f_language = first_opt(&self.language);
        let f_isbn = first_opt(&self.isbn);
        let f_edition = first_opt(&self.edition_key);
        let f_first_sentence = first_opt(&self.first_sentence);

        f.debug_struct("Works")
            .field("title", &self.title.as_deref().unwrap_or(none))
            .field("Author", &author)
            .field("Year", &year.as_deref().unwrap_or(none))
            .field("Key", &self.key.as_deref().unwrap_or(none))
            .field("Language", &f_language.as_deref().unwrap_or(none))
            .field("ISBN", &f_isbn.as_deref().unwrap_or(none))
            .field("Edition Key", &f_edition.as_deref().unwrap_or(none))
            .field("First Sentence", &f_first_sentence.as_deref().unwrap_or(none))
            .field("Cover Edition Key", &self.cover_edition_key.as_deref().unwrap_or(none))
            .finish()
    }
}
impl Works {
    pub fn get_cover_image(&self) -> Result<String> {
        if let Some(k) = self.cover_edition_key.as_deref() {
            let s = format!("https://covers.openlibrary.org/b/olid/{k}-M.jpg");
            return Ok(s.to_string());
        };
        bail!("[get_cover_image] can't find an image!")
    }
    pub fn to_book(&self) -> Result<Book> {
        fn first_opt (opt: &Option<Vec<String>>) -> Option<String> {
            opt.as_ref().and_then(|v| v.first().cloned())
        }
        let first_first_sentence = first_opt(&self.first_sentence);
        let first_language = first_opt(&self.language);
        let (isbn_10, isbn_13): (Option<i64>, Option<i64>) = match first_opt(&self.isbn) {
            Some(isbn) => match isbn.len() {
                10 => (Some(isbn.parse::<i64>()?), None),
                13 => (None, Some(isbn.parse::<i64>()?)),
                _ => (None, None)
            }
            None => bail!("[to_book] missing ISBN information")
        };

        let cover_url: Option<String> = self.get_cover_image().ok();

        let book = Book {
            title: self.title.clone(),
            authors: self.author_name.clone(),
            cover_url,
            cover_path: None,
            pagination: None,
            current_page: None,
            description: None,
            first_sentence: first_first_sentence,
            language: first_language,
            isbn_10,
            isbn_13,
            openlibrary_key: self.key.clone(),
            publish_year: self.publish_year,
            finished: None,
            date_started: None,
            last_modified: OffsetDateTime::now_utc(),
            created_at: OffsetDateTime::now_utc(),
        }; 

        Ok(book)
    }
}










