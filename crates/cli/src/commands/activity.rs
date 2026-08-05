//! `readingbuddy activity` — the reading log, item 21's first frontend.
//!
//! Items 21 and 31 built `reading_events` and the two aggregates over it, and
//! nothing could see any of it. This is that surface, and one rule shapes every
//! line of it: **absence is not zero**. A library that arrived as a Goodreads
//! CSV has no minutes at all, and a screen that prints `0 minutes` has told its
//! reader something false about their own reading. So an unmeasured column is
//! `—`, never a number, and the phrase "not measured" appears in place of a
//! total rather than beside one.
//!
//! What is deliberately absent: any count of what the user has *not* done. No
//! streak, no gap, no "you read on 4 of 30 days". `docs/decisions.md` bans
//! task-completion framing by name, and a period report is exactly where it
//! creeps in — so the days with activity are named and the ones without are
//! simply not there.

use anyhow::Result;
use readingbuddy::{ActivitySummary, Confidence, DayActivity, DayRange, Engine, ReadingEvent};
use time::{Duration, OffsetDateTime, macros::format_description};

use super::resolve_one;

pub struct Args {
    pub book: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub days: bool,
    pub refill: bool,
}

/// The default window. Thirty days is a month of reading rather than a calendar
/// month, which is what makes it the same question whenever it is asked.
const DEFAULT_SPAN_DAYS: i64 = 30;

pub async fn run(engine: &Engine, args: Args) -> Result<()> {
    if args.refill {
        let report = engine.refill_reading_events().await?;
        println!(
            "rebuilt from what was already here: {} new, {} changed",
            report.inserted(),
            report.updated()
        );
    }

    if let Some(selector) = &args.book {
        return one_book(engine, selector).await;
    }

    let range = requested_range(args.from.as_deref(), args.to.as_deref())?;
    let summary = engine.activity_summary(&range).await?;
    print!("{}", summary_text(&summary));

    if summary.activity_days == 0 && !args.refill {
        // The log is filled by nothing automatically — deliberately, so it is
        // never a side effect of whichever importer ran last. An empty period is
        // therefore ambiguous between "nothing happened" and "nobody has built
        // it yet", and naming the move is what stops this being a dead end.
        println!("    nothing recorded here yet.");
        println!("    readingbuddy activity --refill  builds it from what you already have");
        return Ok(());
    }
    if args.days {
        for d in engine.activity_by_day(&range).await? {
            println!("{}", day_line(&d));
        }
    }
    Ok(())
}

/// One book's log, day by day. Not range-filtered: a book's whole history is
/// the interesting length, and it is bounded by how long the book took.
async fn one_book(engine: &Engine, selector: &str) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let id = book.id.expect("a stored book has an id");
    let events = engine.reading_events(id).await?;

    if events.is_empty() {
        println!("{}: no days recorded.", book.display_title());
        println!("    readingbuddy activity --refill  builds it from what you already have");
        return Ok(());
    }
    println!("{} — {} days", book.display_title(), day_count(&events));
    for e in &events {
        println!("{}", event_line(e));
    }
    Ok(())
}

/// Distinct days, not rows: two sources speaking about one day is one day of
/// reading, and counting rows would inflate every book the device *and* the
/// vault know about.
fn day_count(events: &[ReadingEvent]) -> usize {
    let mut days: Vec<&str> = events.iter().map(|e| e.day.as_str()).collect();
    days.sort_unstable();
    days.dedup();
    days.len()
}

/// One row of one book's log.
///
/// The source and the confidence are both printed, because they answer
/// different questions: *who says so* and *how much they are claiming*. An
/// inferred day from a highlight stamp means you were in the book; it does not
/// mean anyone measured anything, and a row that looked identical to a measured
/// one would quietly turn evidence into a number.
fn event_line(e: &ReadingEvent) -> String {
    format!(
        "  {}  {:>7}  {:>6}  {:<9} {}",
        e.day,
        measured(e.minutes, "min"),
        measured(e.pages, "pp"),
        e.source,
        match e.confidence {
            Confidence::Measured => "measured",
            Confidence::Inferred => "inferred",
        }
    )
}

