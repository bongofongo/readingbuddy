use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use readingbuddy::{BookImportStats, DeviceBook, DeviceState, Engine, MatchCandidate};

use super::resolve_one;

pub async fn import(engine: &Engine, path: &Path, dry_run: bool) -> Result<()> {
    let report = engine.import_koreader(path, dry_run).await?;
    let mode = if dry_run { " (dry run)" } else { "" };

    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    for s in &report.imported {
        println!("{}", stats_line(s, mode));
    }
    for u in &report.unmatched {
        println!(
            "unmatched{mode}: {} ({})",
            u.title.as_deref().unwrap_or("unknown title"),
            u.path.display()
        );
        // Unmatched is a decision, not a dead end. Say what the two moves are,
        // and — when the library holds something close — which book it probably
        // is, rather than leaving the user to find it.
        print_candidates(&u.candidates);
        println!("    pull it in : readingbuddy ko pull {}", u.path.display());
        println!(
            "    or link it : readingbuddy ko link {} <book>",
            u.path.display()
        );
    }
    if report.imported.is_empty() && report.unmatched.is_empty() && report.warnings.is_empty() {
        println!("nothing to import.");
    }
    Ok(())
}

/// Create the book from the sidecar's own metadata and import its highlights.
pub async fn pull(engine: &Engine, path: &Path, new: bool) -> Result<()> {
    // Look before creating. The whole reason `match_candidates` exists is that a
    // variant title used to become a silent duplicate, and creating first and
    // warning after would reproduce exactly that.
    if !new {
        let candidates = engine.sidecar_candidates(path).await?;
        if !candidates.is_empty() {
            println!("{} looks like a book you already have:", path.display());
            print_candidates(&candidates);
            println!(
                "    link it    : readingbuddy ko link {} <book>",
                path.display()
            );
            println!("    or pull it as a new book: --new");
            return Ok(());
        }
    }

    let report = engine.pull_book_from_sidecar(path).await?;
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    println!("{}", stats_line(&report.stats, ""));
    // The pull is offline by design, so the book arrives bare. This line used to
    // name `search` then `ko link` — hand-assembling, out of two commands meant
    // for something else, exactly the thing item 30 built. It is a real command
    // now, and it is one.
    println!(
        "  #{} — no ISBN, cover or description yet: `readingbuddy enrich {}`",
        report.stats.book_id, report.stats.book_id
    );
    Ok(())
}

/// Record that this sidecar is that book, then import into it.
pub async fn link(engine: &Engine, path: &Path, selector: &str) -> Result<()> {
    let book = resolve_one(engine, selector).await?;
    let Some(book_id) = book.id else {
        bail!("'{selector}' resolved to an unsaved book");
    };
    let md5 = engine.link_sidecar(path, book_id).await?;
    println!(
        "linked {} to {} ({md5})",
        path.display(),
        book.display_title()
    );
    // The link only pays off on the next import, so run it now: otherwise
    // `link` looks like it did nothing.
    import(engine, path, false).await
}

/// Show the state of every book on a mounted reader. Writes nothing.
pub async fn scan(engine: &Engine, path: Option<&Path>) -> Result<()> {
    let root = resolve_mount(path)?;
    let scan = engine.scan_device(&root).await?;

    for w in &scan.warnings {
        eprintln!("warning: {w}");
    }
    for b in &scan.books {
        println!("{}", device_line(b));
        match &b.state {
            // Unmatched is a decision, not a dead end — say what the two moves
            // are, and which book it probably already is.
            DeviceState::New { candidates } => {
                print_candidates(candidates);
                if !candidates.is_empty() {
                    println!(
                        "    link it    : readingbuddy ko link {} <book>",
                        b.path.display()
                    );
                }
            }
            DeviceState::Unreadable(d) => println!("    {}", d.detail),
            _ => {}
        }
    }

    let syncable = scan.syncable().count();
    println!();
    println!(
        "{} books on {} ({} read, {} unchanged since last scan)",
        scan.books.len(),
        root.display(),
        scan.parsed,
        scan.cached
    );
    if syncable > 0 {
        println!(
            "  bring them across: readingbuddy ko sync {} --all",
            root.display()
        );
    }
    Ok(())
}

