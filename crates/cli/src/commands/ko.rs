use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use readingbuddy::{
    BookImportStats, DeviceBook, DeviceState, Engine, MatchCandidate, PluginCondition,
};

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
    // `--all` is `sync_mount` and takes **the whole function**, before this
    // command scans anything itself. Three reasons, and the first two were
    // found by running it. It is the only verb that knows the mount and so the
    // only one that can stamp `last_synced_at` (migration `0020`); scanning
    // here first and calling it afterwards prints every warning **twice**,
    // since it scans too; and an early return on "nothing to sync" placed
    // above it skipped the stamp entirely, so a reader you were fully up to
    // date with could never record that it was — the CLI and the engine
    // disagreeing about a column whose whole job is to be the same on both.
    //
    // A per-book `--book` pull deliberately does *not* stamp: it leaves the
    // question that column answers — *is this reader's reading here* —
    // unchanged.
    if all {
        let done = engine.sync_mount(path).await?;
        for w in &done.warnings {
            eprintln!("warning: {w}");
        }
        for report in &done.reports {
            for w in &report.warnings {
                eprintln!("warning: {w}");
            }
            println!("{}", stats_line(&report.stats, ""));
        }
        // Two sentences, because they are two facts: a reader you have read
        // nothing on and a reader you are up to date with are not the same
        // picture, and one "nothing to sync" renders them identically.
        if done.synced == 0 {
            if done.found == 0 {
                // Not "nothing to read **yet**". `docs/decisions.md` bans
                // completion framing and the GUI asserts the word away by name;
                // this is a fact about a volume, not a thing the reader owes.
                println!("readingbuddy found no books on {}.", path.display());
            } else {
                println!(
                    "nothing to sync — everything on {} is already here.",
                    path.display()
                );
            }
        }
        return Ok(());
    }

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
    if selectors.is_empty() {
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

    let chosen: Vec<&DeviceBook> = select(&syncable, selectors)?;

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

/// `ko stats` — measured reading time out of the device's own
/// `statistics.sqlite3` (item 31).
///
/// **Its own verb, and not part of `sync`.** `docs/decisions.md` makes arrival
/// read-only, and a scan that quietly began importing months of timing data
/// would not be read-only in spirit even though every byte written is ours.
///
/// The counts are printed even when nothing was imported, because
/// `books_in_db` against `books_matched` is the whole answer to "why did this
/// import so little" — a device full of books none of which the library holds
/// is a linking problem, and a device with no statistics database at all is a
/// plugin that was never enabled. Those need different next moves, so they must
/// not print the same line.
pub async fn stats(engine: &Engine, path: &Path) -> Result<()> {
    let report = engine.import_device_statistics(path).await?;

    for w in &report.warnings {
        // Absence is ordinary here: no database, an unknown schema, a book the
        // library does not hold. None of it is an error and none of it stops
        // the rest, so these are warnings and the report below still prints.
        eprintln!("warning: {w}");
    }

    match report.schema_version {
        Some(v) => println!("{}: statistics database, schema {v}", path.display()),
        // Not "0 books": there was nothing to read, which is a different fact
        // from a database that held nothing.
        None => {
            println!("{}: no statistics database here.", path.display());
            println!("    enable KOReader's statistics plugin on the device, then read a page.");
            return Ok(());
        }
    }

    println!(
        "  {} of {} books are in your library",
        report.books_matched, report.books_in_db
    );
    println!("  {} days measured", report.days);
    println!(
        "  {} new, {} changed",
        report.events.inserted, report.events.updated
    );
    if report.books_in_db > 0 && report.books_matched == 0 {
        // A linking problem wearing the shape of an empty import. `ko scan`
        // is what shows which books the library does not hold.
        println!("    nothing here matches a book you have.");
        println!("    readingbuddy ko scan {}", path.display());
    }
    Ok(())
}

// ---- the plugin (item 15a) -------------------------------------------------

/// What is on the reader, and — when no reader is named — every reader we have
/// ever paired with.
///
/// The second half is the point of the `paired_devices` table: a reader in a
/// bag is still paired, and an answer that could only be produced by walking a
/// mount would have no way to say so.
pub async fn plugin_status(engine: &Engine, path: Option<&Path>) -> Result<()> {
    if path.is_none() && readingbuddy::candidate_mounts().is_empty() {
        let paired = engine.paired_devices().await?;
        if paired.is_empty() {
            println!("no reader is plugged in, and none has been paired.");
            println!("    plug one in, then: readingbuddy ko plugin install");
            return Ok(());
        }
        println!("no reader is plugged in. paired readers:");
        for d in &paired {
            println!("  {}  {}", device_name(d), d.device_id);
            // Where and **when**, said apart. This line used to read "last seen
            // at <path>", labelling a place with a time word — and until item
            // 55 there was no honest date to print anyway, because
            // `last_seen_at` moved on install alone.
            println!(
                "      plugin v{}, last plugged in at {}",
                d.plugin_version,
                d.last_mount_path.as_deref().unwrap_or("an unknown path"),
            );
            println!(
                "      last in your hands: {}",
                match d.last_seen_at {
                    Some(t) => crate::render::date(t),
                    None => "not since readingbuddy started recording it".to_string(),
                }
            );
            // *Not since we started recording* rather than *never*: migration
            // `0020` arrived with no back-fill, because nothing recorded which
            // device a past sync read from.
            println!(
                "      everything brought across: {}",
                match d.last_synced_at {
                    Some(t) => crate::render::date(t),
                    None => "not since readingbuddy started recording it".to_string(),
                }
            );
        }
        println!();
        println!("    plug one in : readingbuddy ko sync <mount> --all");
        println!("    sold it?    : readingbuddy ko plugin forget <device-id>");
        return Ok(());
    }

    let mount = resolve_mount(path)?;
    let status = engine.plugin_status(&mount).await?;
    println!("reader     : {}", status.mount.display());
    println!("plugin dir : {}", status.plugin_dir.display());

    // The verdict is the engine's (item 55) and the words are ours. This used
    // to compare `installed_version` against `our_version` here, which is the
    // second spelling of a domain rule item 17 exists to prevent — the GUI would
    // have been the third.
    match status.condition() {
        PluginCondition::Absent => println!(
            "installed  : no (this readingbuddy carries v{})",
            status.our_version
        ),
        PluginCondition::Current => println!(
            "installed  : v{} (up to date)",
            status.installed_version.unwrap_or(status.our_version)
        ),
        PluginCondition::Upgradable => println!(
            "installed  : v{} (this readingbuddy carries v{})",
            status.installed_version.unwrap_or_default(),
            status.our_version
        ),
        PluginCondition::Unversioned => println!("installed  : yes, but it carries no version"),
        // Which obstruction it is comes from the three lines below, which is
        // why this one names none of them.
        PluginCondition::Obstructed => match status.installed_version {
            Some(v) if v > status.our_version => println!(
                "installed  : v{v} — newer than this readingbuddy (v{})",
                status.our_version
            ),
            Some(v) => println!("installed  : v{v}"),
            None => println!("installed  : yes"),
        },
    }

    match (&status.device_id, status.paired) {
        (Some(id), true) => println!("paired     : yes, as {id}"),
        // A reader that says it is paired with a readingbuddy that is not this
        // one. Reinstalling is the whole repair, so say that rather than
        // reporting a state.
        (Some(id), false) => {
            println!("paired     : with another readingbuddy, as {id}");
            println!("    take it over: readingbuddy ko plugin install");
        }
        (None, _) => println!("paired     : no"),
    }

    for m in &status.modified {
        println!("edited here: {m} — readingbuddy will not overwrite or remove it");
    }
    for u in &status.unrecognised {
        println!("not ours   : {u} — readingbuddy will not remove it");
    }
    if !status.installed && !status.is_obstructed() {
        println!("    install it : readingbuddy ko plugin install");
    }
    Ok(())
}

/// Install or upgrade the plugin. Explicit, never automatic, and it prints
/// where it is about to write before it writes — `docs/decisions.md` requires
/// the path to be shown, and a path shown afterwards is not the same promise.
pub async fn plugin_install(engine: &Engine, path: Option<&Path>, yes: bool) -> Result<()> {
    let mount = resolve_mount(path)?;
    let status = engine.plugin_status(&mount).await?;

    // Three verbs, not two. A reinstall of the same version is not an upgrade,
    // and saying so on a device that already has v1 reads as if readingbuddy
    // had something newer to give it.
    println!(
        "{} readingbuddy's plugin into:",
        match () {
            _ if status.is_version_upgrade() => "upgrade",
            _ if status.is_reinstall() => "reinstall",
            _ => "install",
        }
    );
    println!("    {}", status.plugin_dir.display());
    println!(
        "nothing else on the reader is written to, and `ko plugin uninstall` removes exactly this."
    );
    if !yes && !confirm()? {
        println!("nothing was written.");
        return Ok(());
    }

    let report = engine.install_plugin(&mount).await?;
    for f in &report.written {
        println!("wrote {}", report.plugin_dir.join(f).display());
    }
    match report.upgraded_from {
        Some(from) if from < report.version => println!(
            "upgraded v{from} → v{}, paired as {}",
            report.version, report.device_id
        ),
        Some(_) => println!(
            "reinstalled v{}, still paired as {}",
            report.version, report.device_id
        ),
        None => println!(
            "installed v{}, paired as {}",
            report.version, report.device_id
        ),
    }
    println!("the reader has no address for readingbuddy yet, so it sends nothing.");
    Ok(())
}

/// Remove exactly what we installed, and forget the pairing.
pub async fn plugin_uninstall(engine: &Engine, path: Option<&Path>) -> Result<()> {
    let mount = resolve_mount(path)?;
    let report = engine.uninstall_plugin(&mount).await?;
    if report.removed.is_empty() {
        println!("nothing of readingbuddy's was on {}.", mount.display());
        return Ok(());
    }
    for f in &report.removed {
        println!("removed {}", report.plugin_dir.join(f).display());
    }
    if let Some(id) = &report.forgot_device {
        println!("forgot the pairing with {id}.");
    }
    Ok(())
}

/// A y/n on stdin. Not a prompt library: this is the only question the CLI
/// asks, and it asks it because the alternative is writing to somebody's
/// hardware without saying so.
fn confirm() -> Result<bool> {
    use std::io::Write as _;
    print!("go ahead? [y/N] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes"))
}

