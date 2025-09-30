use super::*;
use anyhow::{Result, anyhow};
use::reqwest::Client;
use sqlx::sqlite::SqlitePoolOptions;

pub async fn run_cli (db_url: &str, image_path: &str) -> Result<()> {
    let q: &str = "\nBookBuddy:\
    \n\tSearch for books [s]\
    \n\tRead a .epub [r]\
    \n\tView database [d]\
    \n\tRemove database entry [rd]\
    \n\tExit [e]\nenter: ";

    loop {
        let input = gen_lib::get_user_input(q)?;

        match input.as_ref() {
            "s" => { let b = user_search_books(db_url, image_path).await?; println!("{b:#?}") },
            "r" => user_print_epub(db_url, image_path).await?,
            "d" => user_print_db(10, db_url).await?,
            "rd" => user_remove_db_entry(db_url).await?,
            "e" => break,
            _   => println!("didn't register input.")
        };
    }
    Ok(())
}

pub async fn user_remove_db_entry(url: &str) -> Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await?;

    let books = Book::db_read_to_books(50, &pool).await?;
    for (i, book) in books.iter().enumerate() {
        println!("{i}: {book:#?}");
    }

    let index: usize = select_element("Please enter a number: ", books.len());
    let b = books.get(index).ok_or(anyhow!("[user_remove_db_entry] index didn't parse"))?;

    b.db_remove(&pool).await?;

    Ok(())
}
pub async fn user_search_books(url: &str, path: &str) -> Result<Book> {
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
        .ok_or(anyhow!("[user_search_books] index didn't parse!"))?;

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

pub async fn user_print_epub(url: &str, path: &str) -> Result<()> {
    let fp = get_user_input("Enter epub filepath: ")?;
    let client = Client::new();
    let mut book: Book = epub_to_ol_book(&fp, path, &client).await?;

    println!("{:#?}", book);
    while let Err(e) = book.poll_user() {
        println!("[error]: {}", e);
    };
    book.db_add(url).await?;
    Ok(())
}

pub async fn user_print_db(limit: i32, url: &str) -> Result<()> {
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
