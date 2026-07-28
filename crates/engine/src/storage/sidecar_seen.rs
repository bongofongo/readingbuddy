//! The device-scan pre-filter cache (`sidecar_seen`, migration `0006`).
//!
//! Every column is something the *file* said. Nothing here is a verdict about
//! the library — see the migration's header for why that line matters.

use sqlx::Row;

use super::Storage;
use crate::error::Result;

/// What one sidecar file said, last time it was read.
///
/// `size` and `mtime` are the freshness key; the rest is the part of a parse a
/// scan actually needs. Deliberately not a `KoSidecar`: the highlights are the
/// expensive half and are never stored, because storing them would make this a
/// second copy of `highlights` that nothing keeps in step.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarFacts {
    pub size: i64,
    /// Nanoseconds since the epoch. See the migration.
    pub mtime: i64,
    pub partial_md5: Option<String>,
    /// How many highlights the file yielded.
    pub entry_count: i64,
    /// The file's annotations through `DeviceDigest`. See the migration for why
    /// a count alone was not enough.
    pub entry_digest: String,
    /// `doc_props.title` — the value the matcher sees, not a prettier one. A
    /// cache hit has to reproduce the same match a fresh parse would.
    pub title: Option<String>,
    pub authors: Option<String>,
    pub ko_percent: Option<f64>,
    /// [`crate::koreader::KoStatus`] as written by the device. Kept as text so
    /// a status we do not model round-trips unchanged.
    pub ko_status: Option<String>,
}

