use std::{error::Error};
use crate::structs::MissingInfoError;

#[derive(serde::Deserialize, Debug)]
pub struct SearchResp {
    // #[serde(alias = "numFound")]
    pub num_found : Option<u32>,
    start : Option<u32>,
    #[serde(alias = "NumFoundExact")]
    num_found_exact : Option<bool>,
    q : Option<String>,
    pub docs : Option<Vec<DocEntry>>,
}

impl SearchResp {
    pub fn get_works(&self) -> Result<&Vec<DocEntry>, Box<dyn Error>> {
        return match self.docs {
            None => Err(Box::new(MissingInfoError)),
            Some(ref vec) => Ok(vec)
        }
    }
    pub fn get_work(&self, i: usize) -> Result<&DocEntry, Box<dyn Error>> {
        let work: Option<&DocEntry> = self.get_works()?
            .get(i);
        return match work {
            None => Err(Box::new(MissingInfoError)),
            Some(doc) => Ok(doc)
        }
    }
    pub fn print(&self) -> () {
        if let Some(ref s) = self.num_found {
            println!("num_found: {}", s);
        }
        if let Some(ref s) = self.start {
            println!("start: {}", s);
        }
        if let Some(ref s) = self.num_found_exact {
            println!("num_found_exact: {}", s);
        }
        if let Some(ref s) = self.q {
            println!("q: {}", s);
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct DocEntry {
    pub author_name : Option<Vec<String>>,
    edition_count : Option<u32>,
    pub author_key : Option<Vec<String>>,
    cover_i : Option<u64>,
    pub cover_edition_key : Option<String>, 
    ebook_access : Option<String>,
    pub first_publish_year : Option<u32>,
    pub has_fulltext: Option<bool>,
    pub ia : Option<Vec<String>>,
    pub ia_collection_s : Option<String>,
    pub key : Option<String>,
    pub language : Option<Vec<String>>,
    pub lending_edition_s : Option<String>,
    lending_identifier_s : Option<String>,
    public_scan_b : Option<bool>,
    pub title: Option<String>
}

impl DocEntry {
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
        if let Some(lending_edition) = self.lending_edition_s.as_deref() {
            println!("\tlending_edition: {}", lending_edition);
        };
    }
}
