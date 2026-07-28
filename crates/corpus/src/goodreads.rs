//! A Goodreads export, generated.
//!
//! It sits beside the hand-authored files in
//! `crates/engine/tests/fixtures/goodreads/recorded/` and answers a different
//! question. Those pin the *shapes* only a real export has — Excel armour, an
//! embedded newline, CRLF — because a Goodreads CSV is a recorded artifact of
//! another system and generating one from our own understanding of the format
//! would prove only that we agree with ourselves.
//!
//! This one covers *volume and variety*: every shelf, the whole rating range,
//! rereads, multi-shelf rows, missing ISBNs, at a size nobody would hand-write.
//! It is what makes "import the library, import it again, nothing moved" a
//! statement about forty books rather than five.
//!
//! Determinism is the same rule as the rest of the crate: ChaCha8 from a fixed
//! seed (never `StdRng`, whose algorithm may change between `rand` versions), a
//! fixed date epoch, never `now()`. A diff in the output file is a change in
//! this generator and nothing else.
//!
//! And, as everywhere in `crates/corpus`: **no dependency on `readingbuddy`**.
//! The ISBN check digits below are computed here, from the spec, rather than
//! borrowed from `book.rs` — a generator that reused the engine's own
//! normalization would bless a bug in it into every fixture.

use std::path::{Path, PathBuf};

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// Bumped when the shape of the output changes, so a stale committed file is
/// visibly stale rather than subtly wrong.
pub const GENERATOR_VERSION: u32 = 1;

/// 2016-01-01, as days since the unix epoch. Fixed, because `now()` in a
/// fixture generator means the fixture changes every day it is regenerated.
const EPOCH_DAY: i64 = 16_801;

const HEADER: [&str; 24] = [
    "Book Id",
    "Title",
    "Author",
    "Author l-f",
    "Additional Authors",
    "ISBN",
    "ISBN13",
    "My Rating",
    "Average Rating",
    "Publisher",
    "Binding",
    "Number of Pages",
    "Year Published",
    "Original Publication Year",
    "Date Read",
    "Date Added",
    "Bookshelves",
    "Bookshelves with positions",
    "Exclusive Shelf",
    "My Review",
    "Spoiler",
    "Private Notes",
    "Read Count",
    "Owned Copies",
];

const FIRST: [&str; 12] = [
    "Sunja", "Han", "Noa", "Mozasu", "Yumi", "Solomon", "Kyunghee", "Yoseb", "Isak", "Hansu",
    "Etsuko", "Phoebe",
];
const LAST: [&str; 10] = [
    "Baek", "Kim", "Lee", "Park", "Choi", "Jeong", "Kang", "Yun", "Song", "Hwang",
];
const NOUNS: [&str; 14] = [
    "Winter",
    "Salt",
    "Harbour",
    "Pachinko",
    "Vegetarian",
    "Ledger",
    "Orchard",
    "Tide",
    "Quiet",
    "Grammar",
    "Interpreter",
    "Almanac",
    "Border",
    "Weather",
];
/// Leading spaces are part of the data: `, Volume Two` and `: A Memoir` attach
/// directly to the noun and the rest do not.
const QUALIFIERS: [&str; 8] = [
    " of the Long Night",
    " in Translation",
    " and Other Stories",
    ", Volume Two",
    " Revisited",
    ": A Memoir",
    " at the End of the Line",
    " for Beginners",
];
const SHELVES: [&str; 8] = [
    "korean-lit",
    "favourites",
    "sci-fi",
    "owned",
    "book-club",
    "non-fiction",
    "re-read",
    "borrowed",
];
const PUBLISHERS: [&str; 5] = [
    "Grand Central Publishing",
    "Hogarth",
    "Ace",
    "Little, Brown and Company",
    "Verso",
];

pub struct Options {
    pub seed: u64,
    pub books: usize,
}

pub fn default_out() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/goodreads/generated")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("crates/engine/tests/fixtures/goodreads/generated"))
}

pub fn generate(out: &Path, opts: &Options) -> std::io::Result<usize> {
    std::fs::create_dir_all(out)?;
    let mut rng = ChaCha8Rng::seed_from_u64(opts.seed);
    let mut w = csv::WriterBuilder::new()
        // Goodreads writes CRLF, so this does too — a fixture that quietly
        // normalized line endings would stop exercising the one thing a
        // line-oriented reader gets wrong.
        .terminator(csv::Terminator::CRLF)
        .from_writer(Vec::new());
    w.write_record(HEADER)?;

    for i in 0..opts.books {
        let record = book(&mut rng, i);
        w.write_record(&record)?;
    }
    let bytes = w.into_inner().expect("in-memory writer");
    // The armour is written *around* the csv writer, for the same reason the
    // engine strips it outside the csv reader: `="123"` is a bare field with
    // quotes in the middle of it, which is not valid CSV and is exactly what
    // Goodreads emits. Writing it through `write_record` would quote it and
    // the fixture would stop being a recording of their dialect.
    let text = String::from_utf8(bytes)
        .expect("utf-8")
        .replace("\"=\"\"", "=\"")
        .replace("\"\"\"", "\"");
    let path = out.join("library.csv");
    std::fs::write(&path, text)?;
    Ok(opts.books)
}

