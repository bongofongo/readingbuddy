//! The CLI as a process — the only thing that can observe what the binary
//! actually does.
//!
//! Everything else in this workspace tests the engine. `crates/cli` had tests
//! in `logging.rs` and nowhere else, so argument parsing, selector resolution,
//! the confirmation prompts and the "this command must not open the engine"
//! rules were all enforced by the code alone.
//!
//! **Zero new dependencies**: `env!("CARGO_BIN_EXE_readingbuddy")` is set by
//! cargo for integration tests, and `std::process::Command` does the rest.
//!
//! Offline throughout — nothing here searches, and the one import path used
//! (`ko pull`) reads a committed fixture.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_readingbuddy");

const SYNTHETIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../engine/tests/fixtures/koreader/synthetic"
);

/// A sandboxed invocation of the real binary.
struct Cli {
    root: tempfile::TempDir,
}

struct Out {
    ok: bool,
    stdout: String,
    stderr: String,
}

impl Cli {
    fn new() -> Cli {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("home")).unwrap();
        std::fs::create_dir_all(root.path().join("config")).unwrap();
        std::fs::create_dir_all(root.path().join("data")).unwrap();
        Cli { root }
    }

    fn data_dir(&self) -> PathBuf {
        self.root.path().join("data")
    }

    fn try_run(&self, args: &[&str]) -> Out {
        self.try_run_in(&self.data_dir(), args)
    }

    /// Every environment pin here is load-bearing, and one of them is a safety
    /// issue rather than hygiene.
    fn try_run_in(&self, cwd: &Path, args: &[&str]) -> Out {
        self.try_run_with_path(cwd, None, args)
    }

    /// The same, with `PATH` prefixed by `bin_dir`.
    ///
    /// This is the only place `PATH` can honestly be exercised: it belongs to a
    /// *child* process here, so setting it is a per-spawn value rather than a
    /// `set_var` racing every other test in the binary. It is what covers
    /// calibre feature detection finding a tool the ordinary way — the engine's
    /// own suite points `EngineConfig::calibre_bin_dir` at a directory instead,
    /// precisely to keep out of the process environment.
    fn try_run_with_path(&self, cwd: &Path, bin_dir: Option<&Path>, args: &[&str]) -> Out {
        let path = match bin_dir {
            Some(dir) => {
                let existing = std::env::var("PATH").unwrap_or_default();
                format!("{}:{existing}", dir.display())
            }
            None => std::env::var("PATH").unwrap_or_default(),
        };
        let out: Output = Command::new(BIN)
            .env("PATH", path)
            .args(["--data-dir", self.data_dir().to_str().unwrap()])
            .args(args)
            .current_dir(cwd)
            // An ambient READINGBUDDY_DATA_DIR would silently redirect the whole
            // suite at the developer's own library.
            .env_remove("READINGBUDDY_DATA_DIR")
            // XDG_CONFIG_HOME *and* HOME, because `config_file::config_path`
            // falls back to `home_dir()/.config` when XDG is unset. With only
            // one of them pinned, `config set google-api-key` would write the
            // developer's real key file. A test that can damage the machine it
            // runs on is not a test.
            .env("XDG_CONFIG_HOME", self.root.path().join("config"))
            .env("HOME", self.root.path().join("home"))
            // clap merges this from the environment, so a developer with a key
            // exported would make `search` reach the network — and this suite
            // makes no network calls, ever.
            .env_remove("GOOGLE_BOOKS_API_KEY")
            // A developer's RUST_LOG=debug changes stderr out from under the
            // assertions.
            .env_remove("RUST_LOG")
            // Belt and braces: if a regression ever routes one of these through
            // an editor, `true` exits immediately instead of hanging CI on a vi
            // that cannot read its terminal.
            .env("EDITOR", "true")
            .env("VISUAL", "true")
            // EOF on stdin. This is also an assertion in its own right — see
            // `a_destructive_command_declines_when_it_cannot_ask`.
            .stdin(Stdio::null())
            .output()
            .expect("the binary runs");
        Out {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn run(&self, args: &[&str]) -> Out {
        let out = self.try_run(args);
        assert!(
            out.ok,
            "`readingbuddy {}` failed\n--- stdout ---\n{}\n--- stderr ---\n{}",
            args.join(" "),
            out.stdout,
            out.stderr
        );
        out
    }
}

impl Out {
    fn has(&self, needle: &str) -> &Out {
        assert!(
            self.stdout.contains(needle) || self.stderr.contains(needle),
            "expected {needle:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.stdout,
            self.stderr
        );
        self
    }

    fn lacks(&self, needle: &str) -> &Out {
        assert!(
            !self.stdout.contains(needle) && !self.stderr.contains(needle),
            "did not expect {needle:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.stdout,
            self.stderr
        );
        self
    }

    /// The one place these tests are coupled to output *formatting*, and it is
    /// deliberate: parsing an id out of one command and feeding it to the next
    /// is what lets everything else assert on the contract *between* commands
    /// rather than on their wording. `render::book_line` prints `#<id>  …`.
    fn book_id(&self) -> String {
        self.stdout
            .split('#')
            .nth(1)
            .and_then(|rest| {
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                (!digits.is_empty()).then_some(digits)
            })
            .unwrap_or_else(|| panic!("no `#<id>` in output:\n{}", self.stdout))
    }
}

/// Copy a committed sidecar fixture into a writable tree.
fn place(dst_root: &Path, fixture: &str) -> PathBuf {
    let src = Path::new(SYNTHETIC).join(fixture);
    let dst = dst_root.join(fixture);
    std::fs::create_dir_all(&dst).unwrap();
    let mut sidecar = None;
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        std::fs::copy(entry.path(), dst.join(&name)).unwrap();
        let n = name.to_string_lossy().into_owned();
        if n.starts_with("metadata.") && n.ends_with(".lua") {
            sidecar = Some(dst.join(n));
        }
    }
    sidecar.expect("fixture has a sidecar")
}

