//! `readingbuddy enrich` and `readingbuddy set` — item 30.
//!
//! Two halves of one loop. `enrich` asks the providers about a book we already
//! have; `set` is what the user says back when the answer was wrong or when
//! there was not enough to ask with. All the printing lives here; the engine
//! does no terminal I/O.
//!
//! The refusal is the interesting output. `ko pull` and `calibre import` both
//! established the shape — say what it probably is, then name the moves — and
//! the moves here are `set --isbn` (this is the edition) and `set --title` /
//! `--author` (the book is described wrongly, which is *why* nothing matched).
//! Both write through `Source::User`, so a corrected field is then held against
//! every future run of this command, which is the whole point of item 29.

use anyhow::{Result, bail};
use readingbuddy::{
    Book, Engine, EnrichCandidate, EnrichMatch, EnrichOutcome, FieldChange, HeldField, Source,
    series_index_text,
};

use super::resolve_one;

pub async fn enrich(engine: &Engine, selector: &str) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let id = book.id.expect("a stored book has an id");
    let report = engine.enrich_book_from_providers(id).await?;

    for w in &report.warnings {
        println!("warning: {w}");
    }

    let title = book.display_title();
    match &report.outcome {
        EnrichOutcome::Enriched(how) => {
            println!("{title} — looked up again ({})", matched_by(*how));
            if !report.wrote_anything() {
                // Not "nothing happened": the providers answered and agreed with
                // every field we hold. Saying so is the difference between a
                // book that is up to date and one nobody could reach.
                println!("    everything they know, you already had.");
            }
            for c in &report.filled {
                println!("{}", filled_line(c));
            }
            if let Some(path) = &report.cover {
                println!("    {:<16}{}", "cover", path.display());
            }
        }
        EnrichOutcome::Refused { candidates } => {
            println!("{title}: nothing written — nothing came back that is certainly this book.");
            for c in candidates {
                print_candidate(c);
            }
            print_next_moves(selector, candidates);
        }
        EnrichOutcome::NothingFound => {
            println!("{title}: no provider knows this book.");
            if book.isbn_13.is_some() || book.isbn_10.is_some() {
                // An ISBN is an identity, so this is an answer about *this
                // edition* and not about the work. There is no fall-through to a
                // title search, deliberately: it would answer a different
                // question and write another edition's page count over this one.
                println!("    that is about this ISBN, not about the book.");
                println!("    wrong ISBN? readingbuddy set {selector} --isbn <isbn>");
            }
        }
        EnrichOutcome::NoAnswer => {
            // Deliberately not "no provider knows this book": one of them never
            // spoke. A dead network must not read as a fact about the book.
            println!("{title}: no answer — the providers above did not come back.");
            println!("    nothing was written. try again in a moment.");
        }
        EnrichOutcome::Unaskable => {
            println!("{title}: nothing to ask with — no ISBN, and no title to match on.");
            println!("    readingbuddy set {selector} --title \"...\" --author \"...\"");
        }
    }

    for h in &report.held {
        print_held(selector, h);
    }
    Ok(())
}

fn matched_by(how: EnrichMatch) -> String {
    match how {
        EnrichMatch::Isbn => "by isbn".to_string(),
        EnrichMatch::Title(score) => format!("by title, {:.0}%", score * 100.0),
    }
}

fn filled_line(c: &FieldChange) -> String {
    let source = c.source.map(source_name).unwrap_or("unattributed");
    let value = clip(&c.after);
    match &c.before {
        // The old value is shown only when there was one. For the books this
        // command exists for there almost never is, and "(was: none)" on every
        // line would bury the case where something was actually replaced.
        Some(before) => format!(
            "    {:<16}{value}  [{source}, was {}]",
            c.field,
            clip(before)
        ),
        None => format!("    {:<16}{value}  [{source}]", c.field),
    }
}

/// A held-back field, with both values and the way to change your mind.
///
/// Silence here is the failure this reports its way out of: a field not updated
/// because the user owns it looks exactly like a field the provider had nothing
/// for, and the second needs no explanation while the first does.
fn print_held(selector: &str, h: &HeldField) {
    let by = h.offered_by.map(source_name).unwrap_or("a provider");
    println!(
        "    held back: {} — {by} says {}, yours says {}",
        h.field,
        clip(&h.offered),
        h.kept.as_deref().map(clip).unwrap_or_else(|| "—".into()),
    );
    match flag_of(h.field) {
        Some(flag) => {
            println!("               yours stays. readingbuddy set {selector} --{flag} …")
        }
        // No flag reaches this column, so there is no move to offer. `set` is
        // deliberately narrower than the merge table, and naming a flag that
        // does not exist would be a dead end dressed as a next step.
        None => println!("               yours stays."),
    }
}

