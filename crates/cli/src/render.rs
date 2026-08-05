use readingbuddy::{Book, RankedResult, Reading};

/// One reading of a book: which of how many, its dates, and where it got to.
///
/// The device-owned mirror is shown separately from ours (`ko …`) because they
/// are different claims — theirs is what the reader said, ours is what the user
/// told readingbuddy — and collapsing them would hide a disagreement.
pub fn reading_line(r: &Reading, nth: usize, of: usize) -> String {
    let dates = match (r.started_at, r.finished_at) {
        (Some(a), Some(b)) => format!("{} → {}", date(a), date(b)),
        (Some(a), None) => format!("{} → …", date(a)),
        (None, Some(b)) => format!("? → {}", date(b)),
        (None, None) => "no dates".to_string(),
    };
    let page = r
        .current_page
        .map(|p| format!("  p.{p}"))
        .unwrap_or_default();
    let device = match (&r.ko_status, r.ko_percent, r.ko_rating) {
        (None, None, None) => String::new(),
        (s, pct, rating) => {
            let mut parts = Vec::new();
            if let Some(s) = s {
                parts.push(s.clone());
            }
            if let Some(p) = pct {
                parts.push(format!("{:.0}%", p * 100.0));
            }
            if let Some(n) = rating {
                parts.push(format!("{n}★"));
            }
            format!("  [ko {}]", parts.join(" "))
        }
    };
    format!(
        "reading {nth}/{of}  {}  {dates}{page}{device}  ({})",
        r.status, r.source
    )
}

/// Unix seconds as a plain date. Times of day are noise here — a reading is a
/// span of days.
fn date(unix: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .map(|d| format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day()))
        .unwrap_or_else(|| unix.to_string())
}

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
    // One line, because the pair is one fact: `Dune #2`, not a name and a
    // number on separate rows for the reader to put back together.
    push("series", b.series_label());
    push(
        "subjects",
        (!b.subjects.is_empty()).then(|| b.subjects.join(", ")),
    );
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
