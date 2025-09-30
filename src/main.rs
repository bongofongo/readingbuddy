mod app;

use crate::app::{cli::run_cli, gen_lib};
use anyhow::Result;
use tokio::time::{sleep, Duration};


const DB_URL: &str = "sqlite://database/app.db";
const IMAGE_PATH: &str = "database/images/";

#[tokio::main]
async fn main() -> Result<()> {
    gen_lib::db_make(DB_URL).await?;    

    while let Err(e) = run_cli(DB_URL, IMAGE_PATH).await {
        eprintln!("[error]: {}", e);
        sleep(Duration::from_secs(1)).await;
    };
    // iced::run("BookApp", BookApp::update, BookApp::view)?;
    Ok(())
}


