//! The KOReader `statistics.sqlite3` fixture (item 31).
//!
//! # Why this emits SQL text rather than a `.sqlite3` file
//!
//! The obvious move is to link a SQLite writer here and commit the binary the
//! device would have. Three arguments against it, and the last one decides:
//!
//! * Both existing tier-1 generators emit **text** — `gen-synthetic` writes
//!   Lua, `gen-goodreads` writes CSV — and both are reviewed as text in a diff.
//!   A committed `.sqlite3` would be the only fixture in the repo whose
//!   contents no reviewer can see and no `git diff` can describe.
//! * A database file's bytes depend on the writer's SQLite version, so
//!   "regenerate and confirm nothing moved" stops being a check that means
//!   anything, while a `.sql` script is byte-stable for a fixed seed for ever.
//! * It would cost a new dependency for no coverage. What the engine must be
//!   tested against is the **schema and the data**, and the schema is a string
//!   either way. The engine's own test materialises this script into a real
//!   database, and the WAL-copy path is exercised there, where a real
//!   write-ahead log can actually be produced.
//!
//! The rule that matters — `crates/corpus` does not depend on `readingbuddy` —
//! is kept: nothing here parses anything the engine parses, and the expected
//! totals below are accumulated **as the rows are written**, never by reading
//! them back. That is what makes `expected.json` an independent oracle rather
//! than a recording of our own aggregate query.
//!
//! # The schema
//!
//! Verbatim from `plugins/statistics.koplugin/main.lua`, `koreader/koreader@master`.
//! The engine's `ko_statistics.rs` quotes the same DDL in its module doc; this
//! is the copy that is *executed*, so the two must agree and the engine's tests
//! are what notice if they stop.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// Bump when the emitted fixture changes shape, so a stale checkout is
/// diagnosable rather than merely different.
pub const GENERATOR_VERSION: u32 = 1;

/// `DB_SCHEMA_VERSION` in the statistics plugin, stamped into
/// `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 20221111;

/// 2026-01-05, as days since the unix epoch. A fixed epoch, never `now()` —
/// the corpus determinism rule.
const BASE_DAY: i64 = 20458;

/// The plugin's own DDL, quoted exactly. Reformatted whitespace only.
const SCHEMA: &str = "\
PRAGMA user_version=20221111;

CREATE TABLE IF NOT EXISTS book
    (
        id integer PRIMARY KEY autoincrement,
        title text,
        authors text,
        notes      integer,
        last_open  integer,
        highlights integer,
        pages      integer,
        series text,
        language text,
        md5 text,
        total_read_time  integer,
        total_read_pages integer
    );

CREATE TABLE IF NOT EXISTS page_stat_data
    (
        id_book     integer,
        page        integer NOT NULL DEFAULT 0,
        start_time  integer NOT NULL DEFAULT 0,
        duration    integer NOT NULL DEFAULT 0,
        total_pages integer NOT NULL DEFAULT 0,
        UNIQUE (id_book, page, start_time),
        FOREIGN KEY(id_book) REFERENCES book(id)
    );

CREATE TABLE IF NOT EXISTS numbers
    (
        number INTEGER PRIMARY KEY
    );

CREATE UNIQUE INDEX IF NOT EXISTS book_title_authors_md5 ON book(title, authors, md5);
CREATE INDEX IF NOT EXISTS page_stat_data_start_time ON page_stat_data(start_time);
";

pub struct Options {
    pub seed: u64,
}

/// One book as the fixture describes it, for the engine test to seed from.
struct BookSpec {
    title: &'static str,
    /// `None` models a statistics row the plugin wrote with no md5 — nothing to
    /// join on.
    md5: Option<&'static str>,
    /// Whether the engine test should create a library book and link this md5.
    /// A `false` here is the ordinary "the device holds a book we never
    /// imported" case.
    in_library: bool,
    /// `(day offset from BASE_DAY, [(page, seconds)])`.
    days: &'static [(i64, &'static [(i64, i64)])],
}

/// The fixture, written out rather than randomised, because every row here is
/// a *case*: the interesting inputs are the awkward ones and a PRNG does not
/// reliably produce them. The seed drives the filler rows only.
const BOOKS: &[BookSpec] = &[
    // An ordinary, well-behaved book: three days, several pages each.
    BookSpec {
        title: "Pachinko",
        md5: Some("8cb32bca81b36ca0816851073e5661d3"),
        in_library: true,
        days: &[
            (0, &[(10, 300), (11, 240), (12, 180)]),
            // A page revisited later the same day: two rows, one page turned.
            (1, &[(13, 600), (13, 120), (14, 90)]),
            (4, &[(15, 3600)]),
        ],
    },
    // The measured-zero case: a real session of nineteen seconds. It must
    // record `Some(0)` minutes, never `None` — the device is saying something.
    BookSpec {
        title: "Brief Encounter",
        md5: Some("a5b01da92a68bbbb6d88c12483cf3b56"),
        in_library: true,
        days: &[(2, &[(1, 19)])],
    },
    // On the device, never imported here. Ordinary, and reported.
    BookSpec {
        title: "A Book We Never Imported",
        md5: Some("25dc3d7e5bd746db64267cff902d3edd"),
        in_library: false,
        days: &[(0, &[(3, 900)])],
    },
    // A statistics row carrying no md5 at all.
    BookSpec {
        title: "No Checksum Here",
        md5: None,
        in_library: false,
        days: &[(1, &[(2, 450)])],
    },
];