/// What to call a reader: its name, else the head of its id.
///
/// The fallback is short on purpose — a full uuid in a column of names reads as
/// noise, and `plugin_status` prints the whole id on the line beneath, which is
/// what `rename`/`forget` actually take.
fn device_name(d: &readingbuddy::PairedDevice) -> &str {
    match d.label.as_deref() {
        Some(l) => l,
        None => &d.device_id[..8.min(d.device_id.len())],
    }
}

/// Resolve a device id the user typed, allowing a unique prefix.
///
/// Uuids are not memorable and `ko plugin status` prints them in full beside a
/// name; asking for all 32 characters to rename something would make the verb
/// unusable without a clipboard. Ambiguity is a refusal that lists the
/// candidates rather than picking one.
async fn resolve_device(engine: &Engine, typed: &str) -> Result<readingbuddy::PairedDevice> {
    let paired = engine.paired_devices().await?;
    let hits: Vec<readingbuddy::PairedDevice> = paired
        .into_iter()
        .filter(|d| d.device_id.starts_with(typed))
        .collect();
    match hits.len() {
        1 => Ok(hits.into_iter().next().expect("one")),
        0 => anyhow::bail!(
            "no paired reader starts with \"{typed}\".\n    \
             which ones there are: readingbuddy ko plugin status"
        ),
        _ => {
            let names: Vec<String> = hits
                .iter()
                .map(|d| format!("{} ({})", d.device_id, device_name(d)))
                .collect();
            anyhow::bail!(
                "\"{typed}\" names {} readers: {}.\n    \
                 type more of the id",
                hits.len(),
                names.join(", ")
            )
        }
    }
}

