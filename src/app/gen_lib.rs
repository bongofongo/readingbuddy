use std::{io::{copy, self, Write, Cursor}, fs::File};
use url::Url;
use sqlx::{migrate::MigrateDatabase, Sqlite, sqlite::{SqlitePool, SqlitePoolOptions}};
use anyhow::{Result,anyhow};

pub fn get_user_input(s: &str) -> Result<String> {
    print!("{}", s);
    let mut input = String::new();
    io::stdout().flush()?;
    io::stdin().read_line(&mut input)?;
    let user_selection: String = input.trim().to_string();
    Ok(user_selection)
}

pub fn select_element(s: &str, len: usize) -> usize {
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

pub async fn db_make_database(url: &str) -> Result<(), sqlx::Error> {
    if !Sqlite::database_exists(url).await.unwrap_or(false) {
        println!("Creating database {}", url);
        match Sqlite::create_database(url).await {
            Ok(_) => println!("Success"),
            Err(error) => return Err(error),
        }
    } 
    Ok(())
}

pub async fn db_create_books_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE TABLE IF NOT EXISTS books (
            id INTEGER PRIMARY KEY,
            title TEXT,
            authors TEXT,
            cover_url TEXT,
            cover_path TEXT,
            pagination INTEGER, 
            description TEXT,
            first_sentence TEXT,
            language TEXT,
            isbn_10 INTEGER,
            isbn_13 INTEGER,
            openlibrary_key TEXT,
            publish_year INTEGER,
            current_page INTEGER,
            finished INTEGER,
            date_started INTEGER,
            last_modified INTEGER NOT NULL,
            created_at INTEGER NOT NULL, 
            UNIQUE(isbn_10),
            UNIQUE(isbn_13)
            );
            ")
        .execute(pool).await?;

    Ok(())
}

// Creates db if it doesn't exist, and adds a books table to it.
pub async fn db_make(url: &str) -> Result<(), sqlx::Error> {
    db_make_database(url).await?;
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await?;
    db_create_books_table(&pool).await?;
    Ok(())
}


pub async fn image_from_url(url_str: &str, path: &str) -> Result<String> {
    let url = Url::parse(url_str)?;
    let path_vec = url.path_segments().ok_or(anyhow!("[image_from_url] couldn't find url path"))?;
    let mut fname: String = String::from(path);

    let s: &str = match path_vec.last() {
        Some(olid) => olid, 
        None => "random_fname.jpg"
    };

    fname.push_str(s);
    println!("\tDownloading... {}", fname);

    let response = reqwest::get(url).await?;
    let mut f = File::create(&fname)?;
    let mut content =  Cursor::new(response.bytes().await?);
    copy(&mut content, &mut f)?;
    Ok(fname.to_string())
}

