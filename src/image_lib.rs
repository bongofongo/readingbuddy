use std::{error::Error, fs::File, io::{copy, Cursor}};
use url::Url;

pub async fn image_from_url(url_str: &str, path: &str) -> Result<String, Box<dyn Error>> {
    let url = Url::parse(url_str)?;
    let path_vec = url.path_segments().ok_or("file_not_found")?;
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
