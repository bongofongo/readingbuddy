use url::Url;
use std::{error::Error, fmt};
use crate::{
    structs::{MissingInfoError, Book, BookCover},
    image_lib::image_from_url,
};

#[derive(serde::Deserialize, Debug)]
pub struct SearchResp {
    pub num_found : Option<u32>,
    // start : Option<u32>,
    // #[serde(alias = "NumFoundExact")]
    // num_found_exact : Option<bool>,
    pub q : Option<String>,
    pub docs : Option<Vec<Works>>,


}

impl SearchResp {
    pub fn get_works(&self) -> Result<&Vec<Works>, Box<dyn Error>> {
        match self.docs {
            None => Err(Box::new(MissingInfoError)),
            Some(ref vec) => Ok(vec)
        }
    }
    pub fn get_work(&self, i: usize) -> Result<&Works, Box<dyn Error>> {
        let work: Option<&Works> = self.get_works()?
            .get(i);
        match work {
            None => Err(Box::new(MissingInfoError)),
            Some(doc) => Ok(doc)
        }
    }
    pub fn show(&self) -> (){
        if let Some(ref s) = self.num_found {
            println!("num_found: {}", s);
        }
        if let Some(ref s) = self.q {
            println!("q: {}", s);
        }
    }
}

#[derive(serde::Deserialize)]
pub struct Works {
    pub title : Option<String>,
    pub author_name : Option<Vec<String>>,
    pub first_publish_year : Option<u32>,
    pub cover_edition_key : Option<String>, 
    pub key : Option<String>,
    pub language : Option<Vec<String>>,
    pub docs : Option<Vec<SearchResp>>, // if editions work
    pub isbn : Option<Vec<String>>,
    pub edition_key : Option<Vec<String>>,
    pub first_sentence : Option<Vec<String>>
}

impl fmt::Debug for Works {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let none: &str = "None";
        let year = self.first_publish_year.map(|n| n.to_string());
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
    pub fn show(&self) -> () {
        if let Some(title) = self.title.as_deref() {
            println!("{}:", title)
        }
        if let Some(authors) = self.author_name.as_ref() {
            print!("\tAuthor(s): ");
            for author in authors {
                print!("{}, ", author);
            };
            println!("");
        };
        if let Some(key) = self.key.as_deref() {
            println!("\tKey: {}", key);
        };
        if let Some(year) = self.first_publish_year {
            println!("\tYear: {}", year);
        };
        if let Ok(url) = self.get_cover_image() {
            println!("\tCover: {}", url.as_str());
            if let Err(e) = image_from_url(url) {
                println!("\t[Works::show()][error]: {}", e);
            }
        }
    }

    pub fn get_cover_image(&self) -> Result<Url, Box<dyn Error>> {
        if let Some(k) = self.cover_edition_key.as_deref() {
            let s = format!("https://covers.openlibrary.org/b/olid/{k}-M.jpg");
            let url: Url = Url::parse(&s)?;
            return Ok(url);
        };
        Err(Box::new(MissingInfoError))
    }
    pub fn to_book(&self) -> Result<Book, Box<dyn Error>> {
        let cover_url: Option<BookCover>= match self.get_cover_image() {
            Ok(url) => Some(BookCover::UrlPath(url)),
            Err(_) => None
        };
        fn first_opt (opt: &Option<Vec<String>>) -> Option<String> {
            opt.as_ref().and_then(|v| v.first().cloned())
        }
        let first_first_sentence = first_opt(&self.first_sentence);
        let first_language = first_opt(&self.language);
        let first_isbn = first_opt(&self.isbn);

        let book = Book {
            title: self.title.clone(),
            author: self.author_name.clone(),
            cover: cover_url,
            total_pages: None,
            current_page: None,
            description: None,
            first_sentence: first_first_sentence,
            language: first_language,
            isbn: first_isbn,
            openlibrary_key: self.key.clone(),
            first_publish_year: self.first_publish_year,
            finished: None,
            date_started: None
        }; 

        Ok(book)

    }
}










