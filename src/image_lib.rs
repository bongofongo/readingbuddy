use std::{error::Error, fs::File, io::copy};

pub fn image_from_url(url : url::Url) -> Result<(), Box<dyn Error>> {
    let path_vec = url.path_segments() .ok_or_else(|| "file_not_found")?;
    let mut fname: String = String::from("images/covers/");

    let s: &str = match path_vec.last() {
        Some(olid) => olid, 
        None => "random_fname.jpg"
    };

    fname.push_str(s);
    println!("\tDownloading... :{}", fname);

    let mut response = reqwest::blocking::get(url.as_str())?;

    let mut f = File::create(fname)?;
    copy(&mut response, &mut f)?;
    Ok(())
}
