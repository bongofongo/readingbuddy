mod structs;
mod json_funcs;
mod ol_api_containers;

use std::{env, error::Error, process};
use crate:: {
        structs::{Config, UserInput, MissingInfoError}, 
        json_funcs::{SearchQuery},
        ol_api_containers::{SearchResp, DocEntry},
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
    let works: &Vec<DocEntry> = json.get_works()?;
    for work in works {
        work.show();
    };
    Ok(())
}


