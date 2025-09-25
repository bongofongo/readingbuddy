mod structs;
mod json_funcs;
mod ol_api_containers;
mod image_lib;
mod gen_lib;
mod epub_lib;

use epub::doc::EpubDoc;
use std::{error::Error};
use crate:: {
        structs::{MissingInfoError, Book}, 
        json_funcs::{SearchQuery},
        ol_api_containers::{SearchResp, Works},
        gen_lib::{select_element, get_user_input},
        epub_lib::read_epub,
    };

fn main() -> std::io::Result<()> {
    while let Err(e) = run() {
        eprintln!("[error]: {}", e);
    }
    // if let Err(e) = run_epub() {
    //     eprintln!("[error]: {}", e);
    // }
    Ok(())
}

fn run_epub() -> Result<(), Box<dyn Error>> {
    let fp = get_user_input("Enter epub filepath: ")?;
    let doc = EpubDoc::new(&fp)?;
    read_epub(&doc)?;
    Ok(())
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
    if let Some(_) = &b.cover_url && 
        let "y" = get_user_input("Download image? y/n: ")?.as_str() {
            b.download_image()?;
            println!("{:#?}", b)
    };

    Ok(())
}
