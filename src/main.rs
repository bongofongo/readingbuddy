mod structs;
mod json_funcs;
mod ol_api_containers;
mod image_lib;

use std::{env, error::Error, process};
use crate:: {
        structs::{Config, UserInput, MissingInfoError, Book}, 
        json_funcs::{SearchQuery},
        ol_api_containers::{SearchResp, Works},
        image_lib::image_from_url,
    };

fn main() {
    // let config: Config = Config::build(env::args());

    if let Err(e) = run() {
        println!("[error]: {}", e);
        process::exit(1)
    }
}

fn run () -> Result<(), Box<dyn Error>> {
    let search: SearchQuery = SearchQuery::poll_user();
    let json: SearchResp = search.get_ol_json()?;
    let works: &Vec<Works> = json.get_works()?;
    for work in works {
        let b: Book = work.to_book()?;
        println!("{:#?}", b)
    };
    Ok(())
}


