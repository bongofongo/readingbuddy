//! Noticing, without being asked — a reader arriving, and a note changing under
//! us.
//!
//! Two watchers live here, and they are here together because the second one
//! departs from a rule the first one made. Reading only one of them is how that
//! rule gets re-broken by accident.
//!
//! # What both share
//!
//! **The event source is injected, never hard-wired.** A [`MountWatcher`] is
//! driven by a channel of [`MountStir`]s and a [`VaultWatcher`] by a channel of
//! [`VaultStir`]s; [`watch_mounts`] and [`watch_vault`] are the adapters that
//! fill those channels from `notify`, and are the only parts of the file that
//! cannot run in CI. A watcher that can only be driven by plugging in real
//! hardware — or by racing a real text editor — is a watcher with no tests, and
//! the debounce is exactly the part that has to be tested.
//!
//! **The debounce is the substance.** A change is never one event. A mount is a
//! burst for as long as the reader keeps flushing; a save from an editor is a
//! temp file, a rename and a touch. Acting on the first event reads a file that
//! is still being written.
//!
//! **Neither is a requirement.** Both constructors fail when the platform has
//! no notification service to offer, and a caller degrades around that rather
//! than aborting: an app that cannot notice a mount is the app we had
//! yesterday, and an app that cannot watch the vault still has
//! [`crate::Engine::reconcile_vault`].
//!
//! # Where they part, and why
//!
//! **[`MountWatcher`] may scan; it may not sync.** `docs/decisions.md` is
//! explicit that mount → import is automatic and read-only while anything that
//! *writes* to the device is explicit and shows the path. That watcher
//! therefore holds no [`crate::storage::Storage`] at all: it announces arrivals
//! and departures, and what the frontend does about one is the frontend's
//! decision.
//!
//! **[`VaultWatcher`] holds one, and writes.** That is a deliberate departure,
//! ruled on in item 24, and the argument is that the rule above was never about
//! watchers writing. It is about **consent**: a mounted reader is somebody
//! else's disk, and a cable is not permission to modify it. The vault is ours,
//! in our own data directory, and the write in question is not a write to the
//! user's notes at all — it is a *derived index* catching up with the file that
//! was already the origin of its content. Re-deriving a cache from its source is
//! not a sync; it is the cache being correct.
//!
//! So the rule is preserved rather than abandoned, by being stated about the
//! thing each watcher watches:
//!
//! - **`MountWatcher` never writes to a device.**
//! - **`VaultWatcher` never writes to the vault.**
//!
//! The second is an invariant of this module, not a habit: nothing below opens
//! a note file for writing, and `never_writes_to_the_vault` asserts it over a
//! whole tree of files. Everything a `VaultWatcher` can do to the database is
//! recomputable from the vault by [`crate::Engine::reconcile_vault`], which is
//! the property that makes the departure cheap to be wrong about.
//!
//! And each holds exactly what its consequence needs. A mount's consequence is
//! a *decision* — scan this, ignore that, ask the user — so the frontend must
//! be the one to make it, and a `Storage` here would be an invitation. A file
//! edit's consequence is a *re-derivation* with no decision in it, the same
//! answer for every frontend, so putting it anywhere else means three frontends
//! computing it and one of them getting it wrong silently.
//!
//! **No task is ever spawned.** Both watchers are pull-driven: nothing happens
//! until the caller polls `next()`, and for the vault that includes the write.
//! The engine grows no background thread, no detached runtime work and no
//! writer the caller cannot see, which answers most of what "a background task
//! that writes to the database" would otherwise have to answer — there is no
//! second writer racing a foreground import, because the refresh runs on the
//! caller's own task and takes the pool the ordinary way.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::device::{mount_roots, offers_reader};
use crate::error::{EngineError, Result};
use crate::notes;
use crate::storage::Storage;

/// How long a volume has to hold still before it is announced.
///
/// A mount is not one event: it is a burst, and a reader that is still flushing
/// will hand over a sidecar mid-write for as long as it takes. Scanning on the
/// first event reads that half-written file and reports it `Unreadable`.
///
/// A `const` and not a config knob, for the same reason `PROVIDER_TIMEOUT` is
/// one: [`MountWatcher::quiet_for`] exists so the tests can collapse it, and
/// making it a user setting would mean the tested value and the shipped value
/// could differ.
pub const MOUNT_QUIET: Duration = Duration::from_secs(2);

/// How many raw stirs may queue before the oldest are dropped.
///
/// Dropping is safe here and nowhere else in the codebase: every stir means only
/// "look at this path again", the debounce coalesces a burst into one look
/// regardless, and a settle re-reads the filesystem rather than trusting
/// anything the event carried.
const STIR_CAPACITY: usize = 256;

/// A raw "something happened at this path" from the source.
///
/// The path is a **mount** (`/Volumes/KOBOeReader`) or one of the mount roots
/// themselves (`/Volumes`) — never a file deep inside a volume. Normalizing to
/// that is the adapter's job, because the adapter is what knows which roots it
/// asked to be told about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountStir(pub PathBuf);

/// What the watcher announces, once a volume has settled and been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountEvent {
    /// A volume holding a KOReader install is here, and was not before.
    Arrived(PathBuf),
    /// A volume we had announced is gone.
    Departed(PathBuf),
}

impl MountEvent {
    pub fn path(&self) -> &Path {
        match self {
            MountEvent::Arrived(p) | MountEvent::Departed(p) => p,
        }
    }
}

/// Debounces raw filesystem stirs into readers arriving and leaving.
///
/// Cancel-safe: [`MountWatcher::next`] holds no state of its own, so a
/// `tokio::select!` that drops it mid-wait resumes the same deadlines and the
/// same already-decided verdicts on the next call. It is dropped that way on
/// every keypress in the TUI's event loop, so this is not a theoretical claim.
pub struct MountWatcher {
    stirs: mpsc::Receiver<MountStir>,
    quiet: Duration,
    /// Paths whose burst has not finished, and when it will have.
    settling: HashMap<PathBuf, Instant>,
    /// Mounts already announced. This is what makes a second stir about a volume
    /// that is still plugged in cost nothing — one arrival per arrival, however
    /// many events the platform decided to send.
    present: HashSet<PathBuf>,
    /// Decided, not yet handed out. `next` returns one event at a time and a
    /// single settle can decide several, so the surplus waits here rather than
    /// being lost to the next cancellation.
    decided: VecDeque<MountEvent>,
    /// The directories being watched, if any. A stir *at* a root (rather than at
    /// a volume inside it) cannot name which volume changed, so it is expanded
    /// against the filesystem when it settles.
    roots: Vec<PathBuf>,
    /// Keeps the platform watcher alive for as long as anyone holds this. Its
    /// callback owns the sender; dropping it is what closes the channel.
    _source: Option<notify::RecommendedWatcher>,
}

