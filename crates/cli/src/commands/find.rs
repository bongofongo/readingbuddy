//! `find` — the door onto item 33's one search surface.
//!
//! `Engine::search_marks` answers notes and highlights in one ranked list, and
//! without a command here that list would have been reachable only from the
//! daemon and from a GUI that has not been built yet — the same failure
//! `measure_stored_covers` had for a whole wave and `list --sort author` had
//! before it: the engine can do it and nothing can ask.
//!
//! `rb notes --search` still exists and still searches notes alone. It is not a
//! second door: it calls this same method with `SearchSource::Note`, so the two
//! cannot hold different opinions about what matching or ordering means.
//!
//! All the printing lives here; the engine does no terminal I/O.

use std::collections::HashMap;

use anyhow::Result;
use readingbuddy::{Engine, SearchHit, SearchSource};

/// How many hits a terminal gets. The engine takes any limit; this is a
/// screenful, and the flag is there for the reader who wants more.
pub const DEFAULT_LIMIT: i64 = 25;

pub async fn find(
    engine: &Engine,
    query: &str,
    source: Option<SearchSource>,
    limit: i64,
) -> Result<()> {
    // A note edited in Obsidian since the last run is a note the index has not
    // caught up with, and searching for it would report it missing.
    super::catch_up(engine).await;

    let hits = engine.search_marks(query, source, limit).await?;
    if hits.is_empty() {
        println!("{}", nothing(query, source));
        return Ok(());
    }

    // One lookup per distinct book rather than one per hit: a page of
    // highlights out of one book is the ordinary case.
    let mut titles: HashMap<i64, String> = HashMap::new();
    for id in hits.iter().filter_map(|h| h.book_id()) {
        if let std::collections::hash_map::Entry::Vacant(slot) = titles.entry(id)
            && let Some(book) = engine.get_book(id).await?
        {
            slot.insert(book.title.unwrap_or_else(|| format!("book #{id}")));
        }
    }

    for hit in &hits {
        let book = hit.book_id().and_then(|id| titles.get(&id)).cloned();
        println!("{}", headline(hit, book.as_deref()));
        println!("      {}", hit.snippet());
    }
    Ok(())
}

/// What is printed when nothing matched — a pure function so the wording can be
/// asserted.
///
/// Two rules from `docs/decisions.md` are load-bearing. **Absence is not zero**:
/// a search that found nothing says so, and never `0 results`, which states the
/// library as a quantity of failure. And an **empty query is not a search that
/// failed** — it is not asking — so it gets its own line naming the move rather
/// than reporting that the library contains no blankness.
fn nothing(query: &str, source: Option<SearchSource>) -> String {
    if query.trim().is_empty() {
        return "nothing to search for — try `readingbuddy find grief`".to_string();
    }
    let what = match source {
        None => "notes or highlights",
        Some(SearchSource::Note) => "notes",
        Some(SearchSource::Highlight) => "highlights",
    };
    format!("no {what} match '{query}'")
}

/// The line above a snippet: what kind of thing this is, its id, and where it
/// came from.
///
/// A pure function for the same reason `covers::summary` is one — this is
/// user-visible text and the alternative is asserting on stdout.
fn headline(hit: &SearchHit, book: Option<&str>) -> String {
    match hit {
        SearchHit::Note { note, .. } => {
            let where_from = match book {
                Some(title) => format!("{title} — {}", note.file_path),
                // An unanchored thought is an ordinary note, not a note with a
                // missing book.
                None => note.file_path.clone(),
            };
            format!("note      #{:<5} {}  ({where_from})", note.id, note.title)
        }
        SearchHit::Highlight { highlight, .. } => {
            let mut where_from = book.unwrap_or("").to_string();
            if let Some(page) = highlight.page {
                if !where_from.is_empty() {
                    where_from.push_str(" — ");
                }
                where_from.push_str(&format!("p.{page}"));
            }
            format!(
                "highlight #{:<5} {}",
                highlight.id,
                if where_from.is_empty() {
                    String::new()
                } else {
                    format!("({where_from})")
                }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use readingbuddy::{Highlight, NoteRecord};

    fn note(id: i64, book_id: Option<i64>) -> SearchHit {
        SearchHit::Note {
            note: NoteRecord {
                id,
                book_id,
                reading_id: None,
                highlight_id: None,
                page: None,
                location: None,
                file_path: "pachinko/1-on-grief.md".into(),
                title: "On grief".into(),
                kind: "note".into(),
                created_at: None,
            },
            snippet: "what >>grief<< does".into(),
        }
    }

    fn highlight(id: i64, page: Option<i64>) -> SearchHit {
        SearchHit::Highlight {
            highlight: Highlight {
                id,
                book_id: 1,
                text: "history has failed us".into(),
                chapter: None,
                page,
                ko_note: None,
                annotation: None,
                ko_datetime: None,
                reading_id: None,
                source: "koreader".into(),
                created_at: 0,
            },
            snippet: "history has >>failed<< us".into(),
        }
    }

    /// A search that found nothing is a fact about the query, not a count.
    #[test]
    fn nothing_found_is_never_reported_as_zero() {
        let s = nothing("thermodynamics", None);
        assert!(!s.contains('0'), "absence is not zero: {s}");
        assert!(
            s.contains("no notes or highlights match 'thermodynamics'"),
            "{s}"
        );
        assert!(
            nothing("x", Some(SearchSource::Highlight)).contains("no highlights match"),
            "the narrowing has to be named, or an empty answer looks like an \
             empty library"
        );
    }

    /// An empty search box is not a search that failed — it is a search nobody
    /// asked for, and it names the move rather than reporting a result.
    #[test]
    fn an_empty_query_is_not_a_failed_search() {
        let s = nothing("   ", None);
        assert!(s.contains("nothing to search for"), "{s}");
        assert!(!s.contains("match"), "{s}");
    }

    /// Both kinds are labelled, because a list mixing them is unreadable if the
    /// reader has to infer which is which from the shape of the text.
    #[test]
    fn every_hit_says_what_kind_of_thing_it_is() {
        assert!(headline(&note(12, Some(1)), Some("Pachinko")).starts_with("note "));
        assert!(headline(&highlight(48, Some(42)), Some("Pachinko")).starts_with("highlight "));
    }

    /// A note with no book is an ordinary note. It must not render as a book
    /// that went missing.
    #[test]
    fn an_unanchored_note_names_no_book() {
        let line = headline(&note(12, None), None);
        assert!(line.contains("On grief"), "{line}");
        assert!(!line.contains("—"), "{line}");
        assert!(!line.contains("None"), "{line}");
    }

    /// A highlight with no page still renders — `page` is nullable and a PDF
    /// sidecar is where that shows up.
    #[test]
    fn a_highlight_without_a_page_still_names_its_book() {
        let line = headline(&highlight(48, None), Some("Pachinko"));
        assert!(line.contains("Pachinko"), "{line}");
        assert!(!line.contains("p."), "{line}");
    }
}