/// The subcommand set is a promise, not a detail — so this one golden is right
/// where a golden of whole output would be wrong.
///
/// Comparing the *names* rather than the help text makes it immune to clap's
/// formatting and to every cosmetic change, while still failing when a
/// subcommand appears, vanishes or is renamed without anyone deciding to.
#[test]
fn the_subcommand_set_is_what_we_decided() {
    let cli = Cli::new();
    let help = cli.run(&["--help"]).stdout;

    // Everything indented by exactly two spaces in clap's Commands block, up to
    // the first whitespace.
    let mut found: Vec<&str> = help
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .collect();
    found.sort_unstable();
    found.dedup();

    let expected = [
        "activity",
        "add",
        "calibre",
        "cards",
        "cite",
        "config",
        "covers",
        "enrich",
        "epub",
        "find",
        "goodreads",
        "help",
        "highlights",
        "ko",
        "links",
        "list",
        "merge",
        "note",
        "notes",
        "progress",
        "rating",
        "reflect",
        "repl",
        "review",
        "rm",
        "search",
        "set",
        "show",
        "toc",
    ];
    assert_eq!(
        found, expected,
        "the subcommand set changed. If that was deliberate, update this list \
         in the same commit — that is the point of it."
    );
}

/// `config` must not open the engine, and therefore must not create `database/`.
///
/// `CLAUDE.md` states it as a rule and the code respects it, but nothing could
/// *observe* it: from inside the process the difference between "opened the
/// engine" and "didn't" is invisible, while from outside it is one directory.
#[test]
fn config_never_opens_the_engine() {
    let cli = Cli::new();
    let scratch = cli.root.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();

    cli.try_run_in(&scratch, &["config", "show"]);

    assert!(
        !scratch.join("database").exists(),
        "`config` created a database/ in the working directory"
    );
    assert!(
        !cli.data_dir().join("database").exists(),
        "`config` created a database/ in the data dir"
    );
}