fn print_candidate(c: &EnrichCandidate) {
    let mut detail: Vec<String> = Vec::new();
    if !c.book.authors.is_empty() {
        detail.push(c.book.authors.join(", "));
    }
    if let Some(year) = c.book.publish_year {
        detail.push(year.to_string());
    }
    if let Some(publisher) = &c.book.publisher {
        detail.push(publisher.clone());
    }
    let sources: Vec<String> = c.sources.iter().map(|s| s.to_string()).collect();
    println!(
        "    maybe: {} ({}) {:.0}% [{}]",
        c.book.display_title(),
        detail.join(", "),
        c.score * 100.0,
        sources.join(", ")
    );
    match c.book.isbn_13.as_deref().or(c.book.isbn_10.as_deref()) {
        Some(isbn) => println!("           isbn {isbn}"),
        // Without an ISBN there is no durable name for this record, and the next
        // move below cannot name it. Saying so beats offering a move that does
        // not exist.
        None => println!("           no isbn — nothing here can name this edition"),
    }
}

/// The two next moves, and why neither is `--accept 2`.
///
/// An index would point into a candidate list produced by a *previous* provider
/// search, which the next run cannot reproduce — `ko link <path> <book>` and
/// `calibre import --new` both name something durable, and this follows them
/// rather than the number on the screen.
fn print_next_moves(selector: &str, candidates: &[EnrichCandidate]) {
    if let Some(isbn) = candidates
        .iter()
        .find_map(|c| c.book.isbn_13.as_deref().or(c.book.isbn_10.as_deref()))
    {
        println!("    it is one of these : readingbuddy set {selector} --isbn {isbn}");
        println!("                         then run enrich again");
    }
    println!("    or it is described wrongly here:");
    println!(
        "                         readingbuddy set {selector} --title \"...\" --author \"...\""
    );
}

fn source_name(s: Source) -> &'static str {
    s.as_str()
}

/// The `set` flag that owns a column, for the "change your mind" line.
///
/// **Exhaustive and `None` for the rest, not a column name with its underscores
/// swapped.** That fallback was the first cut and it was wrong for
/// `translators`, whose flag is the singular `--translator`; the test below
/// found it. A guessed flag is a next move that fails with a clap error, which
/// is worse than offering none — and `set` is deliberately narrower than
/// `MERGE_RULES`, so some columns genuinely have no flag.
fn flag_of(field: &str) -> Option<&'static str> {
    Some(match field {
        "title" => "title",
        "authors" => "author",
        "translators" => "translator",
        "publisher" => "publisher",
        "publish_year" => "year",
        "language" => "language",
        "page_count" => "page-count",
        "description" => "description",
        "isbn_10" | "isbn_13" => "isbn",
        "subjects" => "subject",
        "series" => "series",
        "series_index" => "series-index",
        _ => return None,
    })
}

fn clip(s: &str) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= 58 {
        return flat;
    }
    flat.chars().take(57).collect::<String>() + "…"
}

// ---- set -------------------------------------------------------------------

/// What `readingbuddy set` can write.
///
/// Deliberately the fields a *provider* also writes and a reader would correct,
/// and no others: this is the counterweight to enrichment, not a general row
/// editor. Reading state has `progress`, notes have `note`, and a rating goes
/// through the explicit scale.
#[derive(clap::Args, Debug, Default)]
pub struct SetArgs {
    /// Book selector: id, ISBN, or title fragment
    pub book: String,
    #[arg(long)]
    pub title: Option<String>,
    /// Repeatable; replaces the author list
    #[arg(long = "author")]
    pub authors: Vec<String>,
    /// Repeatable; replaces the translator list
    #[arg(long = "translator")]
    pub translators: Vec<String>,
    #[arg(long)]
    pub publisher: Option<String>,
    #[arg(long)]
    pub year: Option<i64>,
    /// BCP-47 or MARC code, as the providers give it
    #[arg(long)]
    pub language: Option<String>,
    #[arg(long)]
    pub isbn: Option<String>,
    #[arg(long = "page-count")]
    pub page_count: Option<i64>,
    #[arg(long)]
    pub description: Option<String>,
    /// Repeatable; replaces the subject set. A provider's subjects, not a shelf
    /// — `docs/decisions.md` defers collections, and this is not one.
    #[arg(long = "subject")]
    pub subjects: Vec<String>,
    #[arg(long)]
    pub series: Option<String>,
    /// Fractional is fine — novellas are 0.5. Refused when the book is in no
    /// series and `--series` does not name one: an index alone is a number
    /// under no name, which is the incoherent half-pair `Rule::pair` exists to
    /// prevent.
    #[arg(long = "series-index")]
    pub series_index: Option<f64>,
}

