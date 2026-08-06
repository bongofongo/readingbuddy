//! One search over everything the reader wrote or kept.
//!
//! Two fts5 indexes exist — `notes_fts` (migration `0001`, an ordinary table
//! maintained from application code because a note's body is on disk) and
//! `highlights_fts` (migration `0015`, external content over `highlights`,
//! maintained by triggers because a highlight's text is a column). This module
//! is the one place they are read, and [`Storage::search_marks`] is the one
//! method that reads them.
//!
//! Not to be confused with [`crate::search`], which is the federated *provider*
//! search — asking OpenLibrary and Google Books about a book that is not here
//! yet. This module asks the library about reading that already happened.
//!
//! ## One list, and how it is ordered
//!
//! Two lists a frontend interleaves is a relevance ordering invented above the
//! seam: it has no way to say a note outranks a highlight, so it interleaves by
//! whatever it has, which is source order. So this returns **one ordered list**.
//!
//! What it deliberately does *not* do is order that list by `bm25`. fts5's rank
//! is computed from **that index's own corpus** — its document frequencies and
//! its average document length — so a note scoring −8.2 and a highlight scoring
//! −8.2 are not making the same claim, and there is no constant that converts
//! one into the other. Sorting a union on the two numbers would be arithmetic on
//! incommensurable units, which is worse than source order because it *looks*
//! like relevance.
//!
//! So each index is asked separately and ordered by its own `rank`, which is
//! the one thing bm25 can honestly support, and the two ordered lists are
//! merged **by within-source position**: the best note and the best highlight
//! come first, then the second-best of each, and so on. That is reciprocal rank
//! fusion with one source per item, where the score is `1/(k + rank)` and every
//! item appears in exactly one list — so the ordering is identical to ordering
//! by rank, and no `k` has to be picked. It claims only what bm25 supports (an
//! order *within* a source) and nothing it does not (a comparison across them).
//!
//! It has one consequence worth stating rather than discovering. A query
//! matching fifty notes strongly and one highlight weakly puts that weak
//! highlight at the top beside the best note. That is the intended behaviour
//! here: the question a search box in this app answers is "where did I read
//! that", and a single matching passage buried at position 47 is a passage the
//! reader will never see. Both kinds surface immediately, which is the
//! "a place, not a tool" reading of the same list.
//!
//! ## Ties, and the total order underneath
//!
//! Two hits at the same position need a rule, for `list_books`' reason: a page
//! is only the successor of the one before it if both statements break ties the
//! same way. The rule is **newer first**, which is the one key that genuinely is
//! comparable across the two — `notes.created_at` and `highlights.created_at`
//! are both "when we stored it", in unix seconds, off one clock. A note with no
//! decodable timestamp sorts last rather than first, then the kind, then the id,
//! which closes the order completely.
//!
//! ## Absence, and what an empty query is
//!
//! `MATCH ''` is an fts5 **syntax error**, not an empty result — so an empty
//! query had to be decided rather than passed through. It returns no hits and
//! no error, and issues no statement: an empty search box is not asking, and
//! "everything" is not what it would mean if it were. Whitespace is the same
//! answer.
//!
//! The same measurement settled a live defect in the old `search_notes`, which
//! bound the user's raw text straight into `MATCH`. `don't` and `C++` are both
//! fts5 syntax errors, so `rb notes --search "don't"` failed with a raw sqlx
//! error rather than a search. [`fts_query`] is the fix: every token becomes a
//! quoted phrase, which is the one fts5 form that cannot contain an operator.

use sqlx::Row;

use super::highlights::{HIGHLIGHT_COLUMNS, row_to_highlight};
use super::notes::{NOTE_COLUMNS, qualified, row_to_note};
use super::{Highlight, NoteRecord, Storage};
use crate::error::Result;

/// Which of the two indexes a search is about.
///
/// A filter, in `BookFilter`'s idiom: `None` is *not asking*, and asking for one
/// narrows what is in the list without ever changing how it is ordered. It
/// exists because "search my notes" is a real question — `rb notes --search`
/// has asked it since item 7 — while the *ordering* is never the caller's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSource {
    Note,
    Highlight,
}

