mod books;
mod json_funcs;
mod ol_api_containers;
mod gen_lib;
mod epub_lib;

use crate::{
    books::{MissingInfoError, Book}, 
    json_funcs::{SearchQuery},
    ol_api_containers::{SearchResp, Works},
    gen_lib::{select_element, get_user_input},
    epub_lib::{download_epub_cover, read_epub_to_book},
    gen_lib::db_make
};

use anyhow::{Result};
use epub::doc::EpubDoc;
use std::time::Duration;
use tokio::time::sleep;
use sqlx::sqlite::SqlitePoolOptions;

const DB_URL: &str = "sqlite://database/app.db";
const IMAGE_PATH: &str = "database/images/";

#[tokio::main]
async fn main() -> Result<()> {
    db_make(DB_URL).await?;    

    while let Err(e) = run().await {
        eprintln!("[error]: {}", e);
        sleep(Duration::from_secs(1)).await;
    };
    Ok(())
}


async fn run () -> Result<()> {
    let q: &str = "\nBookBuddy:\
    \n\tSearch for books [s]\
    \n\tRead a .epub [r]\
    \n\tView database [d]\
    \n\tRemove database entry [rd]\
    \n\tExit [e]\nenter: ";

    loop {
        let input = get_user_input(q)?;

        match input.as_ref() {
            "s" => { let b = user_search_books(DB_URL, IMAGE_PATH).await?; println!("{b:#?}") },
            "r" => user_print_epub(DB_URL, IMAGE_PATH).await?,
            "d" => user_print_db(10, DB_URL).await?,
            "rd" => user_remove_db_entry(DB_URL).await?,
            "e" => break,
            _   => println!("didn't register input.")
        };
    }
    Ok(())
}

async fn user_remove_db_entry(url: &str) -> Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await?;

    let books = Book::db_read_to_books(50, &pool).await?;
    for (i, book) in books.iter().enumerate() {
        println!("{i}: {book:#?}");
    }

    let index: usize = select_element("Please enter a number: ", books.len());
    let b = books.get(index).ok_or(MissingInfoError)?;

    b.db_remove(&pool).await?;

    Ok(())
}
async fn user_search_books(url: &str, path: &str) -> Result<Book> {
    let search: SearchQuery = SearchQuery::poll_user();
    let json: SearchResp = search.get_ol_json().await?;
    let works: &Vec<Works> = json.get_works()?;

    for (i, work) in works.iter().enumerate() {
        print!("{}: ", i);
        println!("{:#?}", work);
    };

    let index: usize = select_element("Please enter a number: ", works.len());
    let mut b: Book = works.get(index)
        .map(|w| w.to_book())
        .transpose()?
        .ok_or(MissingInfoError)?;

    b.language = Some("eng".to_string());
    println!("{:#?}", b);
    while let Err(e) = b.poll_user() {
        println!("[error]: {}", e);
    };
    if let Some(_) = &b.cover_url && 
        let "y" = get_user_input("Download image? y/n: ")?.as_str() {
            b.download_image(path).await?;
    };

    b.db_add(url).await?;
    Ok(b)
}

async fn user_print_epub(url: &str, path: &str) -> Result<()> {
    let fp = get_user_input("Enter epub filepath: ")?;
    let doc = EpubDoc::new(&fp)?;
    let mut b = read_epub_to_book(&doc)?;

    println!("{:#?}", b);
    while let Err(e) = b.poll_user() {
        println!("[error]: {}", e);
    };
    if let "y" = get_user_input("Download image? y/n: ")?.as_str() {
        b.cover_path = download_epub_cover(&fp, path).ok();
    }
    b.db_add(url).await?;
    Ok(())
}

async fn user_print_db(limit: i32, url: &str) -> Result<()> {
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
