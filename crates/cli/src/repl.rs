use anyhow::Result;
use readingbuddy::Engine;

use crate::commands::{self, search::SearchArgs};
use crate::prompt::get_user_input;

/// Interactive mode — the descendant of the original BookBuddy s/r/d/rd loop.
pub async fn run(engine: &Engine) -> Result<()> {
    let menu = "\nreadingbuddy:\
        \n\tsearch for books        [s]\
        \n\timport an .epub         [r]\
        \n\tview library            [d]\
        \n\twrite a note            [n]\
        \n\timport koreader dir     [k]\
        \n\tremove a book           [rd]\
        \n\texit                    [e]\nenter: ";

    loop {
        let input = get_user_input(menu)?;
        let result = match input.as_str() {
            "s" => search(engine).await,
            "r" => epub(engine).await,
            "d" => commands::book::list(engine, 20, "last-modified").await,
            "n" => note(engine).await,
            "k" => koreader(engine).await,
            "rd" => remove(engine).await,
            "e" => break,
            _ => {
                println!("didn't register input.");
                Ok(())
            }
        };
        if let Err(e) = result {
            eprintln!("[error]: {e}");
        }
    }
    Ok(())
}

fn opt(s: String) -> Option<String> {
    (!s.trim().is_empty()).then(|| s.trim().to_string())
}

async fn search(engine: &Engine) -> Result<()> {
    let title = opt(get_user_input("title: ")?);
    let author = opt(get_user_input("author: ")?);
    let query = opt(get_user_input("free text: ")?);
    let args = SearchArgs {
        query,
        title,
        author,
        publisher: None,
        translator: None,
        lang: None,
        year: None,
        isbn: None,
        limit: 15,
        pick: None,
        no_save: false,
        no_cover: false,
    };
    commands::search::run(engine, args).await
}

async fn epub(engine: &Engine) -> Result<()> {
    let fp = get_user_input("epub filepath: ")?;
    if fp.is_empty() {
        return Ok(());
    }
    commands::book::import_epub(engine, std::path::Path::new(&fp)).await
}

async fn note(engine: &Engine) -> Result<()> {
    let book = opt(get_user_input("book (id/isbn/title, empty for none): ")?);
    let text = get_user_input("note: ")?;
    commands::note::create(engine, book.as_deref(), opt(text), "note", None).await
}

async fn koreader(engine: &Engine) -> Result<()> {
    let dir = get_user_input("koreader library / .sdr path: ")?;
    if dir.is_empty() {
        return Ok(());
    }
    commands::ko::import(engine, std::path::Path::new(&dir), false).await
}

async fn remove(engine: &Engine) -> Result<()> {
    let sel = get_user_input("which book (id/isbn/title): ")?;
    if sel.is_empty() {
        return Ok(());
    }
    commands::book::remove(engine, &sel, false).await
}