/// One hit, from whichever index held it.
///
/// An enum rather than a struct with two `Option`s: exactly one of the two is
/// present, and a shape that can represent neither or both is a shape somebody
/// eventually reaches for `unwrap` on.
#[derive(Debug, Clone)]
pub enum SearchHit {
    Note {
        note: NoteRecord,
        /// The matching passage with `>>`/`<<` around the terms, as
        /// `search_notes` has always shaped it.
        snippet: String,
    },
    Highlight {
        highlight: Highlight,
        snippet: String,
    },
}

impl SearchHit {
    pub fn source(&self) -> SearchSource {
        match self {
            SearchHit::Note { .. } => SearchSource::Note,
            SearchHit::Highlight { .. } => SearchSource::Highlight,
        }
    }

    pub fn snippet(&self) -> &str {
        match self {
            SearchHit::Note { snippet, .. } | SearchHit::Highlight { snippet, .. } => snippet,
        }
    }

    pub fn as_note(&self) -> Option<&NoteRecord> {
        match self {
            SearchHit::Note { note, .. } => Some(note),
            _ => None,
        }
    }

    pub fn as_highlight(&self) -> Option<&Highlight> {
        match self {
            SearchHit::Highlight { highlight, .. } => Some(highlight),
            _ => None,
        }
    }

    /// Which book this hit is about, when it is about one.
    ///
    /// A highlight always has a book; a note need not — an unanchored thought is
    /// an ordinary note. Derived here rather than in three frontends.
    pub fn book_id(&self) -> Option<i64> {
        match self {
            SearchHit::Note { note, .. } => note.book_id,
            SearchHit::Highlight { highlight, .. } => Some(highlight.book_id),
        }
    }

    /// When we stored it, unix seconds — the one key comparable across both
    /// kinds, and therefore the tie-break between them.
    fn stored_at(&self) -> Option<i64> {
        match self {
            SearchHit::Note { note, .. } => note.created_at.map(|t| t.unix_timestamp()),
            SearchHit::Highlight { highlight, .. } => Some(highlight.created_at),
        }
    }

    fn id(&self) -> i64 {
        match self {
            SearchHit::Note { note, .. } => note.id,
            SearchHit::Highlight { highlight, .. } => highlight.id,
        }
    }
}

/// How much context a snippet carries — twelve tokens, which is the number
/// `search_notes` has answered in since item 7.
///
/// Named once because the two queries must not shape their snippets
/// differently: a frontend rendering one list cannot have two conventions in
/// it.
///
/// The `-1` at both call sites is the other half: it selects the column the
/// match is actually in, which is what lets `highlights_fts` index three
/// columns and still answer in one string.
const SNIPPET_TOKENS: i64 = 12;

/// Turn what a human typed into an fts5 query, or `None` when they typed
/// nothing.
///
/// **Every token becomes a quoted phrase.** fts5's query language reads `-`,
/// `+`, `*`, `:`, `^`, `(`, `,`, `NEAR`, `AND`, `OR` and a bare `'` as syntax,
/// so `don't` and `C++` are syntax errors rather than searches — measured
/// against sqlite3 3.51, and the reason the old `search_notes` failed on an
/// apostrophe. A double-quoted phrase is the one form in which none of that is
/// operator text, and `""` is how a quote escapes itself inside one.
///
/// Tokens are joined by a space, which fts5 reads as **AND**: two words both
/// have to appear, which is what a search box means by two words.
///
/// The one piece of syntax that survives is a **trailing `*`**, kept because
/// prefix search is the thing a reader reaches for on purpose (`resil*`) and
/// because `"resil"*` is a legal phrase-prefix. Nothing else is passed through:
/// a query language is a surface, and this is a search box.
pub(super) fn fts_query(raw: &str) -> Option<String> {
    let mut phrases: Vec<String> = Vec::new();
    for token in raw.split_whitespace() {
        let (body, prefix) = match token.strip_suffix('*') {
            Some(stem) => (stem, true),
            None => (token, false),
        };
        if body.is_empty() {
            // A lone `*` is punctuation, not a query for everything.
            continue;
        }
        let quoted = body.replace('"', "\"\"");
        phrases.push(if prefix {
            format!("\"{quoted}\"*")
        } else {
            format!("\"{quoted}\"")
        });
    }
    (!phrases.is_empty()).then(|| phrases.join(" "))
}

