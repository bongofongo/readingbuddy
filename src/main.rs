mod structs;
mod json_funcs;
mod ol_api_containers;

use std::{env, error::Error, process};
use crate:: {
        structs::{Config, UserInput, MissingInfoError}, 
        json_funcs::json_from_title,
        ol_api_containers::{SearchResp, DocEntry},
    };

fn main() {
    let config: Config = Config::build(env::args());

    if let Err(e) = run(&config) {
        println!("[error]: {}", e);
        process::exit(1)
    }
}

fn run (cfg : &Config) -> Result<(), Box<dyn Error>> {
    let input: UserInput = 
        if cfg.user_input.is_empty() {
            UserInput::new(
                cfg.user_input.title.clone(), 
                cfg.user_input.author.clone()
                )
        } else {
            UserInput::poll_user()
        };
    let query: String = input.title.ok_or(Box::new(MissingInfoError))?;
    let search: SearchResp = json_from_title(&query)?;
    let works: &Vec<DocEntry> = search.get_works()?;
    for work in works {
        work.print();
    };
    Ok(())
}


