use std::{error::Error, fmt};
use crate::{
    image_lib::image_from_url,
    gen_lib::get_user_input,
    };
use serde::{Serialize, Deserialize};

// Everything the user should be interacting with. 
// Struct, the information of which should be saved persistently.
#[derive(Deserialize, Serialize)]
pub struct Book {
    pub title : Option<String>,
    pub author : Option<Vec<String>>,
    pub cover_url : Option<String>,
    pub cover_path : Option<String>,
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
        match &self.cover_url {
            Some(url) => 
            {
                let fname = image_from_url(url)?;
                self.cover_path = Some(fname);
            },
            None => match &self.cover_path {
                Some(fp) => println!("{}", &fp),
                _ => println!("[error]: no image found"),
            },
        };
        Ok(())
    }

    pub fn poll_user(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
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
                "isbn" => self.isbn = Some(decision),
                "openlibrary key" => self.openlibrary_key = Some(decision),
                "coverurl" => self.cover_url = Some(decision),
                "year" => {
                    let year: u32 = decision.parse::<u32>()?;
                    self.first_publish_year = Some(year)
                },
                "page count" => {
                    let pages: u32 = decision.parse::<u32>()?;
                    self.total_pages = Some(pages)
                },
                _ => return Err(Box::new(InvalidInputError))
            };
            println!("{:#?}", self);
        }
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

        f.debug_struct("Book")
            .field("Title", &self.title.as_deref().unwrap_or(none))
            .field("Author", &author) 
            .field("Page Count", &pages.as_deref().unwrap_or(none))
            .field("Cover URL", &self.cover_url.as_deref().unwrap_or(none))
            .field("Cover Path", &self.cover_path.as_deref().unwrap_or(none))
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


#[derive(Debug)]
pub struct InvalidInputError; 

impl fmt::Display for InvalidInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "didn't understand your input")
    }
}

impl Error for InvalidInputError {}