/// Record what the user says, and that it was the user who said it.
///
/// Every field written here is stamped `user` in `field_provenance` and is from
/// then on **not overwritten by any provider merge**. That is the point of the
/// command rather than a side effect of it, and it is why this is the next move
/// `enrich`'s refusal offers.
pub async fn set(engine: &Engine, args: &SetArgs) -> Result<()> {
    let book = resolve_one(engine, &args.book).await?;
    let id = book.id.expect("a stored book has an id");

    let mut fields = Book {
        title: args.title.clone(),
        authors: args.authors.clone(),
        translators: args.translators.clone(),
        publisher: args.publisher.clone(),
        publish_year: args.year,
        language: args.language.clone(),
        page_count: args.page_count,
        description: args.description.clone(),
        subjects: args.subjects.clone(),
        series: args.series.clone(),
        series_index: args.series_index,
        ..Default::default()
    };
    if let Some(raw) = &args.isbn {
        // Every ISBN entering the system goes through `normalize_isbn`, no
        // exceptions — and a typed one is exactly where a bad check digit shows
        // up. Both columns are set from the one flag: which of them the string
        // is, is not the user's problem.
        let isbn = readingbuddy::normalize_isbn(raw)
            .ok_or_else(|| anyhow::anyhow!("{raw:?} is not a valid ISBN"))?;
        if isbn.len() == 13 {
            fields.isbn_13 = Some(isbn);
        } else {
            fields.isbn_10 = Some(isbn);
        }
    }

    if is_empty(&fields) {
        bail!("nothing to set — name at least one field (--help lists them)");
    }
    if let Some(orphan) = orphan_index(&fields, &book) {
        bail!(
            "{} is in no series — name it too: --series \"…\" --series-index {}",
            book.display_title(),
            series_index_text(orphan)
        );
    }

    let after = engine.set_book_fields(id, &fields).await?;
    println!("{} — set, and recorded as yours:", after.display_title());
    for (field, value) in changed(&fields) {
        println!("    {field:<16}{}", clip(&value));
    }
    println!("    no provider merge will overwrite these.");
    // `set_book_fields` sets and cannot clear, so renaming a series leaves the
    // old number behind — silently, and reading as a fact about the new one.
    // Saying so beats a `#2` that belongs to a different shelf.
    if fields.series.is_some() && fields.series_index.is_none() && after.series_index.is_some() {
        println!(
            "    note: still numbered {} — --series-index changes it.",
            after
                .series_index
                .map(series_index_text)
                .expect("checked just above")
        );
    }
    Ok(())
}

/// Which fields the user actually named, for the echo.
///
/// Read off the *request* rather than off the stored row: the row cannot say
/// which of its values arrived a second ago, and a field set to what it already
/// held is still a field the user has now claimed.
fn changed(fields: &Book) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let mut push = |name, value: Option<String>| {
        if let Some(v) = value {
            out.push((name, v));
        }
    };
    push("title", fields.title.clone());
    push(
        "authors",
        (!fields.authors.is_empty()).then(|| fields.authors.join(", ")),
    );
    push(
        "translators",
        (!fields.translators.is_empty()).then(|| fields.translators.join(", ")),
    );
    push("publisher", fields.publisher.clone());
    push("publish_year", fields.publish_year.map(|y| y.to_string()));
    push("language", fields.language.clone());
    push("isbn_10", fields.isbn_10.clone());
    push("isbn_13", fields.isbn_13.clone());
    push("page_count", fields.page_count.map(|n| n.to_string()));
    push("description", fields.description.clone());
    push(
        "subjects",
        (!fields.subjects.is_empty()).then(|| fields.subjects.join(", ")),
    );
    push("series", fields.series.clone());
    push("series_index", fields.series_index.map(series_index_text));
    out
}

fn is_empty(fields: &Book) -> bool {
    changed(fields).is_empty()
}