/// An empty library says so, and creates its data dir where it was told rather
/// than where it happened to be run.
#[test]
fn an_empty_library_says_so_and_writes_only_where_told() {
    let cli = Cli::new();
    let scratch = cli.root.path().join("scratch2");
    std::fs::create_dir_all(&scratch).unwrap();

    cli.try_run_in(&scratch, &["list"]).has("library is empty");

    assert!(cli.data_dir().join("database/app.db").is_file());
    assert!(
        !scratch.join("database").exists(),
        "--data-dir was ignored and the cwd was used instead"
    );
}

/// Pull a book off a fixture sidecar, set progress, read it back.
///
/// Three commands agreeing about one book, which is the contract worth pinning:
/// `ko pull` reports an id, `progress` accepts it, `show` reflects what
/// `progress` set. It also exercises the offline import path end to end through
/// the binary — `epub` is unusable here, since a found ISBN goes to the
/// providers and this suite makes no network calls.
#[test]
fn pull_then_progress_then_show_agree_about_the_same_book() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Gen-Summary.sdr");

    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);

    let listed = cli.run(&["list"]);
    listed.has("A Rated Book");
    let id = listed.book_id();

    cli.run(&["progress", &id, "120"]);
    cli.run(&["show", &id]).has("120").has("A Rated Book");
}

/// A confirmation prompt reading EOF must decline.
///
/// This is the one that would actually hurt. `prompt::confirm` treats anything
/// but `y`/`yes` as no, so a closed stdin declines — but nothing asserted it,
/// and the opposite reading (EOF as "assume yes") is a plausible thing for
/// someone to change while making a command scriptable. It would destroy a
/// library from a cron job.
#[test]
fn a_destructive_command_declines_when_it_cannot_ask() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Gen-Summary.sdr");
    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);
    let id = cli.run(&["list"]).book_id();

    cli.run(&["rm", &id]).has("kept.");
    cli.run(&["list"]).has("A Rated Book");

    // …and with an explicit --yes it goes through, so the guard above is the
    // prompt declining rather than `rm` being broken.
    cli.run(&["rm", &id, "--yes"]);
    cli.run(&["list"]).has("library is empty");
}

/// `reflect --show` on a book with no reflection must not create one — and, the
/// part that is easy to get wrong, must not open a *reading* either.
///
/// `open_reflection` resolves its anchor through `ensure_reading`, so a
/// read-only inspection that reached it would quietly start reading a book the
/// user only looked at. `--show` exists so that opening one is never forced
/// through an editor, and this is what keeps it read-only.
///
/// The second half — actually opening it — is not decoration. Without it the
/// two `lacks` assertions above would also pass against a `reflect` that did
/// nothing at all, which is the failure mode a negative assertion invites.
#[test]
fn reflect_show_creates_neither_a_note_nor_a_reading() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Unmatched.sdr");
    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);
    let id = cli.run(&["list"]).book_id();

    cli.run(&["reflect", &id, "--show"])
        .has("no reflection yet");
    cli.run(&["notes"]).lacks("Reflection:");
    // `show` lists a book's readings; the pull created none, and looking at the
    // reflection must not either.
    cli.run(&["show", &id]).lacks("reading 1/1");

    // Now open it for real, and both appear — so the absences above were real.
    cli.run(&["reflect", &id, "--no-edit"]);
    cli.run(&["notes"]).has("Reflection:");
    cli.run(&["show", &id]).has("reading 1/1");
}