impl MountWatcher {
    /// A watcher driven by a channel — the seam every test uses.
    pub fn from_stirs(stirs: mpsc::Receiver<MountStir>) -> Self {
        MountWatcher {
            stirs,
            quiet: MOUNT_QUIET,
            settling: HashMap::new(),
            present: HashSet::new(),
            decided: VecDeque::new(),
            roots: Vec::new(),
            _source: None,
        }
    }

    /// Volumes that were already mounted when we started looking.
    ///
    /// Seeded rather than announced: the watcher reports *transitions*, and a
    /// reader that was plugged in before the app started is not one. A caller
    /// that wants to act on those has [`crate::candidate_mounts`], which is
    /// where it got this list.
    pub fn already_here(mut self, mounts: impl IntoIterator<Item = PathBuf>) -> Self {
        self.present.extend(mounts);
        self
    }

    /// Directories whose own stirs should be expanded to the volumes inside
    /// them.
    pub fn under_roots(mut self, roots: impl IntoIterator<Item = PathBuf>) -> Self {
        self.roots.extend(roots);
        self
    }

    /// Shorten the quiet period. Tests only — see [`MOUNT_QUIET`].
    pub fn quiet_for(mut self, quiet: Duration) -> Self {
        self.quiet = quiet;
        self
    }

    /// Is this volume one we have announced and not yet seen leave?
    pub fn is_present(&self, mount: &Path) -> bool {
        self.present.contains(mount)
    }

    /// The next reader to arrive or leave, or `None` once the source is gone and
    /// everything still settling has been decided.
    pub async fn next(&mut self) -> Option<MountEvent> {
        loop {
            if let Some(event) = self.decided.pop_front() {
                return Some(event);
            }
            let Some(deadline) = self.settling.values().min().copied() else {
                // Nothing is settling, so there is nothing to wake up for.
                let stir = self.stirs.recv().await?;
                self.stir(stir);
                continue;
            };
            // `timeout_at` rather than a `select!`: the deadline is a property of
            // the watcher, not of this call, so there is nothing to race that a
            // cancelled call would take with it.
            match tokio::time::timeout_at(deadline, self.stirs.recv()).await {
                Ok(Some(stir)) => self.stir(stir),
                // The source is gone, but a volume that arrived a moment before
                // it died still arrived. Wait the burst out and decide it; the
                // empty `settling` on the next pass is what returns `None`.
                Ok(None) => {
                    tokio::time::sleep_until(deadline).await;
                    self.settle();
                }
                Err(_) => self.settle(),
            }
        }
    }

    /// Arm — or re-arm — the quiet period for a path.
    fn stir(&mut self, MountStir(path): MountStir) {
        self.settling.insert(path, Instant::now() + self.quiet);
    }

    /// Decide every path whose quiet period has run out.
    fn settle(&mut self) {
        let now = Instant::now();
        // Ordered by when each burst finished, so two devices plugged in one
        // after the other are announced in that order rather than in whatever
        // order the map iterated. Ties break on the path, which is arbitrary but
        // at least the same arbitrary every time.
        let mut due: Vec<(Instant, PathBuf)> = self
            .settling
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(path, at)| (*at, path.clone()))
            .collect();
        due.sort();

        for (_, path) in due {
            self.settling.remove(&path);
            if self.roots.contains(&path) {
                for volume in self.volumes_under(&path) {
                    self.decide(volume);
                }
            } else {
                self.decide(path);
            }
        }
    }

    /// Compare what is on disk with what we have announced, and record the
    /// difference. A stir about a volume whose state has not changed — which is
    /// most of them — decides nothing.
    fn decide(&mut self, mount: PathBuf) {
        match (offers_reader(&mount), self.present.contains(&mount)) {
            (true, false) => {
                tracing::info!(mount = %mount.display(), "reader mounted");
                self.present.insert(mount.clone());
                self.decided.push_back(MountEvent::Arrived(mount));
            }
            (false, true) => {
                tracing::info!(mount = %mount.display(), "reader unmounted");
                self.present.remove(&mount);
                self.decided.push_back(MountEvent::Departed(mount));
            }
            _ => {}
        }
    }

    /// Everything worth deciding under a root: what is there now, plus what we
    /// have announced and can therefore no longer see.
    ///
    /// The second half is the point. A stir at the root is all some platforms
    /// give for an *unmount*, and by the time it settles the directory it names
    /// is already gone from `read_dir` — so a departure derived only from what
    /// is on disk is a departure that never fires.
    fn volumes_under(&self, root: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        out.extend(
            self.present
                .iter()
                .filter(|mount| mount.parent() == Some(root))
                .cloned(),
        );
        out.sort();
        out.dedup();
        out
    }
}

/// Watch the platform's mount directories for readers arriving and leaving.
///
/// Seeded with whatever is already mounted, so the first thing it announces is a
/// change rather than the status quo. Fails when the platform has no
/// notification service to offer — which a caller should degrade around, not
/// abort on: an app that cannot notice a mount is the app we had yesterday.
pub fn watch_mounts() -> Result<MountWatcher> {
    let roots = mount_roots();
    if roots.is_empty() {
        return Err(EngineError::Watch(
            "no mount directory exists to watch on this machine".into(),
        ));
    }
    let (tx, rx) = mpsc::channel(STIR_CAPACITY);
    let watched = roots.clone();

    let mut source = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else {
            // A dropped or errored event is not worth failing over: the next
            // stir re-reads the filesystem anyway, and so does the next scan.
            return;
        };
        for path in event.paths {
            let Some(stir) = normalize(&path, &watched) else {
                continue;
            };
            // Full means a burst is already queued, and a burst coalesces into
            // one look regardless. See `STIR_CAPACITY`.
            let _ = tx.try_send(MountStir(stir));
        }
    })
    .map_err(|e| EngineError::Watch(e.to_string()))?;

    for root in &roots {
        // **Non-recursive, and this is load-bearing.** `/Volumes` recursively is
        // every file on every mounted disk; on a reader that is the whole
        // library, and on macOS it is the boot volume too.
        notify::Watcher::watch(&mut source, root, notify::RecursiveMode::NonRecursive)
            .map_err(|e| EngineError::Watch(format!("{}: {e}", root.display())))?;
    }

    tracing::info!(roots = ?roots, "watching for readers");
    Ok(MountWatcher {
        _source: Some(source),
        ..MountWatcher::from_stirs(rx)
            .already_here(crate::device::candidate_mounts())
            .under_roots(roots)
    })
}

