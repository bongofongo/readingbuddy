use std::{error::Error, fmt, io};
use crate::{ol_api_containers::Works};

pub struct Config {
    pub user_input : UserInput
}

impl Config {
    pub fn build(mut args: impl Iterator<Item = String>) -> Config {
        args.next();
        if let Some(title) = args.next() {
            if let Some(author) = args.next() {
                let user_input = UserInput { 
                    title: Some(title), 
                    author: Some(author)
                };
                return Config { user_input }
            }
            let user_input = UserInput { 
                title: Some(title), 
                author: None
            };
            return Config { user_input }
        }
        let user_input = UserInput { 
            title: None, 
            author: None
        };
        Config { user_input }
    }
}

pub struct UserInput {
    pub title: Option<String>,
    pub author: Option<String>
}

impl UserInput {
    fn str_to_opt(s: String) -> Option<String> {
        let empty = String::from("");
        if s == empty {
            return None
        }
        Some(s)
    }
    pub fn poll_user() -> UserInput {
        let mut t = String::new();
        let mut a = String::new();

        print!("Enter Book Title: ");
        let _ = io::stdin().read_line(&mut t);

        print!("Enter Author: ");
        let _ = io::stdin().read_line(&mut a);

        let title = UserInput::str_to_opt(t);
        let author = UserInput::str_to_opt(a);
       
        UserInput { title, author }
       
    }
    pub fn new(title: Option<String>, author: Option<String>) -> UserInput {
        UserInput { title, author } 
    }
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.author.is_none()
    }
    
}

// Gives an option to keep the cover local 
// or to just link it and download it later.
pub enum BookCover {
    UrlPath(url::Url),
    Filepath(String)
}

// Everything the user should be interacting with. 
// Struct, the information of which should be saved persistently.
// TODO: 
// 1. Change author to an option vector datatype
pub struct Book {
    pub title : Option<String>,
    pub author : Option<Vec<String>>,
    pub cover : Option<BookCover>,
    pub total_pages : Option<u32>,
    pub current_page : Option<u32>,
    pub description : Option<String>,
    pub first_sentence : Option<String>,
    pub language : Option<String>,
    pub isbn : Option<String>,
    pub openlibrary_key : Option<String>,
    pub first_publish_year : Option<u32>,
    pub finished : Option<bool>,
    pub date_started : Option<u32>,
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
            .field("Pages", &pages.as_deref().unwrap_or(none))
            .field("First Sentence", &self.first_sentence.as_deref().unwrap_or(none))
            .field("Year", &year.as_deref().unwrap_or(none))
            .field("ISBN", &self.isbn.as_deref().unwrap_or(none))
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








