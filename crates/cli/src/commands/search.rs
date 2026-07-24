use anyhow::Result;
use clap::Args;
use readingbuddy::{Engine, SearchRequest};

use crate::{prompt, render};

#[derive(Args)]
pub struct SearchArgs {
    /// Free-text query
    pub query: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub author: Option<String>,
    #[arg(long)]
    pub publisher: Option<String>,
    #[arg(long)]
    pub translator: Option<String>,
    /// 2-letter language code (en, fr, ...)
    #[arg(long)]
    pub lang: Option<String>,
    #[arg(long)]
    pub year: Option<i64>,
    #[arg(long)]
    pub isbn: Option<String>,
    /// Max results shown
    #[arg(long, default_value_t = 15)]
    pub limit: u32,
    /// Save result N without prompting
    #[arg(long)]
    pub pick: Option<usize>,
    /// Don't offer to save anything (browse only)
    #[arg(long)]
    pub no_save: bool,
    /// Skip cover download on save
    #[arg(long)]
    pub no_cover: bool,
}

impl SearchArgs {
    fn to_request(&self) -> SearchRequest {
        SearchRequest {
            query: self.query.clone(),
            title: self.title.clone(),
            author: self.author.clone(),
            publisher: self.publisher.clone(),
            translator: self.translator.clone(),
            language: self.lang.clone(),
            year: self.year,
            isbn: self.isbn.clone(),
            limit: self.limit.max(15),
        }
    }
}

pub async fn run(engine: &Engine, args: SearchArgs) -> Result<()> {
    let req = args.to_request();
    if req.is_empty() {
        anyhow::bail!("give me something to search: a query, --title, --author, --isbn, ...");
    }
    let outcome = engine.search(&req).await?;
    for w in &outcome.warnings {
        eprintln!("warning: {w}");
    }
    if outcome.results.is_empty() {
        println!("nothing found.");
        return Ok(());
    }

    let shown: Vec<_> = outcome.results.iter().take(args.limit as usize).collect();
    for (i, r) in shown.iter().enumerate() {
        println!("{}", render::search_result_line(i, r));
    }

    if args.no_save {
        return Ok(());
    }
    let pick = match args.pick {
        Some(n) => {
            anyhow::ensure!(
                n < shown.len(),
                "--pick {n} out of range (0..{})",
                shown.len()
            );
            Some(n)
        }
        None => prompt::select_element("\nsave which? (number, Enter to skip): ", shown.len())?,
    };
    let Some(i) = pick else {
        return Ok(());
    };

    let mut book = shown[i].book.clone();
    if !args.no_cover && book.cover_url.is_some() {
        match engine.download_cover(&mut book).await {
            Ok(Some(p)) => println!("cover -> {}", p.display()),
            Ok(None) => {}
            Err(e) => eprintln!("cover download failed: {e}"),
        }
    }
    let saved = engine.save_book(&book).await?;
    println!("saved {}", render::book_line(&saved));
    Ok(())
}