/// Reduce an event path to the volume it belongs to, or to the root itself.
///
/// Everything below a volume is that volume's business — a page turn rewrites a
/// sidecar, and a reader that stirs on every page turn is a reader that is
/// rescanned on every page turn.
fn normalize(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    for root in roots {
        if path == root {
            return Some(root.clone());
        }
        if let Ok(rest) = path.strip_prefix(root)
            && let Some(first) = rest.components().next()
        {
            return Some(root.join(first));
        }
    }
    None
}

// ---- the vault ------------------------------------------------------------

/// How long a note file has to hold still before it is re-indexed.
///
/// Much shorter than [`MOUNT_QUIET`], and for a reason rather than by taste: a
/// mount is a device flushing for as long as it takes, while a save is one
/// editor writing one small file — the burst to coalesce is a temp file, a
/// rename and a touch, which is milliseconds. It has to stay short because the
/// thing waiting on it is a search box: a note edited and then searched for is
/// the case this whole item exists to fix, and two seconds of "the note is
/// gone" is the bug wearing a different hat.
///
/// A `const` and not a config knob, for [`MOUNT_QUIET`]'s reason.
pub const VAULT_QUIET: Duration = Duration::from_millis(400);

/// A raw "something happened at this file" from the source.
///
/// A path to a **file**, unlike [`MountStir`]: a vault is our own bounded
/// directory, so there is no reduce-to-the-volume step and no reason for one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultStir(pub PathBuf);

/// What the watcher announces, once a note file has settled and been read.
///
/// Carries the note id and **not the path**: a note's filename is its slugified
/// title, and `derive_title` takes that from the first six words of the body.
/// A vault path is the user's private reading, so it is not put where a
/// frontend will be tempted to log it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultEvent {
    /// A note's index caught up with a change made outside readingbuddy.
    Reindexed { note_id: i64 },
    /// A note's file is not there any more. **Nothing was written** — see
    /// [`VaultWatcher`]'s ruling on absence.
    Vanished { note_id: i64 },
}

impl VaultEvent {
    pub fn note_id(&self) -> i64 {
        match self {
            VaultEvent::Reindexed { note_id } | VaultEvent::Vanished { note_id } => *note_id,
        }
    }
}

/// Follows the vault, so a note edited in another program is a note the index
/// still knows how to find.
///
/// # The three races
///
/// **We wrote it.** `create_note` and `update_note_body` write the file, so the
/// watcher sees its own writes. It is not a loop and cannot become one, by
/// construction: the only thing this watcher writes is the *database*, and the
/// database is not watched — so the echo is one event deep however many notes
/// are saved. It is not even that expensive, because
/// [`notes::reindex_from_body`] compares what the file says against what is
/// already indexed and returns without a transaction when they agree. The same
/// comparison absorbs the editor that rewrites a file on focus loss whether or
/// not a character changed, which is the far more common event.
///
/// **A partial write.** Temp-file-and-rename (Obsidian, and vim by default) is
/// atomic and cannot be observed half-done. Truncate-in-place can. The debounce
/// is the first answer and is deliberately *not* claimed to be a sufficient
/// one — a write slower than the quiet period lands inside it — so the read
/// goes through [`settled_read`], which stats the file either side of reading
/// it and treats a length or mtime that moved as "still being written", putting
/// the path back into the debounce instead of indexing what it got. The
/// remaining hole is honest and stated: a write that changes neither length nor
/// mtime between the two stats is indistinguishable from a file at rest.
///
/// **Deleted and recreated, and the ruling on it: absence is never
/// destructive.** A file that is not there produces [`VaultEvent::Vanished`]
/// and **writes nothing at all** — the note row, its FTS entry, its edges and
/// its citations all stay. Four reasons, in increasing order of how much they
/// decide it. `docs/decisions.md` gives the vault the courtesy of being
/// editable by other tools, and other tools move files: Obsidian deletes into
/// `.trash/`, a `git checkout` removes and restores, a sync client resolves a
/// conflict by doing both. Deletion already *has* a path here —
/// [`crate::Engine::delete_note`], which removes the row and the file together
/// — so a second, implicit one adds nothing but ways to lose data. The row
/// holds more than the file ever did (`book_id`, `reading_id`, `highlight_id`,
/// page, location, citations, and every *inbound* edge), so an absence taken as
/// a deletion destroys things the returning file could not bring back. And the
/// asymmetry settles it: believe a deletion wrongly and something is gone;
/// believe a persistence wrongly and the user sees a search hit for a note whose
/// file has moved — visible, recoverable, and fixable by the command that
/// exists for it. Recreation then needs no case of its own at all: the row was
/// never removed, so a file that comes back is simply a file that changed.
///
/// # Cancel-safety
///
/// [`VaultWatcher::next`] is dropped mid-await by every `select!` that races
/// it, exactly as [`MountWatcher::next`] is. Two things make that safe. Every
/// deadline lives in the struct rather than in the future. And a path is
/// removed from `settling` only **after** its refresh has committed — so a drop
/// during the write leaves the path armed with a deadline already in the past,
/// and the next call retries it immediately. The write itself is one
/// transaction ([`crate::storage::Storage::reindex_note`]), so a drop inside it
/// rolls back rather than leaving a note whose body and whose graph edges came
/// from different versions of the file.
pub struct VaultWatcher {
    stirs: mpsc::Receiver<VaultStir>,
    quiet: Duration,
    /// Files whose burst has not finished, and when it will have.
    settling: HashMap<PathBuf, Instant>,
    /// Decided, not yet handed out — [`MountWatcher`]'s reason exactly.
    decided: VecDeque<VaultEvent>,
    vault: PathBuf,
    storage: Storage,
    /// Keeps the platform watcher alive for as long as anyone holds this.
    _source: Option<notify::RecommendedWatcher>,
}

impl VaultWatcher {
    /// A watcher driven by a channel — the seam every test uses.
    pub fn from_stirs(
        vault: impl Into<PathBuf>,
        storage: Storage,
        stirs: mpsc::Receiver<VaultStir>,
    ) -> Self {
        VaultWatcher {
            stirs,
            quiet: VAULT_QUIET,
            settling: HashMap::new(),
            decided: VecDeque::new(),
            vault: vault.into(),
            storage,
            _source: None,
        }
    }

    /// Shorten the quiet period. Tests only — see [`VAULT_QUIET`].
    pub fn quiet_for(mut self, quiet: Duration) -> Self {
        self.quiet = quiet;
        self
    }

