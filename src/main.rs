mod structs;
mod json_funcs;
mod ol_api_containers;
mod image_lib;
mod gen_lib;

use std::{error::Error};
use crate:: {
        structs::{MissingInfoError, Book, BookCover}, 
        json_funcs::{SearchQuery},
        ol_api_containers::{SearchResp, Works},
        gen_lib::{select_element, get_user_input},
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
    while let Err(e) = b.poll_user() {
        println!("[error]: {}", e);
    };
    if let Some(BookCover::Urlpath(_url)) = &b.cover && 
        let "y" = get_user_input("Download image? y/n: ")?.as_str() {
            b.download_image()?
    };
    println!("{:#?}", b);

    Ok(())
}