fn book(rng: &mut ChaCha8Rng, i: usize) -> Vec<String> {
    let title = title(rng);
    let author = format!("{} {}", pick(rng, &FIRST), pick(rng, &LAST));
    let (first, last) = author.split_once(' ').expect("first last");

    // A tenth of the rows carry no ISBN at all, which is what a Goodreads
    // library of ebooks and self-published titles really looks like.
    let has_isbn = below(rng, 10) != 0;
    let isbn13 = has_isbn.then(|| isbn13(rng));
    let isbn10 = isbn13.as_deref().and_then(isbn13_to_10);

    let shelf = match below(rng, 10) {
        0..=5 => "read",
        6..=7 => "to-read",
        8 => "currently-reading",
        _ => "read",
    };
    let read_count = match shelf {
        "read" => 1 + below(rng, 4) as i64 / 3, // mostly 1, sometimes 2
        _ => 0,
    };
    let rating = match shelf {
        "read" => below(rng, 6) as u8, // 0 (unrated) through 5
        _ => 0,
    };
    let added = EPOCH_DAY + below(rng, 2_500) as i64;
    let read = (shelf == "read").then(|| added + 1 + below(rng, 400) as i64);

    let mut shelves: Vec<&str> = Vec::new();
    for _ in 0..below(rng, 3) {
        let s = pick(rng, &SHELVES);
        if !shelves.contains(&s) {
            shelves.push(s);
        }
    }

    let review = if shelf == "read" && below(rng, 3) == 0 {
        format!("Finished it on the train. {title} stayed with me.")
    } else {
        String::new()
    };
    let private = if below(rng, 7) == 0 {
        format!("Lent to {first}.")
    } else {
        String::new()
    };

    vec![
        (1_000_000 + i as u64).to_string(),
        title,
        author.clone(),
        format!("{last}, {first}"),
        String::new(),
        isbn10.map(armour).unwrap_or_else(|| armour(String::new())),
        isbn13
            .clone()
            .map(armour)
            .unwrap_or_else(|| armour(String::new())),
        rating.to_string(),
        format!("{}.{}", 3 + below(rng, 2), below(rng, 100)),
        pick(rng, &PUBLISHERS).to_string(),
        "Paperback".to_string(),
        (120 + below(rng, 600)).to_string(),
        (1980 + below(rng, 45)).to_string(),
        String::new(),
        read.map(date).unwrap_or_default(),
        date(added),
        shelves.join(", "),
        shelves
            .iter()
            .enumerate()
            .map(|(n, s)| format!("{s} (#{})", n + 1))
            .collect::<Vec<_>>()
            .join(", "),
        shelf.to_string(),
        review,
        String::new(),
        private,
        read_count.to_string(),
        below(rng, 2).to_string(),
    ]
}

fn armour(value: String) -> String {
    format!("=\"{value}\"")
}

fn title(rng: &mut ChaCha8Rng) -> String {
    let noun = pick(rng, &NOUNS);
    match below(rng, 3) {
        0 => format!("The {noun}"),
        1 => format!("{noun}{}", pick(rng, &QUALIFIERS)),
        _ => noun.to_string(),
    }
}

/// Days since the unix epoch as `YYYY/MM/DD`, by civil-calendar arithmetic.
///
/// Hand-rolled rather than pulled from `time`, because this crate deliberately
/// keeps its dependency list to what the fixtures genuinely need — and because
/// the algorithm (Howard Hinnant's `civil_from_days`) is exact and short.
fn date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}/{m:02}/{d:02}")
}

/// A syntactically valid ISBN-13 with a real check digit, computed here from
/// the spec rather than borrowed from the engine.
fn isbn13(rng: &mut ChaCha8Rng) -> String {
    let mut digits: Vec<u32> = "978".chars().map(|c| c.to_digit(10).unwrap()).collect();
    for _ in 0..9 {
        digits.push(below(rng, 10));
    }
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { *d } else { 3 * d })
        .sum();
    digits.push((10 - sum % 10) % 10);
    digits
        .iter()
        .map(|d| char::from_digit(*d, 10).unwrap())
        .collect()
}

/// The 978- form's ISBN-10 sibling. `None` for a 979- prefix, which has none —
/// not reachable from [`isbn13`] today, and left honest anyway.
fn isbn13_to_10(isbn13: &str) -> Option<String> {
    let body = isbn13.strip_prefix("978")?;
    let body = &body[..9];
    let sum: u32 = body
        .chars()
        .enumerate()
        .map(|(i, c)| (10 - i as u32) * c.to_digit(10).unwrap())
        .sum();
    let check = (11 - sum % 11) % 11;
    let check = if check == 10 {
        'X'
    } else {
        char::from_digit(check, 10)?
    };
    Some(format!("{body}{check}"))
}

fn below(rng: &mut ChaCha8Rng, n: u32) -> u32 {
    rng.next_u32() % n
}

fn pick<'a>(rng: &mut ChaCha8Rng, from: &[&'a str]) -> &'a str {
    from[below(rng, from.len() as u32) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_digits_are_real() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        for _ in 0..50 {
            let i13 = isbn13(&mut rng);
            let sum: u32 = i13
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    let d = c.to_digit(10).unwrap();
                    if i % 2 == 0 { d } else { 3 * d }
                })
                .sum();
            assert_eq!(sum % 10, 0, "{i13}");

            let i10 = isbn13_to_10(&i13).unwrap();
            let sum: u32 = i10
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    let d = if c == 'X' {
                        10
                    } else {
                        c.to_digit(10).unwrap()
                    };
                    (10 - i as u32) * d
                })
                .sum();
            assert_eq!(sum % 11, 0, "{i10}");
        }
    }

    #[test]
    fn the_calendar_agrees_with_known_days() {
        assert_eq!(date(0), "1970/01/01");
        assert_eq!(date(EPOCH_DAY), "2016/01/01");
        // A leap day, which is where hand-rolled calendars go wrong.
        assert_eq!(date(18_321), "2020/02/29");
    }
}
