use url::Url;
use crate::{ol_api_containers::SearchResp};
use std::{error::Error, io};
use reqwest::Client;

pub struct SearchQuery {
    author : Option<String>,
    title : Option<String>,
    lang : Option<String>,
    sort : Option<String>,
}

impl SearchQuery {
    pub fn new(
        author : Option<String>,
        title : Option<String>,
        lang : Option<String>,
        sort : Option<String>
    ) -> Self {
        SearchQuery { author, title, lang, sort }
    }

    fn flatten(&self) -> Vec<Option<&str>> {
        vec![self.author.as_deref(), 
             self.title.as_deref(), 
             self.lang.as_deref(), 
             self.sort.as_deref(),]
    }
    
    fn get_keys(&self) -> Vec<&str> {
        vec!["author", "title", "lang", "sort"]
    }

    pub fn url_of_query (&self) -> Result<String, Box<dyn Error>> {
        let params: Vec<(&str, &str)> = self.get_keys().into_iter()
            .zip(self.flatten().iter())
            .filter_map(|(k, str_opt)| str_opt.map(|s| (k, s)))
            .collect();

        let mut url : Url = Url::parse_with_params(
            "https://openlibrary.org/search.json?limit=30",
            &params)?;
        url.query_pairs_mut() 
            .append_pair("fields", "key,title,\
            author_name, isbn,language, first_sentence,\
            first_publish_year, cover_edition_key, edition_key");
        
        println!("URL: {}", url.as_str());
        Ok(url.into())
    }

    fn str_to_opt(s: String) -> Option<String> {
        let t = s.trim();
        if t.is_empty() { None } else { Some(t.to_owned())}
    }

    pub fn poll_user() -> SearchQuery {
        let mut t = String::new();
        let mut a = String::new();

        println!("\nEnter Book Title: ");
        let _ = io::stdin().read_line(&mut t);
        println!("Enter Author: ");
        let _ = io::stdin().read_line(&mut a);

        let title = SearchQuery::str_to_opt(t);
        let author = SearchQuery::str_to_opt(a);
       
        SearchQuery { title, author, lang: Some("eng".to_string()), sort: None }
    }

    pub async fn get_ol_json(&self) -> Result<SearchResp, Box<dyn Error>> {
        let url= self.url_of_query()?;
        let client = Client::new(); // reuse this (Arc) across calls
        let resp = client.get(url).send().await?
            .error_for_status()?.text().await?;
        // let resp = reqwest::blocking::get(url)?
        //     .text()?;
        let res: SearchResp = serde_json::from_str(&resp)?;
        Ok(res)
    }

}
