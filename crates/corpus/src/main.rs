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

mod gutenberg;
mod synthetic;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "corpus", about = "Generate KOReader import fixtures")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write the committed tier-1 hostile fixtures.
    GenSynthetic {
        /// Fixture root; defaults to the engine's `tests/fixtures/koreader`.
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