/// An index with no series to belong to, if that is what was asked for.
///
/// Half a pair is not a correction — it is a number under no name, which is the
/// incoherent row `Rule::pair` exists to prevent one layer down. Checked against
/// the **stored** series as well as the flag, because numbering a book whose
/// series is already recorded is the ordinary use of `--series-index` and
/// demanding the name back would be a refusal with nothing wrong behind it.
fn orphan_index(fields: &Book, stored: &Book) -> Option<f64> {
    fields
        .series_index
        .filter(|_| fields.series.is_none() && stored.series.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(field: &'static str, before: Option<&str>, after: &str) -> FieldChange {
        FieldChange {
            field,
            source: Some(Source::OpenLibrary),
            before: before.map(str::to_string),
            after: after.to_string(),
        }
    }

    /// A filled field names its source, because "where did this number come
    /// from" is the question item 29 exists to answer and this is the one place
    /// the answer is fresh.
    #[test]
    fn a_filled_field_says_who_supplied_it() {
        let line = filled_line(&change("page_count", None, "490"));
        assert!(line.contains("page_count"), "{line}");
        assert!(line.contains("490"), "{line}");
        assert!(line.contains("openlibrary"), "{line}");
        assert!(!line.contains("was"), "no old value to report: {line}");
    }

    #[test]
    fn a_replaced_field_shows_what_it_replaced() {
        let line = filled_line(&change("page_count", Some("100"), "490"));
        assert!(line.contains("was 100"), "{line}");
    }

    /// Long prose is clipped to one line. A description arrives with newlines in
    /// it and a raw one would break the alignment of everything after it.
    #[test]
    fn a_description_is_flattened_and_clipped() {
        let prose = "A sweeping saga\nof four generations\n".to_string() + &"x".repeat(200);
        let out = clip(&prose);
        assert!(!out.contains('\n'));
        assert_eq!(out.chars().count(), 58);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn short_text_is_left_alone() {
        assert_eq!(clip("Pachinko"), "Pachinko");
    }

    /// The flag names in the "change your mind" line have to be flags `set`
    /// actually takes, or the next move is a clap error.
    #[test]
    fn every_held_field_names_a_real_flag() {
        use clap::CommandFactory;
        #[derive(clap::Parser)]
        struct Wrapper {
            #[command(flatten)]
            set: SetArgs,
        }
        let cmd = Wrapper::command();
        let flags: Vec<String> = cmd
            .get_arguments()
            .filter_map(|a| a.get_long().map(str::to_string))
            .collect();
        // Every column `field_provenance` can hold a user claim on that `set`
        // is meant to reach.
        for field in [
            "title",
            "authors",
            "publisher",
            "publish_year",
            "language",
            "page_count",
            "description",
            "isbn_10",
            "isbn_13",
            "translators",
            "subjects",
            "series",
            "series_index",
        ] {
            let flag = flag_of(field).unwrap_or_else(|| panic!("{field} has no `set` flag"));
            assert!(
                flags.contains(&flag.to_string()),
                "held field {field} points at --{flag}, which `set` does not take: {flags:?}"
            );
        }
        // A column `set` does not reach offers no move rather than a wrong one.
        assert_eq!(flag_of("cover_url"), None);
    }

    /// A series index alone is a number under no name — unless the book is
    /// already in a series, which is the ordinary way this flag gets used.
    #[test]
    fn an_index_is_refused_only_when_no_series_names_it() {
        let numbered = |n: f64| Book {
            series_index: Some(n),
            ..Default::default()
        };
        let in_dune = Book {
            series: Some("Dune".into()),
            ..Default::default()
        };
        assert_eq!(
            orphan_index(&numbered(2.0), &Book::default()),
            Some(2.0),
            "nothing names this number"
        );
        // Renumbering a book whose series is on the row already.
        assert_eq!(orphan_index(&numbered(2.5), &in_dune), None);
        // Both halves in one command.
        assert_eq!(
            orphan_index(
                &Book {
                    series: Some("Dune".into()),
                    ..numbered(2.0)
                },
                &Book::default()
            ),
            None
        );
        // A name with no number is not half a pair — an unnumbered series is an
        // ordinary thing for a provider to know.
        assert_eq!(orphan_index(&in_dune, &Book::default()), None);
    }

    /// The echo names every field `set` can write, including the three item 32
    /// added — a field written and not echoed is a silent write, which is the
    /// state this command exists to be the opposite of.
    #[test]
    fn the_echo_names_the_pair_and_the_subjects() {
        let fields = Book {
            subjects: vec!["Fiction / Literary".into()],
            series: Some("Dune".into()),
            series_index: Some(2.0),
            ..Default::default()
        };
        let named: Vec<&str> = changed(&fields).iter().map(|(f, _)| *f).collect();
        assert_eq!(named, ["subjects", "series", "series_index"]);
        // Whole numbers print as whole numbers, not `2.0`.
        assert_eq!(changed(&fields)[2].1, "2");
    }

    /// `set` with no field named must refuse rather than write an empty record
    /// and claim every column for the user.
    #[test]
    fn set_with_nothing_named_is_refused() {
        assert!(is_empty(&Book::default()));
        assert!(!is_empty(&Book {
            page_count: Some(1),
            ..Default::default()
        }));
    }
}
