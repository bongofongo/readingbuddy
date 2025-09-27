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
use sqlx::sqlite::SqlitePoolOptions;
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
    create_db(DB_URL).await?;    

    while let Err(e) = run().await {
        eprintln!("[error]: {}", e);
        sleep(Duration::from_secs(1)).await;
    };
    Ok(())
}


async fn run () -> Result<(), Box<dyn Error>> {
    let q: &str = "\nBookBuddy:\
    \n\tSearch for books [s]\
    \n\tRead a .epub [r]\
    \n\tView database [d]\
    \n\tExit [e]\nenter: ";

    loop {
        let input = get_user_input(q)?;

        match input.as_ref() {
            "s" => { let b = user_search_books().await?; println!("{b:#?}") },
            "r" => user_print_epub()?,
            "d" => user_print_db(10, DB_URL).await?,
            "e" => break,
            _   => println!("didn't register input.")
        };
    }
    Ok(())
}

async fn user_search_books() -> Result<Book, Box<dyn Error>> {
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
    };

    b.db_add(DB_URL).await?;
    Ok(b)
}

fn user_print_epub() -> Result<(), Box<dyn Error>> {
    let fp = get_user_input("Enter epub filepath: ")?;
    let doc = EpubDoc::new(&fp)?;
    read_epub(&doc)?;
    Ok(())
}

async fn user_print_db(limit: i32, url: &str) -> Result<(), Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;
    let books = Book::db_read_to_books(limit, &pool).await?;
    for b in books {
        println!("{b:#?}");
    }
    Ok(())
}
