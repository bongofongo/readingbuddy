mod structs;
mod json_funcs;
mod ol_api_containers;
mod image_lib;

use std::{error::Error, io::{self, Write}};
use crate:: {
        structs::{MissingInfoError, Book}, 
        json_funcs::{SearchQuery},
        ol_api_containers::{SearchResp, Works},
    };

fn main() {
    while let Err(e) = run() {
        eprintln!("[error]: {}", e);
    }
}

fn run () -> Result<(), Box<dyn Error>> {
    let search: SearchQuery = SearchQuery::poll_user();
    let json: SearchResp = search.get_ol_json()?;
    let works: &Vec<Works> = json.get_works()?;

    for (i, work) in works.iter().enumerate() {
        print!("{}: ", i);
        println!("{:#?}", work);
    };

    let index: usize = select_element("Please enter a number: ", works.len());
    let mut b: Book = works.get(index)
        .map(|w| w.to_book()).transpose()?
        .ok_or(MissingInfoError)?;

    println!("{:#?}", b);
    b.download_image()?;

    Ok(())
}

fn get_user_input(s: &str) -> Result<String, Box<dyn Error>> {
    print!("{}", s);
    let mut input = String::new();
    io::stdout().flush()?;
    io::stdin().read_line(&mut input)?;
    let user_selection: String = input.trim().to_string();
    Ok(user_selection)
}

fn select_element(s: &str, len: usize) -> usize {
    loop {
        match get_user_input(s) {
            Ok(res) => 
                match res.parse::<usize>() {
                    Ok(i) => 
                        if i >= len {
                            println!("[select_element][error]: out of bounds.") 
                        } else {
                            break i
                        },
                    Err(e) => println!("[select_element][error]: {}", e),
                },
            Err(e) => println!("[select_element][error]: {}", e),
        };
        println!("Try again.")
    }
}