impl Storage {
    /// What this path said last time, if we have read it before.
    ///
    /// Returns the row whatever its `size`/`mtime` — deciding whether it is
    /// still fresh is the caller's, because the caller is the one holding the
    /// `stat`.
    pub async fn sidecar_facts(&self, path: &str) -> Result<Option<SidecarFacts>> {
        let row = sqlx::query(
            "SELECT size, mtime, partial_md5, entry_count, entry_digest, title, authors,
                    ko_percent, ko_status
             FROM sidecar_seen WHERE path = ?",
        )
        .bind(path)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| SidecarFacts {
            size: r.get("size"),
            mtime: r.get("mtime"),
            partial_md5: r.get("partial_md5"),
            entry_count: r.get("entry_count"),
            entry_digest: r.get("entry_digest"),
            title: r.get("title"),
            authors: r.get("authors"),
            ko_percent: r.get("ko_percent"),
            ko_status: r.get("ko_status"),
        }))
    }

    /// Record what a sidecar said. Overwrites in full rather than merging: a
    /// sidecar is the complete state of its book, so a field that has gone away
    /// has gone away — the same rule `refresh_device_fields` follows, and for
    /// the same reason.
    ///
    /// `seen_at` is the scan's own token, not the clock. See
    /// [`Storage::forget_sidecars_under`].
    pub async fn record_sidecar_facts(
        &self,
        path: &str,
        f: &SidecarFacts,
        seen_at: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO sidecar_seen
                 (path, size, mtime, partial_md5, entry_count, entry_digest, title, authors,
                  ko_percent, ko_status, last_seen)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(path) DO UPDATE SET
                   size         = excluded.size,
                   mtime        = excluded.mtime,
                   partial_md5  = excluded.partial_md5,
                   entry_count  = excluded.entry_count,
                   entry_digest = excluded.entry_digest,
                   title       = excluded.title,
                   authors     = excluded.authors,
                   ko_percent  = excluded.ko_percent,
                   ko_status   = excluded.ko_status,
                   last_seen   = excluded.last_seen"#,
        )
        .bind(path)
        .bind(f.size)
        .bind(f.mtime)
        .bind(f.partial_md5.as_ref())
        .bind(f.entry_count)
        .bind(&f.entry_digest)
        .bind(f.title.as_ref())
        .bind(f.authors.as_ref())
        .bind(f.ko_percent)
        .bind(f.ko_status.as_ref())
        .bind(seen_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Stamp a row as seen without touching what the file said.
    ///
    /// A cache *hit* still has to say it was seen, or the prune below would
    /// delete exactly the rows that are doing their job.
    pub async fn touch_sidecar_seen(&self, path: &str, seen_at: i64) -> Result<()> {
        sqlx::query("UPDATE sidecar_seen SET last_seen = ? WHERE path = ?")
            .bind(seen_at)
            .bind(path)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Drop cache rows under `root` that this scan did not see — the sidecars
    /// that have been deleted from the device since.
    ///
    /// Scoped to `root` on purpose: a scan of one mount must not forget another
    /// device that simply is not plugged in.
    ///
    /// **`seen_at` is a token, and the comparison is `<>`, not `<`.** The
    /// obvious "delete anything older than this scan started" is wrong at
    /// second resolution: a scan that finishes inside the same second as the
    /// one before it stamps rows with a value that is not *less than* its own
    /// start, so nothing is ever pruned — and the whole prune quietly does
    /// nothing without ever failing. The scan passes one nanosecond-resolution
    /// token to every write it makes and deletes whatever does not carry it.
    ///
    /// `substr(path, 1, n) = root` rather than `LIKE root || '%'`, because a
    /// mount name is user-supplied text and `%`/`_` in it would silently widen
    /// a `LIKE` into deleting rows it was never meant to. The separator is
    /// appended here for the neighbouring reason: a bare `/mnt/a` prefix also
    /// matches `/mnt/abc`.
    pub async fn forget_sidecars_under(&self, root: &str, seen_at: i64) -> Result<u64> {
        let prefix = format!("{}/", root.trim_end_matches('/'));
        let done =
            sqlx::query("DELETE FROM sidecar_seen WHERE substr(path, 1, ?) = ? AND last_seen <> ?")
                .bind(prefix.chars().count() as i64)
                .bind(&prefix)
                .bind(seen_at)
                .execute(self.pool())
                .await?;
        Ok(done.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::now_unix;

    fn facts(entry_count: i64) -> SidecarFacts {
        SidecarFacts {
            size: 100,
            mtime: 1_700_000_000_000_000_000,
            partial_md5: Some("abc".into()),
            entry_count,
            entry_digest: format!("digest-of-{entry_count}"),
            title: Some("Pachinko".into()),
            authors: Some("Min Jin Lee".into()),
            ko_percent: Some(0.5),
            ko_status: Some("reading".into()),
        }
    }

    const FIRST: i64 = 1_000;
    const SECOND: i64 = 2_000;

    #[tokio::test]
    async fn facts_round_trip_and_a_rewrite_replaces_them() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        assert!(s.sidecar_facts("/x").await.unwrap().is_none());

        s.record_sidecar_facts("/x", &facts(3), FIRST)
            .await
            .unwrap();
        assert_eq!(s.sidecar_facts("/x").await.unwrap(), Some(facts(3)));

        // Not a merge: the whole row is what the file now says.
        let mut second = facts(4);
        second.ko_status = None;
        second.title = None;
        s.record_sidecar_facts("/x", &second, SECOND).await.unwrap();
        assert_eq!(s.sidecar_facts("/x").await.unwrap(), Some(second));

        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM sidecar_seen")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn a_prune_forgets_only_unseen_rows_under_its_own_root() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        for p in ["/mnt/a/one.lua", "/mnt/a/two.lua", "/mnt/b/three.lua"] {
            s.record_sidecar_facts(p, &facts(1), FIRST).await.unwrap();
        }
        // One of them is seen again by "this scan".
        s.touch_sidecar_seen("/mnt/a/one.lua", SECOND)
            .await
            .unwrap();

        assert_eq!(s.forget_sidecars_under("/mnt/a", SECOND).await.unwrap(), 1);
        assert!(s.sidecar_facts("/mnt/a/one.lua").await.unwrap().is_some());
        assert!(s.sidecar_facts("/mnt/a/two.lua").await.unwrap().is_none());
        assert!(
            s.sidecar_facts("/mnt/b/three.lua").await.unwrap().is_some(),
            "a scan of one mount must not forget another device"
        );
    }

    /// The reason the prune compares tokens rather than times: two scans inside
    /// one clock second must still be able to tell each other apart, and at
    /// second resolution "older than this scan started" never fires.
    #[tokio::test]
    async fn a_prune_is_not_defeated_by_two_scans_in_the_same_second() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let one_second = now_unix();
        s.record_sidecar_facts("/mnt/a/gone.lua", &facts(1), one_second)
            .await
            .unwrap();
        // The next scan starts inside that same second and sees nothing.
        assert_eq!(
            s.forget_sidecars_under("/mnt/a", one_second).await.unwrap(),
            0,
            "same token: this scan did see it"
        );
        assert_eq!(
            s.forget_sidecars_under("/mnt/a", one_second + 1)
                .await
                .unwrap(),
            1
        );
    }

    /// `/mnt/a` is not a prefix of `/mnt/abc` in any sense that matters here,
    /// and treating it as one would delete a neighbouring mount's cache.
    #[tokio::test]
    async fn a_sibling_mount_with_a_longer_name_is_not_under_this_root() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        s.record_sidecar_facts("/mnt/abc/one.lua", &facts(1), FIRST)
            .await
            .unwrap();
        assert_eq!(s.forget_sidecars_under("/mnt/a", SECOND).await.unwrap(), 0);
        assert!(s.sidecar_facts("/mnt/abc/one.lua").await.unwrap().is_some());
    }

    /// A mount is named by the user, and `%` is a legal character in a volume
    /// name. Under a `LIKE` prefix that is a wildcard, and pruning `/mnt/%`
    /// would take `/mnt/b` with it.
    #[tokio::test]
    async fn a_wildcard_in_a_mount_name_does_not_widen_the_prune() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        s.record_sidecar_facts("/mnt/b/three.lua", &facts(1), FIRST)
            .await
            .unwrap();
        assert_eq!(s.forget_sidecars_under("/mnt/%", SECOND).await.unwrap(), 0);
        assert!(s.sidecar_facts("/mnt/b/three.lua").await.unwrap().is_some());
    }
}
