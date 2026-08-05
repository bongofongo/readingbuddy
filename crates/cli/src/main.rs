mod commands;
mod config_file;
mod logging;
mod prompt;
mod render;
mod repl;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use readingbuddy::{Engine, EngineConfig};

#[derive(Parser)]
#[command(
    name = "readingbuddy",
    version,
    about = "Reading companion: search books, track progress, keep notes, import KOReader highlights"
)]
struct Cli {
    /// Data directory root (default: READINGBUDDY_DATA_DIR env or current dir)
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    /// Increase log verbosity: -v info, -vv debug, -vvv trace (to stderr).
    /// RUST_LOG overrides this.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Silence logging entirely.
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    quiet: bool,

    /// Panic on purpose, to verify the crash hook. Hidden: a test affordance.
    #[arg(long, global = true, hide = true)]
    panic_now: bool,

    /// Google Books API key for this invocation (overrides the stored one;
    /// prefer `readingbuddy config set google-api-key` for persistence)
    #[arg(
        long,
        global = true,
        env = "GOOGLE_BOOKS_API_KEY",
        hide_env_values = true
    )]
    google_api_key: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Search OpenLibrary + Google Books (fielded, merged, ranked)
    Search(commands::search::SearchArgs),
    /// Add a book directly by ISBN
    Add {
        isbn: String,
        /// Skip downloading the cover image
        #[arg(long)]
        no_cover: bool,
    },
    /// Import a local .epub (ISBN lookup + embedded cover)
    Epub { path: PathBuf },
    /// List the library
    List {
        #[arg(long, default_value_t = 20)]
        limit: i64,
        /// last-modified | title | progress
        #[arg(long, default_value = "last-modified")]
        sort: String,
    },
    /// Show one book (selector: id, ISBN, or title fragment)
    Show { book: String },
    /// The chapter list, read from the epub we own (item 32)
    Toc { book: String },
    /// What a period of reading held: days, minutes where measured, notes (item 21)
    Activity {
        /// One book's day-by-day log instead of the whole library's period
        #[arg(long)]
        book: Option<String>,
        /// YYYY-MM-DD, inclusive. Defaults to the last 30 days
        #[arg(long)]
        from: Option<String>,
        /// YYYY-MM-DD, inclusive. Defaults to today
        #[arg(long)]
        to: Option<String>,
        /// List the days rather than only counting them
        #[arg(long)]
        days: bool,
        /// Rebuild the log from what is already stored, then show it
        #[arg(long)]
        refill: bool,
    },
    /// Ask the providers about a book you already have (item 30)
    Enrich { book: String },
    /// Correct a book's metadata, and record that you are the one who said so
    Set(commands::enrich::SetArgs),
    /// Remove a book and its cover image
    Rm {
        book: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Update reading progress
    Progress {
        book: String,
        /// Current page number
        page: Option<i64>,
        /// Mark the book finished
        #[arg(long)]
        finished: bool,
        /// Start reading it again: close the open reading and open a new one
        #[arg(long)]
        reread: bool,
    },
    /// Write a note (opens $EDITOR when TEXT is omitted)
    Note {
        /// Note body; omit to compose in $EDITOR
        text: Option<String>,
        /// Attach to a book (id, ISBN, or title fragment)
        #[arg(long)]
        book: Option<String>,
        /// note | session (reflections and reviews have their own commands)
        #[arg(long, default_value = "note")]
        kind: String,
        #[arg(long)]
        title: Option<String>,
        /// Anchor to a page; with --book and no --page, defaults to the book's
        /// current reading page
        #[arg(long)]
        page: Option<i64>,
        /// Don't auto-anchor to the book's current page
        #[arg(long)]
        no_page: bool,
        /// Free-form location anchor (chapter, "loc 1234", %, ...)
        #[arg(long)]
        location: Option<String>,
        /// Anchor to an existing highlight by id
        #[arg(long)]
        highlight: Option<i64>,
    },
    /// List notes, or full-text search them
    Notes {
        /// Restrict to a book (id, ISBN, or title fragment)
        book: Option<String>,
        /// Full-text query over note bodies
        #[arg(long)]
        search: Option<String>,
    },
    /// Show what a note links to, and what links back to it
    Links {
        /// Note selector: id, or part of its title
        note: String,
    },
    /// Open this reading's reflection — private, and it accretes as you read
    Reflect(ReflectArgs),
    /// Open this reading's review — public prose, and the rating lives here
    Review(ReflectArgs),
    /// Cite a highlight from a note (omit the highlight to list what it cites)
    Cite {
        /// Note id (see `notes`)
        note: i64,
        /// Highlight id (see `cite <note>` or `highlights`)
        highlight: Option<i64>,
    },
    /// Your rating scale, and what its values mean on Goodreads
    Rating {
        #[command(subcommand)]
        cmd: RatingCmd,
    },
    /// Show a book's highlights
    Highlights { book: String },
    /// Fold one book into another (moves highlights, notes, cards, links)
    Merge {
        /// The duplicate, deleted once folded in
        src: String,
        /// The book to keep
        dst: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Goodreads CSV, in and out (their API is dead; the file is the interface)
    Goodreads {
        #[command(subcommand)]
        cmd: GoodreadsCmd,
    },
    /// Calibre, if you have it: format conversion and library import
    Calibre {
        #[command(subcommand)]
        cmd: CalibreCmd,
    },
    /// KOReader integration
    Ko {
        #[command(subcommand)]
        cmd: KoCmd,
    },
    /// Flashcards captured from single-word highlights
    Cards {
        #[command(subcommand)]
        cmd: CardsCmd,
    },
    /// Interactive mode
    Repl,
    /// Manage stored configuration (API keys)
    Config {
        #[command(subcommand)]
        cmd: commands::config::ConfigCmd,
    },
}

#[derive(clap::Args)]
struct ReflectArgs {
    /// Book selector: id, ISBN, or title fragment
    book: String,
    /// Which reading, as `show` numbers them (default: the current one)
    #[arg(long)]
    reading: Option<usize>,
    /// Print it and stop, without opening $EDITOR
    #[arg(long)]
    show: bool,
    /// Open it without editing the body
    #[arg(long)]
    no_edit: bool,
    /// Rating on the active scale (`review` only)
    #[arg(long)]
    rating: Option<f64>,
}

impl<'a> From<&'a ReflectArgs> for commands::reflect::ReflectOpts<'a> {
    fn from(a: &'a ReflectArgs) -> Self {
        commands::reflect::ReflectOpts {
            book_selector: &a.book,
            reading: a.reading,
            show: a.show,
            no_edit: a.no_edit,
            rating: a.rating,
        }
    }
}