    /// The next note whose index caught up with its file, or `None` once the
    /// source is gone and everything still settling has been decided.
    ///
    /// **The work happens here**, on the caller's task, when the caller polls.
    /// A frontend that holds one of these and never polls it gets no watching
    /// and no surprises; there is no thread doing this behind anyone's back.
    pub async fn next(&mut self) -> Option<VaultEvent> {
        loop {
            if let Some(event) = self.decided.pop_front() {
                return Some(event);
            }
            let Some(deadline) = self.settling.values().min().copied() else {
                let stir = self.stirs.recv().await?;
                self.stir(stir);
                continue;
            };
            match tokio::time::timeout_at(deadline, self.stirs.recv()).await {
                Ok(Some(stir)) => self.stir(stir),
                // A file saved a moment before the source died was still saved.
                Ok(None) => {
                    tokio::time::sleep_until(deadline).await;
                    self.settle().await;
                }
                Err(_) => self.settle().await,
            }
        }
    }

    /// Arm — or re-arm — the quiet period for a file.
    fn stir(&mut self, VaultStir(path): VaultStir) {
        self.settling.insert(path, Instant::now() + self.quiet);
    }

    /// Decide every file whose quiet period has run out.
    async fn settle(&mut self) {
        let now = Instant::now();
        let mut due: Vec<(Instant, PathBuf)> = self
            .settling
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(path, at)| (*at, path.clone()))
            .collect();
        due.sort();

        for (_, path) in due {
            match self.decide(&path).await {
                Ok(Decision::Settled(event)) => {
                    self.settling.remove(&path);
                    if let Some(event) = event {
                        self.decided.push_back(event);
                    }
                }
                // The file moved while we were reading it. Back into the
                // debounce rather than indexing half of it.
                Ok(Decision::StillWriting) => {
                    self.settling.insert(path, Instant::now() + self.quiet);
                }
                Err(e) => {
                    // Dropped rather than re-armed: a path that fails for a
                    // durable reason and re-arms itself is a spin, and the
                    // sweep at the next start is what recovers from it.
                    // The error, never the path — a vault path is prose.
                    tracing::warn!(error = %e, "could not re-index a note from the vault");
                    self.settling.remove(&path);
                }
            }
        }
    }

    /// What to do about one settled file.
    async fn decide(&self, path: &Path) -> Result<Decision> {
        // Not markdown, not under the vault, or a file no note row claims.
        // **Unclaimed files are not adopted** — a note readingbuddy did not
        // write is not a note it owns, and creating rows for stray files would
        // invent notes out of whatever an editor left lying in the directory.
        let Some(rel) = notes::vault_relative(&self.vault, path) else {
            return Ok(Decision::Settled(None));
        };
        let Some(note) = self.storage.note_by_path(&rel).await? else {
            return Ok(Decision::Settled(None));
        };

        let body = match settled_read(path)? {
            Settled::Gone => {
                tracing::trace!(note = note.id, "a note's file is not there");
                return Ok(Decision::Settled(Some(VaultEvent::Vanished {
                    note_id: note.id,
                })));
            }
            Settled::Moving => return Ok(Decision::StillWriting),
            Settled::Content(content) => content,
        };
        let (_, body) = notes::frontmatter_and_body(&body);

        if notes::reindex_from_body(&self.storage, note.id, &note.title, body).await? {
            Ok(Decision::Settled(Some(VaultEvent::Reindexed {
                note_id: note.id,
            })))
        } else {
            // Our own write coming back to us, or an editor's no-op save.
            Ok(Decision::Settled(None))
        }
    }
}

enum Decision {
    /// Done with this path; maybe with something to announce.
    Settled(Option<VaultEvent>),
    /// The file is still being written — wait the quiet period out again.
    StillWriting,
}

/// What a read of a note file found.
enum Settled {
    /// The file is not there. Not a deletion — see [`VaultWatcher`].
    Gone,
    /// The file changed while we were reading it.
    Moving,
    Content(String),
}

/// Read a file, and only believe it if the file held still across the read.
fn settled_read(path: &Path) -> Result<Settled> {
    settled_read_with(path, |p| std::fs::read_to_string(p))
}

/// [`settled_read`] with the read injected, which is the only way the
/// mid-write branch can be tested.
///
/// The module's own rule, applied to itself: a guard that can only be triggered
/// by racing a real text editor is a guard with no test, and this one exists
/// precisely for the case that is hard to arrange on purpose.
fn settled_read_with<R>(path: &Path, read: R) -> Result<Settled>
where
    R: FnOnce(&Path) -> std::io::Result<String>,
{
    let before = stamp(path);
    let content = match read(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settled::Gone),
        Err(e) => return Err(e.into()),
    };
    // `before` is `None` when the file appeared between the stat and the read,
    // which is the same answer: we do not yet have a version of it that stood
    // still.
    if before.is_none() || before != stamp(path) {
        return Ok(Settled::Moving);
    }
    Ok(Settled::Content(content))
}