/// Wait for a reader to be plugged in, and scan it when it is. Writes nothing.
///
/// This is the headless half of the mount watcher, and the instrument the wired
/// path is confirmed with on real hardware — the TUI does the same thing behind
/// a screen, which is one more thing to be wrong when a device does not show up.
///
/// **Scans, never syncs**, like everything else on the automatic path: it prints
/// the command that brings books across rather than running it.
pub async fn watch(engine: &Engine) -> Result<()> {
    let mut watcher = readingbuddy::watch_mounts()?;

    let roots = readingbuddy::mount_roots();
    println!(
        "watching {} — ctrl-c to stop",
        roots
            .iter()
            .map(|r| r.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    // A watcher reports changes, so a reader already plugged in would otherwise
    // make this look like it had failed to notice one.
    match readingbuddy::candidate_mounts().as_slice() {
        [] => println!("nothing mounted yet."),
        mounts => {
            for mount in mounts {
                println!("already here: {}", mount.display());
            }
            println!("  scan one now: readingbuddy ko scan <path>");
        }
    }

    while let Some(event) = watcher.next().await {
        match event {
            readingbuddy::MountEvent::Arrived(mount) => {
                println!();
                println!("reader mounted: {}", mount.display());
                scan(engine, Some(&mount)).await?;
            }
            readingbuddy::MountEvent::Departed(mount) => {
                println!();
                println!("unplugged: {}", mount.display());
            }
        }
    }
    Ok(())
}

/// Pull a selection of a mounted reader in.
pub async fn sync(engine: &Engine, path: &Path, all: bool, selectors: &[String]) -> Result<()> {
    let scan = engine.scan_device(path).await?;
    for w in &scan.warnings {
        eprintln!("warning: {w}");
    }

    let syncable: Vec<&DeviceBook> = scan.syncable().collect();
    if syncable.is_empty() {
        println!(
            "nothing to sync — everything on {} is already here.",
            path.display()
        );
        return Ok(());
    }

    // Neither flag is not "sync everything": it is a question. Show what would
    // happen and name the flag rather than writing forty books unasked.
    if !all && selectors.is_empty() {
        println!("{} books to bring across:", syncable.len());
        for b in &syncable {
            println!("{}", device_line(b));
        }
        println!();
        println!(
            "  all of them : readingbuddy ko sync {} --all",
            path.display()
        );
        println!(
            "  or one      : readingbuddy ko sync {} --book \"{}\"",
            path.display(),
            syncable[0].display_title()
        );
        return Ok(());
    }

    let chosen: Vec<&DeviceBook> = if all {
        syncable.clone()
    } else {
        select(&syncable, selectors)?
    };

    let paths: Vec<PathBuf> = chosen.iter().map(|b| b.path.clone()).collect();
    for report in engine.sync_device(&paths).await? {
        for w in &report.warnings {
            eprintln!("warning: {w}");
        }
        println!("{}", stats_line(&report.stats, ""));
    }
    Ok(())
}

/// Which mount to work on: the one given, else the one plugged in.
fn resolve_mount(path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = path {
        return Ok(p.to_path_buf());
    }
    let mut mounts = readingbuddy::candidate_mounts();
    match mounts.len() {
        0 => bail!(
            "no mounted KOReader device found (looked under /Volumes, /run/media/$USER and \
             /media/$USER). Pass the path to scan a library directory instead."
        ),
        1 => Ok(mounts.remove(0)),
        _ => {
            // Picking one for the user would be a guess about which reader they
            // meant, and both are plugged in.
            let list: Vec<String> = mounts.iter().map(|m| m.display().to_string()).collect();
            bail!(
                "several KOReader devices are mounted; name one:\n  {}",
                list.join("\n  ")
            )
        }
    }
}

/// Resolve `--book` selectors against the scanned rows: a case-insensitive
/// fragment of the title, or of the sidecar path.
fn select<'a>(books: &[&'a DeviceBook], selectors: &[String]) -> Result<Vec<&'a DeviceBook>> {
    let mut chosen: Vec<&DeviceBook> = Vec::new();
    for want in selectors {
        let needle = want.to_lowercase();
        let hits: Vec<&&DeviceBook> = books
            .iter()
            .filter(|b| {
                b.display_title().to_lowercase().contains(&needle)
                    || b.path.to_string_lossy().to_lowercase().contains(&needle)
            })
            .collect();
        // Silently syncing nothing, or the wrong book, is worse than stopping.
        match hits.len() {
            0 => bail!("no book to sync matches '{want}'"),
            _ => {
                for b in hits {
                    if !chosen.iter().any(|c| c.path == b.path) {
                        chosen.push(b);
                    }
                }
            }
        }
    }
    Ok(chosen)
}

fn device_line(b: &DeviceBook) -> String {
    let detail = match &b.state {
        DeviceState::Updated {
            new_highlights,
            refreshed,
        } => {
            let mut parts = Vec::new();
            if *new_highlights > 0 {
                parts.push(format!("{new_highlights} new"));
            }
            if *refreshed > 0 {
                parts.push(format!("{refreshed} edited on the device"));
            }
            format!("  ({})", parts.join(", "))
        }
        DeviceState::New { .. } => String::new(),
        _ => String::new(),
    };
    let progress = b
        .ko_percent
        .map(|p| format!("  [{:.0}%]", p * 100.0))
        .unwrap_or_default();
    let authors = b
        .authors
        .as_deref()
        .map(|a| format!(" — {}", a.replace('\n', ", ")))
        .unwrap_or_default();
    format!(
        "  {:<10} {}{authors}{progress}{detail}",
        b.state.label(),
        b.display_title()
    )
}

fn stats_line(s: &BookImportStats, mode: &str) -> String {
    format!(
        "{}{mode}: {} new, {} updated from the device, {} already known, {} flashcard candidates \
         (matched by {})",
        s.book_title, s.inserted, s.updated, s.skipped, s.flashcards, s.matched_by
    )
}

fn print_candidates(candidates: &[MatchCandidate]) {
    for c in candidates {
        println!(
            "    maybe #{}: {} ({:.0}%)",
            c.book_id,
            c.title,
            c.score * 100.0
        );
    }
}
