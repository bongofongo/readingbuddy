use super::books::Book;
use super::edition::*;
use anyhow::{Result, anyhow};
use epub::doc::EpubDoc;
use std::fs::File;
use std::io::{Write, Cursor, Read, Seek};
use reqwest::Client;

pub async fn epub_to_ol_book(fp: &str, image_path: &str, client: &Client) -> Result<Book> {
    let mut file = File::open(fp)?;
    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer)?;

    let cursor = Cursor::new(buffer);
    let doc = EpubDoc::from_reader(cursor)?;

    let mut isbn: String = String::new();
    for (key, val) in doc.metadata.iter() {
        if key == "identifier" {
            isbn.push_str(val.first().ok_or(anyhow!("error"))?);
        }
    };

    let text = ol_edition_of_isbn(&isbn, client).await?;
    let edition: EditionJson = serde_json::from_str(&text)?;

    let mut book: Book = edition_to_book(edition, client).await?;

    book.cover_path = download_epub_cover(doc, image_path);
    
    Ok(book)
}


fn download_epub_cover<R>(mut doc: EpubDoc<R>, image_path: &str) -> Option<String> 
where 
    R: Read + Seek 
{
    let cover_data = doc.get_cover()?;
    let image = doc.mdata("title")?;
    let name = format!("{}{}.png", &image_path, &image);
    let mut f = File::create(&name).ok()?;
    f.write_all(&cover_data.0).ok()?;

    Some(name)
}