/// Merge two rank-ordered lists into one, by within-source position.
///
/// Separated from the queries so the ordering can be asserted without a
/// database — the rule is the substance of this module and a test that has to
/// seed two indexes to reach it is a test that mostly exercises fts5.
fn interleave(notes: Vec<SearchHit>, highlights: Vec<SearchHit>, limit: usize) -> Vec<SearchHit> {
    let mut ranked: Vec<(usize, SearchHit)> = Vec::with_capacity(notes.len() + highlights.len());
    for (position, hit) in notes.into_iter().enumerate() {
        ranked.push((position, hit));
    }
    for (position, hit) in highlights.into_iter().enumerate() {
        ranked.push((position, hit));
    }
    // A stable sort over a total key. `stored_at` descending is spelled as
    // `Reverse` on the key rather than by flipping the comparison, so the three
    // parts of the tie-break read in one direction. A hit with no decodable
    // timestamp sorts last: `None` is not "the beginning of time".
    ranked.sort_by_key(|(position, hit)| {
        (
            *position,
            hit.stored_at().is_none(),
            std::cmp::Reverse(hit.stored_at().unwrap_or(0)),
            match hit.source() {
                SearchSource::Note => 0u8,
                SearchSource::Highlight => 1,
            },
            hit.id(),
        )
    });
    ranked.into_iter().take(limit).map(|(_, hit)| hit).collect()
}

impl Storage {
    /// Notes and highlights matching one query, as **one ranked list**.
    ///
    /// `source` narrows which indexes are asked; `None` asks both. `limit` is
    /// the length of the answer, and each index is asked for at most that many
    /// so the merge has something to choose from at every position.
    ///
    /// An empty or whitespace-only query is no hits and no error — see the
    /// module header, and note that it is not merely a shortcut: `MATCH ''`
    /// raises rather than returning nothing.
    pub async fn search_marks(
        &self,
        query: &str,
        source: Option<SearchSource>,
        limit: i64,
    ) -> Result<Vec<SearchHit>> {
        let Some(prepared) = fts_query(query) else {
            return Ok(Vec::new());
        };
        // The prepared query is the user's own words. `trace!` is the ceiling
        // for a search query and this is the only line that names one at all.
        tracing::trace!(query = %prepared, "searching notes and highlights");
        let want = limit.max(0);

        let notes = match source {
            Some(SearchSource::Highlight) => Vec::new(),
            _ => self.search_note_index(&prepared, want).await?,
        };
        let highlights = match source {
            Some(SearchSource::Note) => Vec::new(),
            _ => self.search_highlight_index(&prepared, want).await?,
        };
        let hits = interleave(notes, highlights, want as usize);
        tracing::debug!(hits = hits.len(), "search answered");
        Ok(hits)
    }

