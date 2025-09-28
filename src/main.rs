mod app;

use crate::app::*;
use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;

const DB_URL: &str = "sqlite://database/app.db";
const IMAGE_PATH: &str = "database/images/";

#[tokio::main]
async fn main() -> Result<()> {
    gen_lib::db_make(DB_URL).await?;    

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
        let input = gen_lib::get_user_input(q)?;

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

