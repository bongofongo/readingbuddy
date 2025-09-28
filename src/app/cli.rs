use super::*;

use anyhow::Result;
use epub::doc::EpubDoc;
use sqlx::sqlite::SqlitePoolOptions;

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
    let b = books.get(index).ok_or(MissingInfoError)?;

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

pub async fn user_print_epub(url: &str, path: &str) -> Result<()> {
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
