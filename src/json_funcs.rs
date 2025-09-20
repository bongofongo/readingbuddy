use crate::ol_api_containers::SearchResp;
use std::error::Error;

pub fn json_from_title(s: &str) -> Result<SearchResp, Box<dyn Error>> {
    let book_query: String = s
        .to_lowercase()
        .replace(' ', "+");
    println!("{}", book_query);
    let url = String::from("https://openlibrary.org/search.json?q=");

    let search_url = url + &book_query;
    let resp = reqwest::blocking::get(search_url)?
        .text()?;
    let res : SearchResp = serde_json::from_str(&resp)?;
    Ok(res)
}