/// Length and mtime — enough to notice a file being written, cheap enough to
/// take twice per read.
fn stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// Watch the vault for note files changed by other programs.
///
/// **Recursive, which is the opposite call [`watch_mounts`] makes**, and for
/// the reason that made that one non-recursive: the question is how big the
/// tree is. `/Volumes` recursively is every file on every mounted disk; the
/// vault recursively is one directory per book of the user's own notes, and a
/// non-recursive watch would see the book directories and none of the notes
/// inside them — which is every note there is.
///
/// Non-markdown paths are dropped in the callback rather than later:
/// `.obsidian/workspace.json` is rewritten constantly by a program that is not
/// editing a note, and a channel full of it is a channel with no room for the
/// save that mattered.
pub fn watch_vault(vault: &Path, storage: Storage) -> Result<VaultWatcher> {
    if !vault.is_dir() {
        return Err(EngineError::Watch(format!(
            "{}: no vault directory to watch",
            vault.display()
        )));
    }
    let (tx, rx) = mpsc::channel(STIR_CAPACITY);

    let mut source = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else {
            return;
        };
        for path in event.paths {
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            // Full means a burst is already queued, and the sweep is what
            // recovers a stir that was dropped. See `STIR_CAPACITY`.
            let _ = tx.try_send(VaultStir(path));
        }
    })
    .map_err(|e| EngineError::Watch(e.to_string()))?;

    notify::Watcher::watch(&mut source, vault, notify::RecursiveMode::Recursive)
        .map_err(|e| EngineError::Watch(format!("{}: {e}", vault.display())))?;

    // The vault's own path, which is a directory the user chose and not a note
    // title. Nothing below this line logs a path inside it above `trace!`.
    tracing::info!(vault = %vault.display(), "watching the vault");
    Ok(VaultWatcher {
        _source: Some(source),
        ..VaultWatcher::from_stirs(vault, storage, rx)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUIET: Duration = Duration::from_millis(100);

    /// A plausible KOReader install, so `offers_reader` says yes.
    fn install(mount: &Path) {
        let dir = mount.join("koreader");
        std::fs::create_dir_all(dir.join("frontend")).unwrap();
        std::fs::create_dir_all(dir.join("plugins")).unwrap();
        std::fs::write(dir.join("reader.lua"), "-- entry point\n").unwrap();
    }

    fn watcher() -> (mpsc::Sender<MountStir>, MountWatcher) {
        let (tx, rx) = mpsc::channel(16);
        (tx, MountWatcher::from_stirs(rx).quiet_for(QUIET))
    }

    /// What the watcher does *not* do before the quiet period, and what it does
    /// after it: one announcement for a burst of a dozen events.
    ///
    /// The burst is the ordinary case, not the pathological one — a mount is
    /// several events on every platform, and a reader that is still flushing
    /// keeps producing them. Scanning on the first one reads a half-written
    /// sidecar.
    #[tokio::test(start_paused = true)]
    async fn a_burst_of_events_becomes_one_arrival_and_not_before_it_stops() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("KOBOeReader");
        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);

        let (tx, mut watcher) = watcher();
        for _ in 0..12 {
            tx.send(MountStir(mount.clone())).await.unwrap();
        }

        // Still inside the quiet period: nothing has been decided, because the
        // device may still be writing.
        assert!(
            tokio::time::timeout(QUIET / 2, watcher.next())
                .await
                .is_err(),
            "announced before the burst had finished"
        );

        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(mount.clone())),
        );
        // And the other eleven events are not eleven more arrivals.
        assert!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .is_err(),
            "one mount, one arrival"
        );
    }

    /// Every event that keeps arriving pushes the deadline out, so a device that
    /// writes for a minute is scanned once, at the end.
    #[tokio::test(start_paused = true)]
    async fn a_device_that_keeps_writing_keeps_the_scan_waiting() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("Kindle");
        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);

        let (tx, mut watcher) = watcher();
        for _ in 0..5 {
            tx.send(MountStir(mount.clone())).await.unwrap();
            // Two thirds of the way through, then stirred again: a watcher that
            // armed once instead of re-arming would have fired by the third.
            assert!(
                tokio::time::timeout(QUIET * 2 / 3, watcher.next())
                    .await
                    .is_err()
            );
        }
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(mount)),
        );
    }

    /// The claim that keeps a scan from being started twice for one mount: a
    /// volume that is still plugged in is not news, however loudly the platform
    /// says so.
    #[tokio::test(start_paused = true)]
    async fn a_mount_that_is_still_there_is_never_announced_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("PocketBook");
        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);

        let (tx, mut watcher) = watcher();
        tx.send(MountStir(mount.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(mount.clone())),
        );

        // A second burst, minutes later, about the same still-mounted volume.
        tokio::time::advance(Duration::from_secs(60)).await;
        for _ in 0..3 {
            tx.send(MountStir(mount.clone())).await.unwrap();
        }
        assert!(
            tokio::time::timeout(QUIET * 4, watcher.next())
                .await
                .is_err(),
            "the same reader was announced twice"
        );
        assert!(watcher.is_present(&mount));
    }

    /// Unplugging is announced, and replugging starts over — the departure is
    /// what makes the volume newsworthy again.
    #[tokio::test(start_paused = true)]
    async fn unplugging_is_announced_and_replugging_arrives_again() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("KOBOeReader");
        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);

        let (tx, mut watcher) = watcher();
        tx.send(MountStir(mount.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(mount.clone())),
        );

        std::fs::remove_dir_all(&mount).unwrap();
        tx.send(MountStir(mount.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Departed(mount.clone())),
        );
        assert!(!watcher.is_present(&mount));

        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);
        tx.send(MountStir(mount.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(mount)),
        );
    }

    /// A USB stick is not a reader, and neither is a folder that merely shares
    /// the name — the same predicate that gates writing to a volume gates
    /// announcing one.
    #[tokio::test(start_paused = true)]
    async fn a_volume_without_koreader_on_it_is_never_announced() {
        let tmp = tempfile::tempdir().unwrap();
        let stick = tmp.path().join("BACKUP");
        std::fs::create_dir_all(stick.join("koreader")).unwrap();
        std::fs::write(stick.join("koreader/notes.txt"), "mine").unwrap();

        let (tx, mut watcher) = watcher();
        tx.send(MountStir(stick.clone())).await.unwrap();
        assert!(
            tokio::time::timeout(QUIET * 4, watcher.next())
                .await
                .is_err(),
            "a plain volume was offered as a reader"
        );
        assert!(!watcher.is_present(&stick));
    }

    /// A reader plugged in before the app started is not a transition, and
    /// announcing it as one would scan a device the caller has already been told
    /// about by `candidate_mounts`.
    #[tokio::test(start_paused = true)]
    async fn a_reader_that_was_already_here_is_not_an_arrival() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("Kindle");
        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);

        let (tx, rx) = mpsc::channel(16);
        let mut watcher = MountWatcher::from_stirs(rx)
            .quiet_for(QUIET)
            .already_here([mount.clone()]);

        tx.send(MountStir(mount.clone())).await.unwrap();
        assert!(
            tokio::time::timeout(QUIET * 4, watcher.next())
                .await
                .is_err()
        );
        // It is still watched, though: unplugging it is a transition.
        std::fs::remove_dir_all(&mount).unwrap();
        tx.send(MountStir(mount.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Departed(mount)),
        );
    }

    /// Two readers, plugged in one after the other, are announced in that order.
    #[tokio::test(start_paused = true)]
    async fn two_readers_are_announced_in_the_order_they_settled() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("Kindle");
        let second = tmp.path().join("KOBOeReader");
        for mount in [&first, &second] {
            std::fs::create_dir_all(mount).unwrap();
            install(mount);
        }

        let (tx, mut watcher) = watcher();
        tx.send(MountStir(first.clone())).await.unwrap();
        // Polled, so the first burst is actually armed before the second is
        // sent: a stir sitting unread in the channel has not started its quiet
        // period, and both would then settle in the same instant.
        assert!(
            tokio::time::timeout(QUIET / 4, watcher.next())
                .await
                .is_err()
        );
        tx.send(MountStir(second.clone())).await.unwrap();

        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(first)),
        );
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(second)),
        );
    }

    /// Some platforms report a mount only as a change to the directory holding
    /// it. That stir cannot name the volume, so it is expanded when it settles —
    /// against the filesystem for arrivals, and against what we have announced
    /// for departures, which `read_dir` can no longer see.
    #[tokio::test(start_paused = true)]
    async fn a_stir_at_the_root_finds_both_the_arrival_and_the_departure() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let mount = root.join("KOBOeReader");
        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);

        let (tx, rx) = mpsc::channel(16);
        let mut watcher = MountWatcher::from_stirs(rx)
            .quiet_for(QUIET)
            .under_roots([root.clone()]);

        tx.send(MountStir(root.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(mount.clone())),
        );

        std::fs::remove_dir_all(&mount).unwrap();
        tx.send(MountStir(root)).await.unwrap();
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Departed(mount)),
        );
    }

    /// The TUI drops this future on every keypress. Resuming it must not lose
    /// the burst that was mid-flight, or a device plugged in while the user is
    /// typing is a device that is never noticed.
    #[tokio::test(start_paused = true)]
    async fn cancelling_the_wait_does_not_lose_the_arrival() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("Kindle");
        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);

        let (tx, mut watcher) = watcher();
        tx.send(MountStir(mount.clone())).await.unwrap();
        for _ in 0..8 {
            // Each of these polls the future and then drops it, exactly as
            // `select!` does when a keypress wins the race. Eight of them come
            // to half the quiet period, so the arrival below is the debounce
            // firing and not the last cancellation coinciding with it.
            assert!(
                tokio::time::timeout(QUIET / 16, watcher.next())
                    .await
                    .is_err()
            );
        }
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(mount)),
        );
    }

    /// A source that dies still owes an answer about what it already saw, and
    /// then says there will be no more.
    #[tokio::test(start_paused = true)]
    async fn a_dead_source_decides_what_it_saw_and_then_ends() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("Kindle");
        std::fs::create_dir_all(&mount).unwrap();
        install(&mount);

        let (tx, mut watcher) = watcher();
        tx.send(MountStir(mount.clone())).await.unwrap();
        drop(tx);

        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            Some(MountEvent::Arrived(mount)),
        );
        assert_eq!(
            tokio::time::timeout(QUIET * 2, watcher.next())
                .await
                .unwrap(),
            None,
            "a closed source must end the stream rather than spin"
        );
    }

    /// A page turn rewrites a sidecar deep inside the volume. Reducing that to
    /// the volume is what stops a reader being rescanned on every page turn —
    /// and a path outside the roots is not ours at all.
    #[test]
    fn an_event_is_reduced_to_the_volume_it_happened_on() {
        let roots = vec![PathBuf::from("/Volumes"), PathBuf::from("/media/me")];
        assert_eq!(
            normalize(
                Path::new("/Volumes/KOBOeReader/books/x.sdr/metadata.epub.lua"),
                &roots
            ),
            Some(PathBuf::from("/Volumes/KOBOeReader")),
        );
        assert_eq!(
            normalize(Path::new("/media/me/Kindle"), &roots),
            Some(PathBuf::from("/media/me/Kindle")),
        );
        // The root itself: a stir that cannot name a volume, expanded later.
        assert_eq!(
            normalize(Path::new("/Volumes"), &roots),
            Some(PathBuf::from("/Volumes")),
        );
        assert_eq!(normalize(Path::new("/home/me/books"), &roots), None);
    }
}

