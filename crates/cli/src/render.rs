use readingbuddy::{Book, RankedResult};

pub fn book_line(b: &Book) -> String {
    let id = b.id.map(|i| i.to_string()).unwrap_or_else(|| "-".into());
    let year = b
        .publish_year
        .map(|y| format!(" ({y})"))
        .unwrap_or_default();
    let progress = match (b.current_page, b.page_count) {
        _ if b.finished => "  [finished]".to_string(),
        (Some(p), Some(t)) => format!("  [{p}/{t}]"),
        (Some(p), None) => format!("  [p.{p}]"),
        _ => String::new(),
    };
    format!(
        "#{id}  {} — {}{year}{progress}",
        b.display_title(),
        b.display_authors()
    )
}

pub fn book_details(b: &Book) -> String {
    let mut out = String::new();
    let mut push = |label: &str, val: Option<String>| {
        if let Some(v) = val {
            out.push_str(&format!("  {label:<14} {v}\n"));
        }
    };
    push("id", b.id.map(|i| i.to_string()));
    push("title", b.title.clone());
    push(
        "authors",
        (!b.authors.is_empty()).then(|| b.authors.join(", ")),
    );
    push(
        "translators",
        (!b.translators.is_empty()).then(|| b.translators.join(", ")),
    );
    push("publisher", b.publisher.clone());
    push("year", b.publish_year.map(|y| y.to_string()));
    push("language", b.language.clone());
    push("isbn-10", b.isbn_10.clone());
    push("isbn-13", b.isbn_13.clone());
    push("pages", b.page_count.map(|p| p.to_string()));
    push("current page", b.current_page.map(|p| p.to_string()));
    push("finished", b.finished.then(|| "yes".to_string()));
    push("openlibrary", b.openlibrary_key.clone());
    push("googlebooks", b.googlebooks_id.clone());
    push("cover", b.cover_path.clone().or(b.cover_url.clone()));
    if let Some(d) = &b.description {
        let short: String = d.chars().take(400).collect();
        let ellipsis = if d.chars().count() > 400 { "…" } else { "" };
        out.push_str(&format!("  {:<14} {short}{ellipsis}\n", "description"));
    }
    if let Some(fs) = &b.first_sentence {
        out.push_str(&format!("  {:<14} “{fs}”\n", "first sentence"));
    }
    out
}

pub fn search_result_line(i: usize, r: &RankedResult) -> String {
    let b = &r.book;
    let year = b
        .publish_year
        .map(|y| format!(" ({y})"))
        .unwrap_or_default();
    let isbn = b
        .any_isbn()
        .map(|i| format!("  isbn:{i}"))
        .unwrap_or_default();
    let sources = r
        .sources
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join("+");
    format!(
        "{i:>3}. {} — {}{year}{isbn}  [{sources}, {:.1}]",
        b.display_title(),
        b.display_authors(),
        r.score
    )
}