/// `links` reads the graph in both directions, and says so about the half that
/// is not there yet.
///
/// The first note's `[[Han]]` is written before any note called Han exists — a
/// forward reference, which the engine back-resolves when the target appears.
/// That is the property `Storage::backlinks` is a plain `WHERE to_note = ?`
/// because of, and this is it observed from outside the process: if
/// back-resolution ever stopped happening, the inbound half of the *first*
/// note's output would silently go empty.
///
/// The dangling `[[Noa]]` is the other half of the contract: a target with no
/// note is printed as the text it is, never dropped. A note you have not
/// written yet is a place to go, not an error.
#[test]
fn links_reads_both_directions_and_prints_a_dangling_target_as_text() {
    let cli = Cli::new();

    cli.run(&["note", "Her whole life is [[Han]].", "--title", "Sunja"]);
    cli.run(&[
        "note",
        "Grief with no bottom. Cf. [[Sunja]], and [[Noa]] later.",
        "--title",
        "Han",
    ]);

    // By title, which is what a wikilink names — and the forward reference
    // resolved, so it is a real edge in both directions.
    cli.run(&["links", "Sunja"])
        .has("links out:")
        .has("“Han”")
        .has("links in:");

    let han = cli.run(&["links", "Han"]);
    han.has("“Sunja”");
    han.has("“Noa”").has("text");

    // Nothing resolves to Noa, so it is not a note at all.
    let missing = cli.try_run(&["links", "Noa"]);
    assert!(!missing.ok, "a selector matching no note must not exit 0");
    missing.has("no note matches");
}