#[cfg(test)]
mod vault_tests {
    use super::*;
    use crate::notes::{NewNoteInput, create_note};

    /// Real time rather than `start_paused`, deliberately. Every settle here
    /// awaits SQLite on another thread, and an idle runtime under a paused
    /// clock auto-advances — so the debounce being tested would be measured
    /// against a clock the test itself was moving. The windows below are wide
    /// enough that only a genuine regression fails them.
    const QUIET: Duration = Duration::from_millis(120);
    /// Long enough that a slow machine is not a failing machine.
    const EVENTUALLY: Duration = Duration::from_secs(5);

    struct Vault {
        _dir: tempfile::TempDir,
        root: PathBuf,
        storage: Storage,
    }

    impl Vault {
        async fn new() -> Vault {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("vault");
            std::fs::create_dir_all(&root).unwrap();
            Vault {
                _dir: dir,
                root,
                storage: Storage::connect("sqlite::memory:").await.unwrap(),
            }
        }

        /// A note written the way the app writes one: file in the vault, row
        /// and index in the database.
        async fn note(&self, body: &str) -> (i64, PathBuf) {
            self.titled(None, body).await
        }

        /// The same, with the title pinned.
        ///
        /// Worth having as its own helper because of what it isolates: a
        /// derived title is the note's first six words *and is indexed beside
        /// the body*, so a search for a word the edit removed still hits it
        /// through the title. That is not a staleness bug — the title is the
        /// `[[wikilink]]` target, and re-deriving it from an outside edit would
        /// silently repoint every backlink in the vault — but it does mean a
        /// test about bodies has to stop the title carrying the body's words.
        async fn titled(&self, title: Option<&str>, body: &str) -> (i64, PathBuf) {
            let created = create_note(
                &self.storage,
                &self.root,
                None,
                NewNoteInput {
                    title: title.map(str::to_string),
                    body: body.to_string(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            (created.id, created.file)
        }

        fn watcher(&self) -> (mpsc::Sender<VaultStir>, VaultWatcher) {
            let (tx, rx) = mpsc::channel(16);
            (
                tx,
                VaultWatcher::from_stirs(&self.root, self.storage.clone(), rx).quiet_for(QUIET),
            )
        }

        async fn finds(&self, query: &str) -> Vec<i64> {
            self.storage
                .search_notes(query, 10)
                .await
                .unwrap()
                .into_iter()
                .map(|h| h.note.id)
                .collect()
        }
    }

    /// **The bug, in one test.** Edit a note file behind the engine's back —
    /// which is what Obsidian is — and then search for what it now says.
    ///
    /// Before item 24 the second search returned nothing and the first still
    /// returned a hit, which is the worst shape this failure can take: the
    /// search box does not look broken, the note looks gone.
    #[tokio::test]
    async fn an_edit_made_behind_our_back_is_searchable_afterwards() {
        let v = Vault::new().await;
        // Titled, so the search below is about the body and nothing else —
        // see `Vault::titled`.
        let (id, path) = v
            .titled(
                Some("Pachinko, chapter 4"),
                "Sunja's dignity under pressure.",
            )
            .await;
        assert_eq!(v.finds("dignity").await, vec![id]);

        // Obsidian, vim, anything at all — the engine is not involved.
        let content = std::fs::read_to_string(&path).unwrap();
        let (header, _) = crate::notes::frontmatter_and_body(&content);
        std::fs::write(&path, format!("{header}Hansu's calculation instead.\n")).unwrap();

        let (tx, mut watcher) = v.watcher();
        tx.send(VaultStir(path)).await.unwrap();
        assert_eq!(
            tokio::time::timeout(EVENTUALLY, watcher.next())
                .await
                .unwrap(),
            Some(VaultEvent::Reindexed { note_id: id }),
        );

        assert_eq!(v.finds("calculation").await, vec![id], "the new text");
        assert!(v.finds("dignity").await.is_empty(), "the old text lingered");
    }

    /// Race one: we wrote it. The engine writes vault files itself, so the
    /// watcher sees its own writes — and must find nothing to do.
    ///
    /// The claim is stronger than "idempotent, so harmless": the refresh does
    /// not happen at all, which is why a save cannot amplify into a second
    /// write. It also pins `create_note` indexing the body it actually wrote
    /// rather than the one it was handed — the two differ by a trim and a
    /// newline, and while they differed every save looked like an edit.
    #[tokio::test]
    async fn our_own_write_is_not_re_indexed() {
        let v = Vault::new().await;
        let (_, path) = v.note("A thought that we saved ourselves.").await;

        let (tx, mut watcher) = v.watcher();
        // A save is several events on every platform.
        for _ in 0..6 {
            tx.send(VaultStir(path.clone())).await.unwrap();
        }
        assert!(
            tokio::time::timeout(QUIET * 8, watcher.next())
                .await
                .is_err(),
            "the watcher re-indexed a file the engine had just written"
        );
    }

    /// An editor that rewrites a file on focus loss without a character
    /// changing is the most common event a vault watcher ever sees. It is not
    /// an edit.
    #[tokio::test]
    async fn a_no_op_save_is_not_an_edit() {
        let v = Vault::new().await;
        let (_, path) = v.note("Unchanged prose.").await;
        let content = std::fs::read_to_string(&path).unwrap();
        // Byte-identical rewrite, new mtime.
        std::fs::write(&path, &content).unwrap();

        let (tx, mut watcher) = v.watcher();
        tx.send(VaultStir(path)).await.unwrap();
        assert!(
            tokio::time::timeout(QUIET * 8, watcher.next())
                .await
                .is_err(),
        );
    }

    /// And a trailing newline is not one either — an editor that adds one on
    /// save would otherwise re-index the whole vault the first time it ran.
    #[tokio::test]
    async fn a_trailing_newline_is_not_an_edit() {
        let v = Vault::new().await;
        let (_, path) = v.note("Prose with an opinionated editor.").await;
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{content}\n\n")).unwrap();

        let (tx, mut watcher) = v.watcher();
        tx.send(VaultStir(path)).await.unwrap();
        assert!(
            tokio::time::timeout(QUIET * 8, watcher.next())
                .await
                .is_err(),
        );
    }

    /// The debounce, in the shape the mount watcher's has: a burst is one
    /// re-index, and not before the burst stops.
    #[tokio::test]
    async fn a_burst_of_saves_becomes_one_reindex_and_not_before_it_stops() {
        let v = Vault::new().await;
        let (id, path) = v.note("first draft").await;

        let (tx, mut watcher) = v.watcher();
        let content = std::fs::read_to_string(&path).unwrap();
        let (header, _) = crate::notes::frontmatter_and_body(&content);
        std::fs::write(&path, format!("{header}second draft\n")).unwrap();
        for _ in 0..12 {
            tx.send(VaultStir(path.clone())).await.unwrap();
        }

        assert!(
            tokio::time::timeout(QUIET / 3, watcher.next())
                .await
                .is_err(),
            "indexed before the writer had finished"
        );
        assert_eq!(
            tokio::time::timeout(EVENTUALLY, watcher.next())
                .await
                .unwrap(),
            Some(VaultEvent::Reindexed { note_id: id }),
        );
        assert!(
            tokio::time::timeout(QUIET * 8, watcher.next())
                .await
                .is_err(),
            "one save, one re-index"
        );
    }

    /// Race three, first half: **a file that vanishes is not a note that was
    /// deleted.** Nothing is written — the row, the index and the graph all
    /// survive, and the note is still searchable.
    #[tokio::test]
    async fn a_vanished_file_does_not_delete_the_note() {
        let v = Vault::new().await;
        let (id, path) = v.note("A note somebody moved out of the way.").await;
        std::fs::remove_file(&path).unwrap();

        let (tx, mut watcher) = v.watcher();
        tx.send(VaultStir(path)).await.unwrap();
        assert_eq!(
            tokio::time::timeout(EVENTUALLY, watcher.next())
                .await
                .unwrap(),
            Some(VaultEvent::Vanished { note_id: id }),
        );

        assert_eq!(v.finds("somebody").await, vec![id], "an absence deleted it");
        assert!(v.storage.get_note(id).await.unwrap().is_some());
    }

    /// Race three, second half: moved out and back. Because absence wrote
    /// nothing, recreation needs no case of its own — the row is still there,
    /// so a returning file is only a file that changed.
    #[tokio::test]
    async fn a_file_moved_out_and_back_finds_its_note_again() {
        let v = Vault::new().await;
        let (id, path) = v.note("Original wording.").await;
        let content = std::fs::read_to_string(&path).unwrap();
        let (header, _) = crate::notes::frontmatter_and_body(&content);

        let (tx, mut watcher) = v.watcher();
        std::fs::remove_file(&path).unwrap();
        tx.send(VaultStir(path.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(EVENTUALLY, watcher.next())
                .await
                .unwrap(),
            Some(VaultEvent::Vanished { note_id: id }),
        );

        // Back it comes, edited while it was away — a sync client resolving a
        // conflict, or a `git checkout`.
        std::fs::write(&path, format!("{header}Rewritten while it was away.\n")).unwrap();
        tx.send(VaultStir(path)).await.unwrap();
        assert_eq!(
            tokio::time::timeout(EVENTUALLY, watcher.next())
                .await
                .unwrap(),
            Some(VaultEvent::Reindexed { note_id: id }),
        );
        assert_eq!(v.finds("Rewritten").await, vec![id]);
    }

    /// The invariant that makes the departure from "may scan, may not sync"
    /// safe to have made: this watcher writes to the database and **never to
    /// the vault**. Everything it did is recomputable from the files it left
    /// alone.
    #[tokio::test]
    async fn never_writes_to_the_vault() {
        let v = Vault::new().await;
        let mut paths = Vec::new();
        for i in 0..4 {
            let (_, path) = v.note(&format!("Note number {i} about something.")).await;
            paths.push(path);
        }
        // Something that is not a note, in the directory too.
        let stray = v.root.join("unsorted/stray.md");
        std::fs::write(&stray, "not ours\n").unwrap();
        paths.push(stray);
        // And an edit, so there is real work to do.
        let content = std::fs::read_to_string(&paths[0]).unwrap();
        let (header, _) = crate::notes::frontmatter_and_body(&content);
        std::fs::write(&paths[0], format!("{header}Edited elsewhere.\n")).unwrap();

        let before = snapshot(&v.root);
        let (tx, mut watcher) = v.watcher();
        for path in &paths {
            tx.send(VaultStir(path.clone())).await.unwrap();
        }
        // Drain everything it has to say.
        while tokio::time::timeout(QUIET * 8, watcher.next())
            .await
            .is_ok_and(|e| e.is_some())
        {}

        assert_eq!(before, snapshot(&v.root), "the watcher wrote to the vault");
    }

    /// A markdown file readingbuddy did not write is not a note it owns.
    /// Adopting one would invent a note out of whatever an editor left lying
    /// in the directory — and would have to guess its book, its kind and its
    /// anchor. Reported as a gap, not built.
    #[tokio::test]
    async fn an_unclaimed_markdown_file_is_not_adopted() {
        let v = Vault::new().await;
        let stray = v.root.join("someone-elses.md");
        std::fs::write(&stray, "# Written in Obsidian, never here\n").unwrap();

        let (tx, mut watcher) = v.watcher();
        tx.send(VaultStir(stray)).await.unwrap();
        assert!(
            tokio::time::timeout(QUIET * 8, watcher.next())
                .await
                .is_err(),
        );
        assert!(v.storage.list_notes(None).await.unwrap().is_empty());
    }

    /// An outside edit is exactly where a new `[[wikilink]]` appears, so the
    /// graph has to follow the file and not only the FTS body.
    #[tokio::test]
    async fn a_wikilink_added_from_outside_lands_in_the_graph() {
        let v = Vault::new().await;
        let (target, _) = v.note("Han").await;
        let (id, path) = v.note("A thought with no links yet.").await;
        assert!(v.storage.outgoing_links(id).await.unwrap().is_empty());

        let content = std::fs::read_to_string(&path).unwrap();
        let (header, _) = crate::notes::frontmatter_and_body(&content);
        std::fs::write(&path, format!("{header}Now it points at [[Han]].\n")).unwrap();

        let (tx, mut watcher) = v.watcher();
        tx.send(VaultStir(path)).await.unwrap();
        tokio::time::timeout(EVENTUALLY, watcher.next())
            .await
            .unwrap()
            .unwrap();

        let out = v.storage.outgoing_links(id).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to.as_ref().map(|n| n.id), Some(target));
    }

    /// The TUI drops this future on every keypress, and here a drop can land
    /// mid-write rather than merely mid-wait. Resuming must not lose the edit.
    #[tokio::test]
    async fn cancelling_the_wait_does_not_lose_the_edit() {
        let v = Vault::new().await;
        let (id, path) = v.note("before").await;
        let content = std::fs::read_to_string(&path).unwrap();
        let (header, _) = crate::notes::frontmatter_and_body(&content);
        std::fs::write(&path, format!("{header}afterwards\n")).unwrap();

        let (tx, mut watcher) = v.watcher();
        tx.send(VaultStir(path)).await.unwrap();
        // Poll and drop, repeatedly, straddling the deadline — so at least one
        // drop lands after the debounce fires and inside the refresh.
        for _ in 0..40 {
            if tokio::time::timeout(QUIET / 10, watcher.next())
                .await
                .is_ok()
            {
                break;
            }
        }
        assert_eq!(v.finds("afterwards").await, vec![id]);
    }

    /// A source that dies still owes an answer about the save it already saw.
    #[tokio::test]
    async fn a_dead_source_decides_what_it_saw_and_then_ends() {
        let v = Vault::new().await;
        let (id, path) = v.note("draft").await;
        let content = std::fs::read_to_string(&path).unwrap();
        let (header, _) = crate::notes::frontmatter_and_body(&content);
        std::fs::write(&path, format!("{header}final\n")).unwrap();

        let (tx, mut watcher) = v.watcher();
        tx.send(VaultStir(path)).await.unwrap();
        drop(tx);

        assert_eq!(
            tokio::time::timeout(EVENTUALLY, watcher.next())
                .await
                .unwrap(),
            Some(VaultEvent::Reindexed { note_id: id }),
        );
        assert_eq!(
            tokio::time::timeout(EVENTUALLY, watcher.next())
                .await
                .unwrap(),
            None,
            "a closed source must end the stream rather than spin"
        );
    }

    /// Race two: a partial write. The debounce is the first answer and not a
    /// sufficient one, so the read is bracketed by two stats — and a file
    /// whose length or mtime moved across the read is not indexed.
    ///
    /// Injected rather than raced, for the module's own reason: a guard that
    /// can only be triggered by out-running a real text editor is a guard with
    /// no test.
    #[test]
    fn a_file_that_moves_while_we_read_it_is_not_believed() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        std::fs::write(&path, "half a sen").unwrap();

        // A truncate-in-place editor, caught in the act.
        let mid_write = |p: &Path| {
            let got = std::fs::read_to_string(p)?;
            std::fs::write(p, "half a sentence, then the rest of it")?;
            Ok(got)
        };
        assert!(matches!(
            settled_read_with(&path, mid_write).unwrap(),
            Settled::Moving
        ));

        // And a file at rest is believed.
        assert!(matches!(
            settled_read(&path).unwrap(),
            Settled::Content(c) if c == "half a sentence, then the rest of it"
        ));

        // A file that is not there is `Gone`, never an error — absence is an
        // ordinary state of a vault other tools can touch.
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(settled_read(&path).unwrap(), Settled::Gone));
    }

    /// What a lookup has to spell to find a note: `create_note` writes
    /// `notes.file_path` with `/` separators, so a reduce that used the
    /// platform's would find nothing on Windows and never say why.
    #[test]
    fn a_path_is_reduced_to_what_the_row_says() {
        let vault = Path::new("/data/vault");
        assert_eq!(
            crate::notes::vault_relative(vault, Path::new("/data/vault/pachinko/2026-a.md")),
            Some("pachinko/2026-a.md".to_string()),
        );
        // Not markdown, not under the vault, and the vault itself.
        assert_eq!(
            crate::notes::vault_relative(vault, Path::new("/data/vault/.obsidian/workspace.json")),
            None,
        );
        assert_eq!(
            crate::notes::vault_relative(vault, Path::new("/elsewhere/note.md")),
            None,
        );
        assert_eq!(crate::notes::vault_relative(vault, vault), None);
    }

    /// Every file under a root, with its bytes — enough to prove nothing here
    /// touched one.
    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push((path.clone(), std::fs::read(&path).unwrap()));
                }
            }
        }
        out.sort();
        out
    }
}
