//! Noticing a reader without being asked.
//!
//! Everything a mounted device needs is already here — [`crate::device`] has
//! `scan_device`, `sync_device`, `candidate_mounts`, `is_koreader_mount` and the
//! `sidecar_seen` cache that makes a rescan cost nothing. What was missing is
//! *noticing*: today a mount is only found because the user opened the device
//! screen and asked.
//!
//! Two things shape this module, and both are constraints rather than choices.
//!
//! **The event source is injected, never hard-wired.** A [`MountWatcher`] is
//! driven by a channel of [`MountStir`]s; [`watch_mounts`] is the adapter that
//! fills that channel from `notify`, and is the only part of the file that
//! cannot run in CI. A watcher that can only be driven by plugging in real
//! hardware is a watcher with no tests, and the debounce below is exactly the
//! part that has to be tested.
//!
//! **It may scan; it may not sync.** `docs/decisions.md` is explicit that
//! mount → import is automatic and read-only while anything that *writes* to the
//! device is explicit and shows the path. This module therefore holds no
//! [`crate::storage::Storage`] at all: it announces arrivals and departures, and
//! what the frontend does about one is the frontend's decision. Nothing here can
//! write to a device or to the library, by construction rather than by rule.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::device::{mount_roots, offers_reader};
use crate::error::{EngineError, Result};

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