    /// `notes_fts`, in its own bm25 order.
    async fn search_note_index(&self, prepared: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let columns = qualified(NOTE_COLUMNS, "n");
        let sql = format!(
            "SELECT {columns},
                    snippet(notes_fts, -1, '>>', '<<', '…', {SNIPPET_TOKENS}) AS snip
             FROM notes_fts JOIN notes n ON n.id = notes_fts.rowid
             WHERE notes_fts MATCH ? ORDER BY rank LIMIT ?"
        );
        let rows = sqlx::query(&sql)
            .bind(prepared)
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .iter()
            .map(|r| SearchHit::Note {
                note: row_to_note(r),
                snippet: r.get("snip"),
            })
            .collect())
    }

    /// `highlights_fts`, in its own bm25 order.
    async fn search_highlight_index(&self, prepared: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let columns = qualified(HIGHLIGHT_COLUMNS, "h");
        let sql = format!(
            "SELECT {columns},
                    snippet(highlights_fts, -1, '>>', '<<', '…', {SNIPPET_TOKENS}) AS snip
             FROM highlights_fts JOIN highlights h ON h.id = highlights_fts.rowid
             WHERE highlights_fts MATCH ? ORDER BY rank LIMIT ?"
        );
        let rows = sqlx::query(&sql)
            .bind(prepared)
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .iter()
            .map(|r| SearchHit::Highlight {
                highlight: row_to_highlight(r),
                snippet: r.get("snip"),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::storage::{NewHighlight, NewNoteMeta};

    // ---- the query preparation --------------------------------------------

    /// The whole reason this function exists. Both of these were `MATCH`
    /// syntax errors reaching the caller as a raw sqlx error, and both are
    /// ordinary things to type into a search box.
    #[test]
    fn punctuation_a_reader_types_is_not_a_syntax_error() {
        assert_eq!(fts_query("don't"), Some(r#""don't""#.into()));
        assert_eq!(fts_query("C++"), Some(r#""C++""#.into()));
        assert_eq!(fts_query("grief -- and after"), {
            Some(r#""grief" "--" "and" "after""#.into())
        });
    }

    /// A quote inside the needle must not close the phrase the needle is in.
    #[test]
    fn a_quote_cannot_escape_its_own_phrase() {
        assert_eq!(
            fts_query(r#"say "hello""#),
            Some(r#""say" """hello""""#.into())
        );
    }

    /// Absence is not a search for everything, and it is not an error either.
    #[test]
    fn an_empty_query_is_not_asking() {
        assert_eq!(fts_query(""), None);
        assert_eq!(fts_query("   \t\n "), None);
        assert_eq!(fts_query("*"), None, "a lone star is punctuation");
    }

    /// The one piece of syntax that survives, because a reader reaches for it
    /// on purpose.
    #[test]
    fn a_trailing_star_stays_a_prefix_search() {
        assert_eq!(fts_query("resil*"), Some(r#""resil"*"#.into()));
        assert_eq!(fts_query("a resil*"), Some(r#""a" "resil"*"#.into()));
    }

    // ---- the merge ---------------------------------------------------------

    fn note_hit(id: i64, at: i64) -> SearchHit {
        SearchHit::Note {
            note: NoteRecord {
                id,
                book_id: None,
                reading_id: None,
                highlight_id: None,
                page: None,
                location: None,
                file_path: format!("unsorted/{id}.md"),
                title: format!("note {id}"),
                kind: "note".into(),
                created_at: time::OffsetDateTime::from_unix_timestamp(at).ok(),
            },
            snippet: String::new(),
        }
    }

    fn highlight_hit(id: i64, at: i64) -> SearchHit {
        SearchHit::Highlight {
            highlight: Highlight {
                id,
                book_id: 1,
                text: format!("passage {id}"),
                chapter: None,
                page: None,
                ko_note: None,
                annotation: None,
                ko_datetime: None,
                reading_id: None,
                source: "koreader".into(),
                created_at: at,
            },
            snippet: String::new(),
        }
    }

    /// The rule, stated as a test: position first, so the best of each kind
    /// share the top of the list rather than one kind owning it.
    #[test]
    fn the_two_sources_interleave_by_their_own_rank() {
        let notes = vec![note_hit(1, 100), note_hit(2, 100), note_hit(3, 100)];
        let highlights = vec![highlight_hit(10, 200), highlight_hit(11, 200)];
        let merged = interleave(notes, highlights, 10);
        assert_eq!(
            merged.iter().map(|h| h.id()).collect::<Vec<_>>(),
            vec![10, 1, 11, 2, 3],
            "position 0 holds both firsts, and the newer one leads"
        );
    }

    /// Within a source the order is that source's own bm25 order, and the merge
    /// may never disturb it — a frontend that saw its 5th hit above its 3rd
    /// would be looking at a ranking nothing produced.
    #[test]
    fn a_source_is_never_reordered_against_itself() {
        let notes = (0..6).map(|i| note_hit(i, 500 - i)).collect::<Vec<_>>();
        let highlights = (0..6)
            .map(|i| highlight_hit(100 + i, i))
            .collect::<Vec<_>>();
        let merged = interleave(notes, highlights, 20);
        let note_order: Vec<i64> = merged
            .iter()
            .filter_map(|h| h.as_note())
            .map(|n| n.id)
            .collect();
        assert_eq!(note_order, vec![0, 1, 2, 3, 4, 5]);
        let hl_order: Vec<i64> = merged
            .iter()
            .filter_map(|h| h.as_highlight())
            .map(|h| h.id)
            .collect();
        assert_eq!(hl_order, vec![100, 101, 102, 103, 104, 105]);
    }

    /// One source matching is the degenerate case, and it has to be that
    /// source's plain ranked list — not a list with gaps where the other kind
    /// would have gone.
    #[test]
    fn one_source_alone_is_its_own_ranked_list() {
        let notes = vec![note_hit(1, 3), note_hit(2, 2), note_hit(3, 1)];
        let merged = interleave(notes, Vec::new(), 10);
        assert_eq!(
            merged.iter().map(|h| h.id()).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    /// A note with no decodable timestamp must not win the tie by being read as
    /// the beginning of time — it sorts last at its position, and the order
    /// stays total.
    #[test]
    fn a_hit_with_no_timestamp_sorts_last_at_its_position() {
        let undated = SearchHit::Note {
            note: NoteRecord {
                created_at: None,
                ..match note_hit(1, 0) {
                    SearchHit::Note { note, .. } => note,
                    _ => unreachable!(),
                }
            },
            snippet: String::new(),
        };
        let merged = interleave(vec![undated], vec![highlight_hit(10, 1)], 10);
        assert_eq!(
            merged.iter().map(|h| h.id()).collect::<Vec<_>>(),
            vec![10, 1]
        );
    }

    // ---- against a real database ------------------------------------------

    async fn seeded() -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        (s, book)
    }

    fn hl(text: &str) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: Some("Ch 1".into()),
            page: Some(42),
            pos0: Some("/body/DocFragment[8]/p[3]/text().0".into()),
            pos1: None,
            ko_datetime: Some("2026-01-01 10:00:00".into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    /// The one thing item 27 exists to do: a highlight that arrived through the
    /// ordinary insert path is findable.
    #[tokio::test]
    async fn an_imported_highlight_is_findable() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("history has failed us, but no matter"))
            .await
            .unwrap();

        let hits = s.search_marks("failed", None, 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].as_highlight().unwrap().text,
            "history has failed us, but no matter"
        );
        assert!(
            hits[0].snippet().contains(">>failed<<"),
            "{}",
            hits[0].snippet()
        );
        assert_eq!(hits[0].book_id(), Some(book));
    }

    /// A note and a highlight come back in **one** list, with snippets, and the
    /// caller is never handed two.
    #[tokio::test]
    async fn a_note_and_a_highlight_share_one_list() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("the antechamber of grief"))
            .await
            .unwrap();
        s.insert_note(
            NewNoteMeta {
                book_id: Some(book),
                reading_id: None,
                highlight_id: None,
                page: None,
                location: None,
                file_path: "pachinko/a.md",
                title: "On grief",
                kind: "note",
            },
            "What grief does to a family over four generations.",
            &[],
        )
        .await
        .unwrap();

        let hits = s.search_marks("grief", None, 10).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.as_note().is_some()));
        assert!(hits.iter().any(|h| h.as_highlight().is_some()));
        for h in &hits {
            assert!(h.snippet().contains(">>"), "every hit carries a snippet");
        }
    }

    /// The narrowing, and what it does *not* change: the order.
    #[tokio::test]
    async fn a_source_filter_narrows_the_list_and_nothing_else() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("the antechamber of grief"))
            .await
            .unwrap();
        s.insert_note(
            NewNoteMeta {
                book_id: Some(book),
                reading_id: None,
                highlight_id: None,
                page: None,
                location: None,
                file_path: "pachinko/a.md",
                title: "On grief",
                kind: "note",
            },
            "What grief does to a family.",
            &[],
        )
        .await
        .unwrap();

        let notes = s
            .search_marks("grief", Some(SearchSource::Note), 10)
            .await
            .unwrap();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].as_note().is_some());

        let highlights = s
            .search_marks("grief", Some(SearchSource::Highlight), 10)
            .await
            .unwrap();
        assert_eq!(highlights.len(), 1);
        assert!(highlights[0].as_highlight().is_some());
    }

    /// `MATCH ''` raises, so this is not a shortcut around a query that would
    /// have returned nothing — it is the difference between an answer and an
    /// error.
    #[tokio::test]
    async fn an_empty_query_answers_nothing_rather_than_failing() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("a passage worth keeping"))
            .await
            .unwrap();
        for q in ["", "   ", "\t"] {
            assert!(s.search_marks(q, None, 10).await.unwrap().is_empty());
        }
    }

    /// The old `search_notes` failed here with a raw sqlx error. It is the most
    /// ordinary thing in the world to type.
    #[tokio::test]
    async fn an_apostrophe_is_searched_for_rather_than_parsed() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("she couldn't say it aloud"))
            .await
            .unwrap();
        let hits = s.search_marks("couldn't", None, 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert!(s.search_marks("C++", None, 10).await.unwrap().is_empty());
    }

    /// A search with no hits is an answer. Nothing above this layer may read it
    /// as a failure, and nothing here may raise.
    #[tokio::test]
    async fn no_hits_is_an_answer() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("a passage worth keeping"))
            .await
            .unwrap();
        assert!(
            s.search_marks("thermodynamics", None, 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    // ---- the index cannot fall behind the table ---------------------------

    /// Every write path to `highlights`, one at a time. This is the property
    /// migration `0015` chose triggers for: a highlight in the table and not in
    /// the index is invisible to the search, and nothing would say so.
    #[tokio::test]
    async fn every_write_path_moves_the_index() {
        let (s, book) = seeded().await;
        let found = |s: Storage, q: &'static str| async move {
            s.search_marks(q, Some(SearchSource::Highlight), 10)
                .await
                .unwrap()
                .len()
        };

        // insert
        let h = hl("history has failed us");
        let id = s.insert_highlight(book, &h).await.unwrap().unwrap();
        assert_eq!(found(s.clone(), "failed").await, 1);

        // the device's own note, through refresh_device_fields
        let edited = NewHighlight {
            note: Some("they mean the occupation".into()),
            ..h.clone()
        };
        assert!(s.refresh_device_fields(book, &edited).await.unwrap());
        assert_eq!(found(s.clone(), "occupation").await, 1);

        // deleting that note on the device removes it from the index too
        assert!(s.refresh_device_fields(book, &h).await.unwrap());
        assert_eq!(found(s.clone(), "occupation").await, 0);

        // our own annotation
        s.set_annotation(id, Some("the whole book in one line"))
            .await
            .unwrap();
        assert_eq!(found(s.clone(), "whole").await, 1);
        assert_eq!(
            found(s.clone(), "failed").await,
            1,
            "indexing the annotation must not drop the passage"
        );

        // and the book going away takes its highlights with it, by cascade —
        // the delete trigger fires for rows nothing named.
        s.delete_book(book).await.unwrap();
        assert_eq!(found(s.clone(), "failed").await, 0);
    }

    /// `merge_books` moves highlights and drops the collisions. The moved ones
    /// stay findable under the surviving book and the dropped ones leave the
    /// index — neither is something the merge code says out loud, which is the
    /// point.
    #[tokio::test]
    async fn a_merge_leaves_the_index_agreeing_with_the_table() {
        let (s, dst) = seeded().await;
        let src = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko (duplicate)".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        let shared = hl("history has failed us");
        let only_src = NewHighlight {
            text: "a passage the duplicate alone holds".into(),
            ko_datetime: Some("2026-02-02 10:00:00".into()),
            ..shared.clone()
        };
        s.insert_highlight(dst, &shared).await.unwrap();
        s.insert_highlight(src, &shared).await.unwrap();
        s.insert_highlight(src, &only_src).await.unwrap();

        s.merge_books(src, dst).await.unwrap();

        let moved = s
            .search_marks("duplicate alone", Some(SearchSource::Highlight), 10)
            .await
            .unwrap();
        assert_eq!(moved.len(), 1, "a moved highlight stays findable");
        assert_eq!(moved[0].as_highlight().unwrap().book_id, dst);

        let deduped = s
            .search_marks("failed", Some(SearchSource::Highlight), 10)
            .await
            .unwrap();
        assert_eq!(
            deduped.len(),
            1,
            "the dropped copy left the index with the row"
        );
    }
}