/// Calibre is feature-detected off `PATH`, and the binary finds it there.
///
/// The engine's own suite points `EngineConfig::calibre_bin_dir` at a directory
/// of fakes, deliberately, because `set_var("PATH")` from a test is a data race.
/// That leaves the `PATH` half — the one every real user actually takes —
/// unasserted, and this is where it can be asserted safely: the environment
/// belongs to the child.
///
/// The *absent* half is deliberately not here. Detection falls through to `PATH`
/// and then to the directories calibre installs itself in — which is right, and
/// which makes "no calibre" unreachable from outside the process on a machine
/// that has it. The wording of that branch has a rule attached to it
/// (`docs/decisions.md`: never ask the user to install or configure other
/// software), so it is asserted where it is deterministic:
/// `commands::calibre::tests::an_absent_calibre_is_reported_and_never_prescribed`.
#[cfg(unix)]
#[test]
fn calibre_is_found_on_path_and_both_tiers_run_through_the_binary() {
    use std::os::unix::fs::PermissionsExt;

    let cli = Cli::new();
    let bin = cli.root.path().join("fakebin");
    std::fs::create_dir_all(&bin).unwrap();
    let lib = cli.root.path().join("callib");
    std::fs::create_dir_all(&lib).unwrap();
    // The marker the engine insists on before it will run calibredb — without
    // it a mistyped path has calibre *create* a library there.
    std::fs::write(lib.join("metadata.db"), b"").unwrap();

    let write = |name: &str, body: &str| {
        let p = bin.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    write("ebook-convert", "cp \"$1\" \"$2\"");
    write(
        "calibredb",
        r#"cat <<'EOF'
[{"id": 1, "uuid": "aaaa-bbbb", "title": "Pachinko", "authors": "Min Jin Lee & Deborah Smith", "tags": ["fiction"]}]
EOF"#,
    );

    let with_fakes = |args: &[&str]| cli.try_run_with_path(&cli.data_dir(), Some(&bin), args);

    // Found on PATH, and the status line says what that enables.
    let status = with_fakes(&["calibre", "status"]);
    assert!(status.ok, "status must not fail\n{}", status.stderr);
    status
        .has("ebook-convert")
        .has("calibredb")
        .has("conversion and library import are available.");

    // Tier (ii), through the real binary: the fake's JSON becomes a book.
    with_fakes(&["calibre", "import", "--library", lib.to_str().unwrap()]).has("Pachinko");
    cli.run(&["list"]).has("Pachinko");

    // Tier (i): the two paths reach `ebook-convert`, and a second run is
    // refused with the flag that would allow it.
    let input = cli.root.path().join("in.epub");
    let output = cli.root.path().join("out.azw3");
    std::fs::write(&input, b"epub").unwrap();
    with_fakes(&[
        "calibre",
        "convert",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ])
    .has("out.azw3");
    assert!(output.is_file());
    with_fakes(&[
        "calibre",
        "convert",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ])
    .has("--force");
}

/// A selector that matches nothing is an error, and an under-specified sync
/// writes nothing and names the flag that would.
///
/// Both are stated in `CLAUDE.md` as deliberate behaviour — "an unmatched
/// `--book` selector is an error, never a silent no-op", and `ko sync` with
/// neither `--all` nor `--book` "prints what would happen and names the flag
/// rather than writing forty books unasked".
#[test]
fn a_bad_selector_fails_loudly_and_an_underspecified_sync_writes_nothing() {
    let cli = Cli::new();
    let bad = cli.try_run(&["show", "zzz-no-such-book"]);
    assert!(!bad.ok, "an unmatched selector must not exit 0");
    bad.has("zzz-no-such-book");

    // A mount with one book on it, and a sync that was not told what to do.
    let device = cli.root.path().join("mount");
    place(&device, "Gen-Summary.sdr");

    let vague = cli.run(&["ko", "sync", device.to_str().unwrap()]);
    vague.has("--all");
    cli.run(&["list"]).has("library is empty");
}

/// The activity log, end to end — and the two rules it exists to keep.
///
/// **One: an empty log is not "you did not read", it is "nobody has built it
/// yet".** No importer fills `reading_events` (deliberately — a log that filled
/// itself as a side effect would be whichever importer ran last), so a fresh
/// library reports nothing and has to name the move that changes that. A dead
/// end here is exactly what `docs/decisions.md` bans.
///
/// **Two: absence is not zero.** A sidecar's highlight stamps say which days you
/// were in the book and measure nothing at all, so the minutes total must read
/// as unmeasured. `0` there is a false statement about the user's own reading,
/// and it is one `unwrap_or(0)` away at every layer between the SQL and this
/// line.
#[test]
fn an_empty_activity_log_names_the_move_and_a_refill_measures_nothing_it_did_not() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Gen-Summary.sdr");
    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);
    let id = cli.run(&["list"]).book_id();

    // Nothing has filled the log, so it says so and names what does.
    cli.run(&["activity", "--from", "2020-01-01", "--to", "2035-01-01"])
        .has("nothing recorded here yet")
        .has("activity --refill");

    let refilled = cli.run(&[
        "activity",
        "--refill",
        "--days",
        "--from",
        "2020-01-01",
        "--to",
        "2035-01-01",
    ]);
    refilled.has("rebuilt from what was already here");
    // Highlight stamps place the days. They do not time them.
    refilled.has("not measured").lacks("nothing recorded here");
    // Nothing counts what was not done — no streak, no "n of 30".
    refilled.lacks("streak").lacks("goal");

    // The per-book view says who supplied each day and how much it claims —
    // and here that is the sharpest form of the rule: these days are
    // `measured`, because a reading's own endpoints are dates somebody
    // recorded, and they still carry **no minutes**. Confidence is about the
    // day, not about the columns beside it, and a renderer that read
    // `measured` as "so there is a number" would print a zero nobody measured.
    cli.run(&["activity", "--book", &id])
        .has("koreader")
        .has("measured")
        .has("—");
}

/// A book with no owned file has **no chapter list to read**, which is not the
/// same answer as an epub carrying no TOC — and the command has to say which.
#[test]
fn a_book_with_no_epub_says_there_is_no_file_rather_than_no_chapters() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Gen-Summary.sdr");
    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);
    let id = cli.run(&["list"]).book_id();

    cli.run(&["toc", &id])
        .has("no epub here")
        .has("readingbuddy epub");
}

