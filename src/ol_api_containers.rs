use url::Url;
use std::{error::Error};
use crate::{
    structs::MissingInfoError,
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
        return match self.docs {
            None => Err(Box::new(MissingInfoError)),
            Some(ref vec) => Ok(vec)
        }
    }
    pub fn get_work(&self, i: usize) -> Result<&Works, Box<dyn Error>> {
        let work: Option<&Works> = self.get_works()?
            .get(i);
        return match work {
            None => Err(Box::new(MissingInfoError)),
            Some(doc) => Ok(doc)
        }
    }
    pub fn show(&self) -> () {
        if let Some(ref s) = self.num_found {
            println!("num_found: {}", s);
        }
        if let Some(ref s) = self.q {
            println!("q: {}", s);
        }
    }
}

#[derive(serde::Deserialize, Debug)]
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
}










