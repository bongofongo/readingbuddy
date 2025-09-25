use std::{error::Error, fs::File, io::copy};
use url::Url;

pub fn image_from_url(url_str : &str) -> Result<String, Box<dyn Error>> {
    let url = Url::parse(url_str)?;
    let path_vec = url.path_segments() .ok_or_else(|| "file_not_found")?;
    let mut fname: String = String::from("images/covers/");

    let s: &str = match path_vec.last() {
        Some(olid) => olid, 
        None => "random_fname.jpg"
    };

    fname.push_str(s);
    println!("\tDownloading... {}", fname);

    let mut response = reqwest::blocking::get(url.as_str())?;

    let mut f = File::create(&fname)?;
    copy(&mut response, &mut f)?;
    Ok(fname.to_string())
}
