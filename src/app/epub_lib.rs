use epub::doc::EpubDoc;
use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::io::{Read, Seek};

use super::books::{MissingInfoError, Book};

pub fn download_epub_cover(fp: &str, image_path: &str) -> Result<String> {
    let mut doc = match EpubDoc::new(fp) {
        Ok(d) => d,
        Err(e) => return Err(e.into()),
    };
    let cover_data = match doc.get_cover() {
        Some(c) => c,
        None => return Err(MissingInfoError.into())
    };
    let image = doc.mdata("title").ok_or(MissingInfoError)?;
    let name = format!("{}{}.png", &image_path, &image);
    let mut f = File::create(&name)?;
    f.write_all(&cover_data.0)?;

    Ok(name)
}

pub fn read_epub_to_book<R>(epub: &epub::doc::EpubDoc<R>) -> Result<Book> 
where 
    R: Read + Seek, 
{
    let mut b = Book::new();
    for (key, val) in epub.metadata.iter() {
        let v = val.clone();
        match key.as_str() {
            "creator" => b.author = Some(v),
            "identifier" => b.isbn = Some(v.concat().parse::<i64>()?),
            "language" => b.language = Some(v.concat()),
            "title" => b.title = Some(v.concat()),
            "description" => b.description = Some(v.concat()),
            _ => continue,
        }
    };
    Ok(b)
}