/// `set` writes the three columns item 32 added, echoes them, and refuses half a
/// pair — a number with no series to belong to.
#[test]
fn set_writes_the_series_pair_and_refuses_half_of_it() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Gen-Summary.sdr");
    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);
    let id = cli.run(&["list"]).book_id();

    let orphan = cli.try_run(&["set", &id, "--series-index", "2"]);
    assert!(!orphan.ok, "an index with no series is not a correction");
    assert!(orphan.stderr.contains("in no series"), "{}", orphan.stderr);

    cli.run(&[
        "set",
        &id,
        "--series",
        "Dune",
        "--series-index",
        "2",
        "--subject",
        "Fiction / Literary",
    ])
    .has("series")
    .has("Fiction / Literary");

    // The pair reads as one fact, written the way a person writes it.
    cli.run(&["show", &id])
        .has("Dune #2")
        .has("Fiction / Literary");

    // …and numbering a book whose series is already recorded is allowed, which
    // is the ordinary use of the flag the refusal above must not block.
    cli.run(&["set", &id, "--series-index", "3"])
        .has("series_index");
    cli.run(&["show", &id]).has("Dune #3");
}

/// The cover back-fill has a door, and it is reachable from the binary.
///
/// `Engine::measure_stored_covers` was written by item 20 and called by nothing
/// for a whole wave — tested on the facade, unreachable from outside the
/// process, and therefore never run over the library a shelf would be built
/// against. That is the same failure as `list --sort author`: the engine can do
/// it and nothing can ask. The counts are the facade test's business
/// (`engine_facade.rs`); what only this can observe is that the verb exists,
/// opens the engine and exits 0.
///
/// A book pulled from a sidecar has no cover, so the work list is empty — and
/// the empty answer must not be a zero.
#[test]
fn the_cover_back_fill_is_reachable_from_the_binary() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Gen-Summary.sdr");
    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);

    cli.run(&["covers"])
        .has("already measured")
        .lacks("0 covers");
}

// ---- item 33: the search door ----------------------------------------------

/// The whole point of `rb find`: a highlight that arrived through the KOReader
/// path is findable from a terminal.
///
/// Through the binary rather than the facade, because that is the claim — the
/// engine could search and, until this command, nothing outside the process
/// could ask it to. That is the failure `measure_stored_covers` and
/// `list --sort author` both had.
#[test]
fn an_imported_highlight_is_findable_from_the_command_line() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Gen-Summary.sdr");
    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);

    cli.run(&["find", "passage"])
        .has("highlight")
        .has(">>passage<<");

    // The narrowing is a filter over the same list, not a second search.
    cli.run(&["find", "passage", "--notes"])
        .has("no notes match");
    cli.run(&["find", "passage", "--highlights"])
        .has(">>passage<<");
}

/// Absence is an answer, in all three of its shapes: nothing matched, nothing
/// was asked, and something was typed that fts5 reads as syntax.
///
/// The last one is a live defect this item fixed rather than a hypothetical —
/// `notes --search "don't"` used to fail with a raw database error, because the
/// query went into `MATCH` unquoted.
#[test]
fn a_search_that_finds_nothing_is_never_an_error() {
    let cli = Cli::new();
    let device = cli.root.path().join("device");
    let sidecar = place(&device, "Gen-Summary.sdr");
    cli.run(&["ko", "pull", sidecar.to_str().unwrap()]);

    let none = cli.run(&["find", "thermodynamics"]);
    none.has("no notes or highlights match");
    assert!(
        !none.stdout.contains('0'),
        "absence is not zero: {}",
        none.stdout
    );

    cli.run(&["find", "   "]).has("nothing to search for");
    // Two things fts5 would have raised a syntax error on.
    cli.run(&["find", "don't"])
        .has("no notes or highlights match");
    cli.run(&["find", "C++"])
        .has("no notes or highlights match");
}
