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
    },
    /// Write a note (opens $EDITOR when TEXT is omitted)
    Note {
        /// Note body; omit to compose in $EDITOR
        text: Option<String>,
        /// Attach to a book (id, ISBN, or title fragment)
        #[arg(long)]
        book: Option<String>,
        /// note | session | final
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
        Cmd::Rm { book, yes } => commands::book::remove(&engine, &book, yes).await?,
        Cmd::Progress {
            book,
            page,
            finished,
        } => commands::book::progress(&engine, &book, page, finished).await?,
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
        Cmd::Highlights { book } => commands::book::highlights(&engine, &book).await?,
        Cmd::Merge { src, dst, yes } => commands::book::merge(&engine, &src, &dst, yes).await?,
        Cmd::Ko { cmd } => match cmd {
            KoCmd::Import { path, dry_run } => {
                commands::ko::import(&engine, &path, dry_run).await?
            }
            KoCmd::Pull { path, new } => commands::ko::pull(&engine, &path, new).await?,
            KoCmd::Link { path, book } => commands::ko::link(&engine, &path, &book).await?,
            KoCmd::Scan { path } => commands::ko::scan(&engine, path.as_deref()).await?,
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