fn day_line(d: &DayActivity) -> String {
    format!(
        "  {}  {:>7}  {:>6}  {} {}",
        d.day,
        measured(d.minutes, "min"),
        measured(d.pages, "pp"),
        d.books,
        if d.books == 1 { "book" } else { "books" }
    )
}

/// Built as text and returned rather than printed, following `render.rs`: a
/// block this shape is worth asserting on, and a function that only prints
/// cannot be.
fn summary_text(s: &ActivitySummary) -> String {
    let mut out = format!("{} … {}\n", s.range.from(), s.range.to());
    let mut line = |label: &str, value: String| out.push_str(&format!("  {label:<14} {value}\n"));
    line("days read", s.activity_days.to_string());
    line("books finished", s.books_finished.to_string());
    line("notes", s.notes_created.to_string());
    line("links", s.links_created.to_string());
    // Said in words rather than as `—`, because a total is where a reader most
    // expects a number and most easily reads a dash as one.
    line("minutes", total(s.minutes));
    line("pages", total(s.pages));
    out
}

/// A period total, or the fact that nobody measured one.
fn total(n: Option<i64>) -> String {
    n.map(|n| n.to_string())
        .unwrap_or_else(|| "not measured".into())
}

/// A measured column, or a dash. **Never `0`** — see the module doc.
fn measured(n: Option<i64>, unit: &str) -> String {
    match n {
        Some(n) => format!("{n} {unit}"),
        None => "—".into(),
    }
}

/// The window to report on, from what the user asked for.
///
/// Defaults are computed here rather than in the engine because "today" is a
/// property of the machine the command is run on, and `DayRange` refuses an
/// inverted or malformed span for both of us.
fn requested_range(from: Option<&str>, to: Option<&str>) -> Result<DayRange> {
    let today = OffsetDateTime::now_utc();
    let fmt = format_description!("[year]-[month]-[day]");
    let to = match to {
        Some(t) => t.to_string(),
        None => today.format(fmt)?,
    };
    let from = match from {
        Some(f) => f.to_string(),
        None => (today - Duration::days(DEFAULT_SPAN_DAYS - 1)).format(fmt)?,
    };
    Ok(DayRange::new(&from, &to)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(minutes: Option<i64>, confidence: Confidence) -> ReadingEvent {
        ReadingEvent {
            book_id: 1,
            reading_id: None,
            day: "2026-08-01".into(),
            minutes,
            pages: None,
            source: "koreader".into(),
            confidence,
            created_at: 0,
        }
    }

    /// **An unmeasured column is a dash, and a measured near-nothing is a
    /// zero.** The two are different facts and the whole activity log is built
    /// on being able to tell them apart; one `unwrap_or(0)` here would erase the
    /// distinction at the last possible moment, where nothing downstream could
    /// notice.
    #[test]
    fn nothing_measured_is_never_printed_as_zero() {
        assert_eq!(measured(None, "min"), "—");
        assert_eq!(measured(Some(0), "min"), "0 min");
        assert_eq!(measured(Some(42), "min"), "42 min");

        let line = event_line(&event(None, Confidence::Inferred));
        assert!(line.contains('—'), "{line}");
        assert!(!line.contains(" 0 "), "{line}");
    }

    /// A summary with no minutes says so in words. `0` is the answer this
    /// command must never give a reader whose library came from a CSV.
    #[test]
    fn a_period_nobody_measured_says_so_rather_than_reporting_zero() {
        let s = ActivitySummary {
            range: DayRange::new("2026-01-01", "2026-01-31").unwrap(),
            books_finished: 0,
            activity_days: 3,
            notes_created: 2,
            links_created: 1,
            minutes: None,
            pages: None,
        };
        let out = summary_text(&s);
        assert!(out.contains("minutes        not measured"), "{out}");
        assert!(out.contains("pages          not measured"), "{out}");
        // A count the engine originates: zero here is knowable, and is a number.
        assert!(out.contains("books finished 0"), "{out}");
        // …and nothing anywhere counts what was *not* done, which is the framing
        // `docs/decisions.md` bans and a period report is where it creeps in.
        for banned in ["streak", "goal", "missed", "of 31", "remaining"] {
            assert!(!out.contains(banned), "{banned} in: {out}");
        }
        assert_eq!(total(Some(0)), "0");
    }
}