/// What the fixture *means*, accumulated while writing it.
#[derive(serde::Serialize)]
struct Expected {
    generator_version: u32,
    schema_version: i64,
    books: Vec<ExpectedBook>,
}

#[derive(serde::Serialize)]
struct ExpectedBook {
    title: String,
    md5: Option<String>,
    in_library: bool,
    days: Vec<ExpectedDay>,
}

#[derive(serde::Serialize)]
struct ExpectedDay {
    day: String,
    seconds: i64,
    /// Distinct pages touched that day.
    pages: i64,
    /// Seconds rounded to the nearest minute — what `reading_events.minutes`
    /// must hold. Computed here rather than in the test so the rounding rule is
    /// asserted against an independently-stated number.
    minutes: i64,
}

pub fn generate(out: &Path, opts: &Options) -> std::io::Result<usize> {
    std::fs::create_dir_all(out)?;
    let mut rng = ChaCha8Rng::seed_from_u64(opts.seed);

    let mut sql = String::from(SCHEMA);
    sql.push('\n');
    let mut expected = Expected {
        generator_version: GENERATOR_VERSION,
        schema_version: SCHEMA_VERSION,
        books: Vec::new(),
    };

    for (i, spec) in BOOKS.iter().enumerate() {
        let id = i as i64 + 1;
        let md5_sql = match spec.md5 {
            Some(m) => format!("'{m}'"),
            None => "NULL".to_string(),
        };
        sql.push_str(&format!(
            "INSERT INTO book (id, title, authors, md5, pages, language) \
             VALUES ({id}, '{}', 'A. Writer', {md5_sql}, 300, 'en');\n",
            spec.title.replace('\'', "''")
        ));

        // Accumulated as we emit, never read back.
        let mut per_day: BTreeMap<i64, (i64, std::collections::BTreeSet<i64>)> = BTreeMap::new();

        for (day_off, pages) in spec.days {
            let day_start = (BASE_DAY + day_off) * 86_400;
            for (n, (page, seconds)) in pages.iter().enumerate() {
                // A deterministic time-of-day, so two rows for one page differ
                // in `start_time` and the UNIQUE constraint is satisfied the
                // way a real device satisfies it.
                let start = day_start + 3_600 + (n as i64 * 907) + (rng.next_u32() % 60) as i64;
                sql.push_str(&format!(
                    "INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) \
                     VALUES ({id}, {page}, {start}, {seconds}, 300);\n"
                ));
                let e = per_day.entry(*day_off).or_default();
                e.0 += seconds;
                e.1.insert(*page);
            }
        }

        // The dead-clock row: `start_time` 0 is the column's declared default,
        // and taken at face value it is a day in 1970 that nobody read. Emitted
        // for the first book only, and deliberately absent from `expected`.
        if i == 0 {
            sql.push_str(&format!(
                "INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) \
                 VALUES ({id}, 99, 0, 500, 300);\n"
            ));
        }

        expected.books.push(ExpectedBook {
            title: spec.title.to_string(),
            md5: spec.md5.map(str::to_string),
            in_library: spec.in_library,
            days: per_day
                .into_iter()
                .map(|(off, (seconds, pages))| ExpectedDay {
                    day: ymd(BASE_DAY + off),
                    seconds,
                    pages: pages.len() as i64,
                    minutes: (seconds + 30) / 60,
                })
                .collect(),
        });
    }

    let mut f = std::fs::File::create(out.join("statistics.sql"))?;
    f.write_all(sql.as_bytes())?;

    let mut e = std::fs::File::create(out.join("expected.json"))?;
    e.write_all(serde_json::to_string_pretty(&expected)?.as_bytes())?;
    e.write_all(b"\n")?;

    Ok(BOOKS.len())
}

/// `YYYY-MM-DD` from days since the unix epoch.
///
/// Hinnant's civil-from-days, written out rather than pulled in: this crate has
/// no date dependency and deliberately shares no code with the engine, which is
/// the whole reason its output can be trusted as an oracle.
fn ymd(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn default_out() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("engine/tests/fixtures/koreader/statistics")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_agrees_with_known_dates() {
        assert_eq!(ymd(0), "1970-01-01");
        assert_eq!(ymd(BASE_DAY), "2026-01-05");
        assert_eq!(ymd(BASE_DAY + 1), "2026-01-06");
        // A leap day, which is where a hand-rolled calendar goes wrong.
        assert_eq!(ymd(19_782), "2024-02-29");
    }
}
