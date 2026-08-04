//! Fixture generator for the KOReader import harness.
//!
//! Two tiers, and the split is deliberate:
//!
//! * **tier 1, `gen-synthetic`** — small, committed, covers every *shape*
//!   (hostile input, the sibling-epub ISBN match path). Runs offline in CI on
//!   every PR. Shape coverage must not sit behind a download, or the branch is
//!   untested on every machine that has not run the fetch script.
//! * **tier 2, `gen-corpus`** — derived from real Project Gutenberg epubs,
//!   gitignored, covers *scale and realism*. Nightly only.
//!
//! This crate does **not** depend on `readingbuddy`. Reusing the engine's own
//! parsing or normalization to build its fixtures would bake any bug in those
//! straight into the goldens; the generator stays an independent oracle.

mod goodreads;
mod gutenberg;
mod kostats;
mod synthetic;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "corpus", about = "Generate KOReader import fixtures")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

// The shared `Gen` prefix is the point, not an accident: `gen-synthetic`,
// `gen-goodreads` and `gen-corpus` are the CLI's own vocabulary for "write
// fixtures", and renaming the variants to please the lint would rename the
// commands people type.
#[allow(clippy::enum_variant_names)]
#[derive(Subcommand)]
enum Cmd {
    /// Write the committed tier-1 hostile fixtures.
    GenSynthetic {
        /// Fixture root; defaults to the engine's `tests/fixtures/koreader`.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write the committed Goodreads export fixture.
    ///
    /// Beside the hand-authored files in `.../goodreads/recorded/`, which are a
    /// recording of Goodreads' own dialect and are the documented exception to
    /// "fixtures are generated". This one covers volume and variety instead.
    GenGoodreads {
        /// PRNG seed. ChaCha8, so the same seed is the same file for ever.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// How many rows.
        #[arg(long, default_value_t = 40)]
        books: usize,
        /// Output dir; defaults to the engine's
        /// `tests/fixtures/goodreads/generated`.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write the committed KOReader `statistics.sqlite3` fixture (item 31).
    ///
    /// Emits **SQL text plus an expected-totals JSON**, not a database file:
    /// see `kostats.rs` for why, and note that the totals are accumulated while
    /// the rows are written rather than read back, which is what makes them an
    /// oracle rather than a recording.
    GenKostats {
        /// PRNG seed. ChaCha8, so the same seed is the same file for ever.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Output dir; defaults to the engine's
        /// `tests/fixtures/koreader/statistics`.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Derive tier-2 sidecars from the fetched Project Gutenberg epubs.
    GenCorpus {
        /// PRNG seed. Output is a pure function of (seed, generator version,
        /// epub bytes), so the same seed always reproduces the same corpus.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Highlights per sidecar.
        #[arg(long, default_value_t = 40)]
        per_book: usize,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long)]
        epubs: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::GenSynthetic { out } => {
            let root = out.unwrap_or_else(default_fixture_root);
            let written = synthetic::generate(&root)?;
            println!("wrote {written} fixture files under {}", root.display());
            println!("now run `make golden` to record their expected behaviour");
            Ok(())
        }
        Cmd::GenGoodreads { seed, books, out } => {
            let out = out.unwrap_or_else(goodreads::default_out);
            let n = goodreads::generate(&out, &goodreads::Options { seed, books })?;
            println!(
                "wrote {n} rows (v{}, seed {seed}) to {}/library.csv",
                goodreads::GENERATOR_VERSION,
                out.display()
            );
            Ok(())
        }
        Cmd::GenKostats { seed, out } => {
            let out = out.unwrap_or_else(kostats::default_out);
            let n = kostats::generate(&out, &kostats::Options { seed })?;
            println!(
                "wrote {n} statistics books (v{}, seed {seed}) to {}/statistics.sql",
                kostats::GENERATOR_VERSION,
                out.display()
            );
            Ok(())
        }
        Cmd::GenCorpus {
            seed,
            per_book,
            manifest,
            epubs,
            out,
        } => {
            let manifest = manifest.unwrap_or_else(gutenberg::default_manifest);
            let epubs = epubs.unwrap_or_else(gutenberg::default_epub_dir);
            let out = out.unwrap_or_else(gutenberg::default_out);
            let n = gutenberg::generate(
                &manifest,
                &epubs,
                &out,
                &gutenberg::Options { seed, per_book },
            )?;
            println!(
                "wrote {n} sidecars (v{}, seed {seed}) under {}",
                gutenberg::GENERATOR_VERSION,
                out.display()
            );
            Ok(())
        }
    }
}

fn default_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/koreader")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("crates/engine/tests/fixtures/koreader"))
}
