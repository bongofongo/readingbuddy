use std::{error::Error, fmt, io};

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

#[derive(Debug)]
pub struct MissingInfoError; 

impl fmt::Display for MissingInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "missing critical book information from openlibrary")
    }
}

impl Error for MissingInfoError {}