/// Give a reader a name (item 55).
pub async fn plugin_rename(engine: &Engine, typed: &str, label: &str) -> Result<()> {
    let device = resolve_device(engine, typed).await?;
    let was = device_name(&device).to_string();
    engine.rename_device(&device.device_id, label).await?;

    // Re-read rather than echoing what was sent: a blank clears the name, and
    // what comes back is then the fallback rather than the empty string.
    let now = engine.paired_devices().await?;
    let now = now
        .iter()
        .find(|d| d.device_id == device.device_id)
        .expect("just renamed");
    if now.label.is_none() {
        println!(
            "{was} has no name of its own again — it is {}.",
            device_name(now)
        );
    } else {
        println!("{was} is now {}.", device_name(now));
    }
    Ok(())
}

/// Forget a reader we do not have in hand (item 55).
///
/// The second line is not decoration. This drops our half of the pairing and
/// cannot reach the device, so the plugin and the token stay where they are —
/// and a message that said only *forgotten* would leave somebody believing a
/// reader they lent out had been cleaned.
pub async fn plugin_forget(engine: &Engine, typed: &str) -> Result<()> {
    let device = resolve_device(engine, typed).await?;
    let name = device_name(&device).to_string();
    engine.forget_device(&device.device_id).await?;
    println!("forgotten: {name}.");
    println!(
        "    the plugin is still on that reader — readingbuddy cannot reach it from here.\n    \
         plugged in? this removes it: readingbuddy ko plugin uninstall"
    );
    Ok(())
}