#[derive(Subcommand)]
enum RatingCmd {
    /// Define (or redefine) a numeric scale
    Scale {
        #[arg(long)]
        min: f64,
        #[arg(long)]
        max: f64,
        #[arg(long)]
        step: f64,
        /// Scale name; new ratings use the most recently created scale
        #[arg(long, default_value = "default")]
        name: String,
    },
    /// Record what one scale value means on Goodreads' integer 0–5
    Map {
        value: f64,
        /// 0–5, where 0 means unrated
        goodreads: u8,
        #[arg(long)]
        scale: Option<String>,
    },
    /// Show the scales and their Goodreads mappings
    Show {
        #[arg(long)]
        scale: Option<String>,
    },
}

#[derive(Subcommand)]
enum GoodreadsCmd {
    /// Import a Goodreads export (My Books > Import and export > Export)
    Import {
        path: PathBuf,
        /// Report what would change without writing
        #[arg(long)]
        dry_run: bool,
        /// Create a book even for a row that looks like one you already have
        #[arg(long)]
        new: bool,
    },
    /// Write a Goodreads-importable CSV of the library
    Export {
        /// Output file (default: goodreads.csv)
        #[arg(long, short, default_value = "goodreads.csv")]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
enum CalibreCmd {
    /// Say which calibre tools are here, and what they enable
    Status,
    /// Convert a book to another format (calibre reads both from the extensions)
    Convert {
        input: PathBuf,
        output: PathBuf,
        /// Overwrite an existing output file
        #[arg(long)]
        force: bool,
    },
    /// Import the books calibre already holds
    Import {
        /// The calibre library to read (default: calibre's own)
        #[arg(long)]
        library: Option<PathBuf>,
        /// Report what would change without writing
        #[arg(long)]
        dry_run: bool,
        /// Create a book even for a row that looks like one you already have
        #[arg(long)]
        new: bool,
    },
}

#[derive(Subcommand)]
enum KoCmd {
    /// Import highlights/notes from a sidecar file, .sdr dir, or library root
    Import {
        path: PathBuf,
        /// Report what would be imported without writing
        #[arg(long)]
        dry_run: bool,
    },
    /// Pull a book in from the reader: create it from the sidecar, then import
    Pull {
        /// A sidecar file or its .sdr directory
        path: PathBuf,
        /// Create a new book even when the library holds a near-miss
        #[arg(long)]
        new: bool,
    },
    /// Record that a sidecar is a book you already have, then import into it
    Link {
        /// A sidecar file or its .sdr directory
        path: PathBuf,
        /// Book selector: id, ISBN, or title fragment
        book: String,
    },
    /// Show the state of every book on a mounted reader. Read-only
    Scan {
        /// The mount to scan. Omitted, a mounted KOReader device is looked for
        path: Option<PathBuf>,
    },
    /// Wait for a reader to be plugged in and scan it. Read-only, ctrl-c to stop
    Watch,
    /// Import measured reading time from the reader's own statistics (item 31)
    ///
    /// A verb of its own, and deliberately not part of `sync`: arrival is
    /// read-only, and a scan that quietly imported months of timing data would
    /// not be read-only in spirit. You ask for this by name.
    Stats {
        /// The mount to read `statistics.sqlite3` from
        path: PathBuf,
    },
    /// Pull books in from a mounted reader
    Sync {
        /// The mount to sync from
        path: PathBuf,
        /// Sync everything new or updated
        #[arg(long)]
        all: bool,
        /// Sync one book: part of its title, or its sidecar path. Repeatable
        #[arg(long = "book")]
        books: Vec<String>,
    },
}

#[derive(Subcommand)]
enum CardsCmd {
    /// List flashcard candidates
    List {
        /// Include already-exported cards
        #[arg(long)]
        all: bool,
    },
    /// Export as Anki-importable TSV
    Export {
        /// Output file (default: anki.tsv)
        #[arg(long, default_value = "anki.tsv")]
        out: PathBuf,
        /// Re-export cards already marked exported
        #[arg(long)]
        all: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    logging::init(cli.verbose, cli.quiet);

    // `config` must not touch the engine: it would create database/ in the
    // current directory just to store a key.
    if let Cmd::Config { cmd } = cli.cmd {
        return commands::config::run(cmd, cli.google_api_key.as_deref()).await;
    }

    let data_root = cli
        .data_dir
        .or_else(|| std::env::var_os("READINGBUDDY_DATA_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut config = EngineConfig::rooted_at(data_root);
    // Key precedence: --google-api-key flag / env (merged by clap) > config file.
    config.google_api_key = cli
        .google_api_key
        .clone()
        .or(config_file::load()?.google_api_key);
    readingbuddy::crash::install_hook(readingbuddy::CrashContext {
        app: "readingbuddy",
        version: env!("CARGO_PKG_VERSION"),
        log_dir: config.log_dir.clone(),
        log_file: None,
    });
    if cli.panic_now {
        panic!("--panic-now: deliberate crash to exercise the crash hook");
    }
    let engine = Engine::open(config).await?;

    match cli.cmd {
        Cmd::Search(args) => commands::search::run(&engine, args).await?,
        Cmd::Add { isbn, no_cover } => commands::book::add_isbn(&engine, &isbn, no_cover).await?,
        Cmd::Epub { path } => commands::book::import_epub(&engine, &path).await?,
        Cmd::List { limit, sort } => commands::book::list(&engine, limit, &sort).await?,
        Cmd::Show { book } => commands::book::show(&engine, &book).await?,
        Cmd::Toc { book } => commands::book::toc(&engine, &book).await?,
        Cmd::Activity {
            book,
            from,
            to,
            days,
            refill,
        } => {
            commands::activity::run(
                &engine,
                commands::activity::Args {
                    book,
                    from,
                    to,
                    days,
                    refill,
                },
            )
            .await?
        }
        Cmd::Enrich { book } => commands::enrich::enrich(&engine, &book).await?,
        Cmd::Set(args) => commands::enrich::set(&engine, &args).await?,
        Cmd::Rm { book, yes } => commands::book::remove(&engine, &book, yes).await?,
        Cmd::Progress {
            book,
            page,
            finished,
            reread,
        } => commands::book::progress(&engine, &book, page, finished, reread).await?,
        Cmd::Note {
            text,
            book,
            kind,
            title,
            page,
            no_page,
            location,
            highlight,
        } => {
            commands::note::create(
                &engine,
                commands::note::NoteOpts {
                    book_selector: book.as_deref(),
                    text,
                    kind: &kind,
                    title,
                    page,
                    no_page,
                    location,
                    highlight,
                },
            )
            .await?
        }
        Cmd::Notes { book, search } => {
            commands::note::list_or_search(&engine, book.as_deref(), search.as_deref()).await?
        }
        Cmd::Links { note } => commands::note::links(&engine, &note).await?,
        Cmd::Reflect(args) => commands::reflect::reflect(&engine, (&args).into()).await?,
        Cmd::Review(args) => commands::reflect::review(&engine, (&args).into()).await?,
        Cmd::Cite { note, highlight } => commands::reflect::cite(&engine, note, highlight).await?,
        Cmd::Rating { cmd } => match cmd {
            RatingCmd::Scale {
                min,
                max,
                step,
                name,
            } => commands::rating::scale(&engine, &name, min, max, step).await?,
            RatingCmd::Map {
                value,
                goodreads,
                scale,
            } => commands::rating::map(&engine, scale.as_deref(), value, goodreads).await?,
            RatingCmd::Show { scale } => commands::rating::show(&engine, scale.as_deref()).await?,
        },
        Cmd::Highlights { book } => commands::book::highlights(&engine, &book).await?,
        Cmd::Merge { src, dst, yes } => commands::book::merge(&engine, &src, &dst, yes).await?,
        Cmd::Goodreads { cmd } => match cmd {
            GoodreadsCmd::Import { path, dry_run, new } => {
                commands::goodreads::import(&engine, &path, dry_run, new).await?
            }
            GoodreadsCmd::Export { out } => commands::goodreads::export(&engine, &out).await?,
        },
        Cmd::Calibre { cmd } => match cmd {
            CalibreCmd::Status => commands::calibre::status(&engine).await?,
            CalibreCmd::Convert {
                input,
                output,
                force,
            } => commands::calibre::convert(&engine, &input, &output, force).await?,
            CalibreCmd::Import {
                library,
                dry_run,
                new,
            } => commands::calibre::import(&engine, library, dry_run, new).await?,
        },
        Cmd::Ko { cmd } => match cmd {
            KoCmd::Import { path, dry_run } => {
                commands::ko::import(&engine, &path, dry_run).await?
            }
            KoCmd::Pull { path, new } => commands::ko::pull(&engine, &path, new).await?,
            KoCmd::Link { path, book } => commands::ko::link(&engine, &path, &book).await?,
            KoCmd::Scan { path } => commands::ko::scan(&engine, path.as_deref()).await?,
            KoCmd::Watch => commands::ko::watch(&engine).await?,
            KoCmd::Stats { path } => commands::ko::stats(&engine, &path).await?,
            KoCmd::Sync { path, all, books } => {
                commands::ko::sync(&engine, &path, all, &books).await?
            }
        },
        Cmd::Cards { cmd } => match cmd {
            CardsCmd::List { all } => commands::cards::list(&engine, all).await?,
            CardsCmd::Export { out, all } => commands::cards::export(&engine, &out, all).await?,
        },
        Cmd::Repl => repl::run(&engine).await?,
        Cmd::Config { .. } => unreachable!("handled before engine startup"),
    }
    Ok(())
}
