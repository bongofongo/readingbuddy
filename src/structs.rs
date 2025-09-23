use std::{error::Error, fmt, io};
use crate::{image_lib::image_from_url};

// Gives an option to keep the cover local 
// or to just link it and download it later.
pub enum BookCover {
    Urlpath(url::Url),
    Filepath(String)
}

// Everything the user should be interacting with. 
// Struct, the information of which should be saved persistently.
pub struct Book {
    pub title : Option<String>,
    pub author : Option<Vec<String>>,
    pub cover : Option<BookCover>,
    pub total_pages : Option<u32>,
    pub description : Option<String>,
    pub first_sentence : Option<String>,
    pub language : Option<String>,
    pub isbn : Option<String>,
    pub openlibrary_key : Option<String>,
    pub first_publish_year : Option<u32>,
    pub current_page : Option<u32>,
    pub finished : Option<bool>,
    pub date_started : Option<u32>,
}

impl Book {
    pub fn download_image(&mut self) -> Result<(), Box<dyn Error>> {
        match &self.cover {
            Some(BookCover::Urlpath(url)) => {
                let fname: String = image_from_url(url)?;
                self.cover = Some(BookCover::Filepath(fname));
            },
            Some(BookCover::Filepath(fp)) => println!("{}", &fp),
            None => println!("[error]: no image found")
        };
        Ok(())
    }

}

impl fmt::Debug for Book {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let none: &str = "None";

        let pages = self.total_pages.map(|n| n.to_string());
        let year = self.first_publish_year.map(|n| n.to_string());

        let author: String = match &self.author {
            Some(v) => v.join(", "),
            None => none.to_string()
        };

        let cover: &str = match &self.cover {
            Some(BookCover::Urlpath(url)) => url.as_str(),
            Some(BookCover::Filepath(fp)) => fp,
            None => "empty"
        };

        f.debug_struct("Book")
            .field("Title", &self.title.as_deref().unwrap_or(none))
            .field("Author", &author) 
            .field("Page Count", &pages.as_deref().unwrap_or(none))
            .field("Cover", &cover)
            .field("First Sentence", &self.first_sentence.as_deref().unwrap_or(none))
            .field("Description", &self.description.as_deref().unwrap_or(none))
            .field("Year", &year.as_deref().unwrap_or(none))
            .field("ISBN", &self.isbn.as_deref().unwrap_or(none))
            .field("Language", &self.language.as_deref().unwrap_or(none))
            .field("OpenLibrary Key", &self.openlibrary_key.as_deref().unwrap_or(none))
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

impl Error for MissingInfoError {}








