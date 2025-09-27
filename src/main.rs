mod books;
mod json_funcs;
mod ol_api_containers;
mod image_lib;
mod gen_lib;
mod epub_lib;

use epub::doc::EpubDoc;
use std::{error::Error};
use std::time::Duration;
use tokio::time::sleep;
use crate::gen_lib::create_db;
use crate:: {
        books::{MissingInfoError, Book}, 
        json_funcs::{SearchQuery},
        ol_api_containers::{SearchResp, Works},
        gen_lib::{select_element, get_user_input},
        epub_lib::read_epub,
    };

const DB_URL: &str = "sqlite://database/app.db";
const IMAGE_PATH: &str = "database/images/";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    while let Err(e) = run().await {
        eprintln!("[error]: {}", e);
        sleep(Duration::from_secs(1)).await;
    }
    Ok(())
    // if let Err(e) = run_epub() {
    //     eprintln!("[error]: {}", e);
    // }
}

fn run_epub() -> Result<(), Box<dyn Error>> {
    let fp = get_user_input("Enter epub filepath: ")?;
    let doc = EpubDoc::new(&fp)?;
    read_epub(&doc)?;
    Ok(())
}

async fn run () -> Result<(), Box<dyn Error>> {
    let search: SearchQuery = SearchQuery::poll_user();
    let json: SearchResp = search.get_ol_json().await?;
    let works: &Vec<Works> = json.get_works()?;

    for (i, work) in works.iter().enumerate() {
        print!("{}: ", i);
        println!("{:#?}", work);
    };

    let index: usize = select_element("Please enter a number: ", works.len());
    let mut b: Book = works.get(index)
        .map(|w| w.to_book()).transpose()?
        .ok_or(MissingInfoError)?;

    b.language = Some("eng".to_string());
    println!("{:#?}", b);
    while let Err(e) = b.poll_user() {
        println!("[error]: {}", e);
    };
    if let Some(_) = &b.cover_url && 
        let "y" = get_user_input("Download image? y/n: ")?.as_str() {
            b.download_image(IMAGE_PATH).await?;
            println!("{:#?}", b)
    };

    create_db(DB_URL).await?;
    b.db_add(DB_URL).await?;
    Ok(())
}
