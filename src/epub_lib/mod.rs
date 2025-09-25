use epub::doc::EpubDoc;

use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::io::{Read, Seek};

use crate::structs::MissingInfoError;

pub fn download_epub_cover(fp: &str) -> Result<(), Box<dyn Error>> {
    let mut doc = match EpubDoc::new(fp) {
        Ok(d) => d,
        Err(e) => {  
            println!("{}", &fp);
            return Err(Box::new(e)) },
    };
    let cover_data = match doc.get_cover() {
        Some(c) => c,
        None => return Err(Box::new(MissingInfoError))
    };
    let name = format!("images/covers/{}.png", &fp);
    let mut f = File::create(&name)?;
    f.write_all(&cover_data.0)?;

    Ok(())
}

pub fn read_epub<R>(epub: &epub::doc::EpubDoc<R>) -> Result<(), Box<dyn Error>> 
where 
    R: Read + Seek, 
{
    for (key, val) in epub.metadata.iter() {
        print!("key: {}, val: ", key);
        for v in val.iter() {
            println!("{}", v);
        }
    };
    Ok(())
}
