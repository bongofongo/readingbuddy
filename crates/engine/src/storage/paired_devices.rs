//! The readers we have installed the plugin onto (`paired_devices`, migration
//! `0019`).
//!
//! Our half of a pairing; the device's half is `pairing.lua` inside
//! `readingbuddy.koplugin/`. Identity is `device_id` — see the migration for
//! why `last_mount_path` is a breadcrumb and never a key.

use super::{Storage, now_unix};
use crate::error::Result;
use sqlx::Row;

/// One paired reader.
///
/// `Debug` is hand-written for one reason: `token` must never reach a log, an
/// error message or a `Diagnostic`, and a derived `Debug` is how it would —
/// `tracing::debug!(?device)` is one keystroke and there would be nothing to
/// see. The rest of the struct is worth printing, so redacting the field beats
/// suppressing the impl.
#[derive(Clone, PartialEq, Eq)]
pub struct PairedDevice {
    pub device_id: String,
    pub label: Option<String>,
    pub token: String,
    pub plugin_version: i64,
    pub installed_at: i64,
    pub last_mount_path: Option<String>,
    pub last_seen_at: Option<i64>,
}

impl std::fmt::Debug for PairedDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairedDevice")
            .field("device_id", &self.device_id)
            .field("label", &self.label)
            .field("token", &"<redacted>")
            .field("plugin_version", &self.plugin_version)
            .field("installed_at", &self.installed_at)
            .field("last_mount_path", &self.last_mount_path)
            .field("last_seen_at", &self.last_seen_at)
            .finish()
    }
}

const COLUMNS: &str = "device_id, label, token, plugin_version, installed_at, \
                       last_mount_path, last_seen_at";

fn row_to_device(row: &sqlx::sqlite::SqliteRow) -> Result<PairedDevice> {
    Ok(PairedDevice {
        device_id: row.try_get("device_id")?,
        label: row.try_get("label")?,
        token: row.try_get("token")?,
        plugin_version: row.try_get("plugin_version")?,
        installed_at: row.try_get("installed_at")?,
        last_mount_path: row.try_get("last_mount_path")?,
        last_seen_at: row.try_get("last_seen_at")?,
    })
}

impl Storage {
    /// Record a pairing, or refresh one that already exists.
    ///
    /// **An upgrade does not rotate the token and does not move
    /// `installed_at`.** Reinstalling a newer plugin onto a reader that is
    /// already paired is an upgrade, not a re-pairing: the relationship is the
    /// same one, and rotating the secret would mean every plugin upgrade
    /// silently invalidated whatever item 15b builds on top of it. What does
    /// refresh is the version we put there and where we last saw it.
    ///
    /// **`paired_at` is the caller's, and deliberately not read from the clock
    /// here.** The installer writes the same instant into the device's own
    /// `pairing.lua`, and a second `now_unix()` in this function made one event
    /// carry two timestamps — a real Kindle came back holding
    /// `paired_at = …446` against an `installed_at` of `…447`. It reaches only
    /// a *new* row; the conflict arm leaves `installed_at` where it was.
    pub async fn record_pairing(
        &self,
        device_id: &str,
        label: Option<&str>,
        token: &str,
        plugin_version: i64,
        mount_path: Option<&str>,
        paired_at: i64,
    ) -> Result<()> {
        let now = now_unix();
        sqlx::query(
            "INSERT INTO paired_devices
                 (device_id, label, token, plugin_version, installed_at,
                  last_mount_path, last_seen_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(device_id) DO UPDATE SET
                 label           = COALESCE(excluded.label, paired_devices.label),
                 plugin_version  = excluded.plugin_version,
                 last_mount_path = excluded.last_mount_path,
                 last_seen_at    = excluded.last_seen_at",
        )
        .bind(device_id)
        .bind(label)
        .bind(token)
        .bind(plugin_version)
        .bind(paired_at)
        .bind(mount_path)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn paired_device(&self, device_id: &str) -> Result<Option<PairedDevice>> {
        let sql = format!("SELECT {COLUMNS} FROM paired_devices WHERE device_id = ?");
        let row = sqlx::query(&sql)
            .bind(device_id)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_device).transpose()
    }

    /// Every paired reader, most recently seen first.
    ///
    /// This is what the table is *for*: a reader in a bag is still paired, and
    /// a frontend that could only answer by walking a mount would have no way
    /// to say so.
    pub async fn list_paired_devices(&self) -> Result<Vec<PairedDevice>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM paired_devices
             ORDER BY COALESCE(last_seen_at, installed_at) DESC, device_id"
        );
        let rows = sqlx::query(&sql).fetch_all(self.pool()).await?;
        rows.iter().map(row_to_device).collect()
    }

    /// Forget a pairing. Returns whether there was one.
    pub async fn forget_pairing(&self, device_id: &str) -> Result<bool> {
        let done = sqlx::query("DELETE FROM paired_devices WHERE device_id = ?")
            .bind(device_id)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::Storage;

    async fn store() -> Storage {
        Storage::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn a_pairing_round_trips() {
        let s = store().await;
        s.record_pairing(
            "dev-1",
            Some("KOBOeReader"),
            "tok",
            1,
            Some("/mnt/k"),
            1_700_000_000,
        )
        .await
        .unwrap();
        let d = s.paired_device("dev-1").await.unwrap().unwrap();
        assert_eq!(d.label.as_deref(), Some("KOBOeReader"));
        assert_eq!(d.token, "tok");
        assert_eq!(d.plugin_version, 1);
        assert_eq!(d.last_mount_path.as_deref(), Some("/mnt/k"));
    }

    #[tokio::test]
    async fn an_upgrade_keeps_the_token_and_the_install_date() {
        let s = store().await;
        s.record_pairing(
            "dev-1",
            Some("KOBOeReader"),
            "tok",
            1,
            Some("/mnt/k"),
            1_700_000_000,
        )
        .await
        .unwrap();
        let first = s.paired_device("dev-1").await.unwrap().unwrap();

        // A second install, a version later, from a different mount point.
        s.record_pairing(
            "dev-1",
            None,
            "a-different-token",
            2,
            Some("/mnt/other"),
            1_700_086_400,
        )
        .await
        .unwrap();
        let second = s.paired_device("dev-1").await.unwrap().unwrap();

        assert_eq!(second.token, first.token, "an upgrade is not a re-pairing");
        assert_eq!(second.installed_at, first.installed_at);
        assert_eq!(second.plugin_version, 2);
        assert_eq!(second.last_mount_path.as_deref(), Some("/mnt/other"));
        // A label is not un-set by an install that could not read one.
        assert_eq!(second.label.as_deref(), Some("KOBOeReader"));
    }

    #[tokio::test]
    async fn forgetting_says_whether_there_was_anything_to_forget() {
        let s = store().await;
        s.record_pairing("dev-1", None, "tok", 1, None, 1_700_000_000)
            .await
            .unwrap();
        assert!(s.forget_pairing("dev-1").await.unwrap());
        assert!(!s.forget_pairing("dev-1").await.unwrap());
        assert!(s.paired_device("dev-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn the_token_is_not_in_the_debug_output() {
        let s = store().await;
        s.record_pairing("dev-1", None, "super-secret", 1, None, 1_700_000_000)
            .await
            .unwrap();
        let d = s.paired_device("dev-1").await.unwrap().unwrap();
        let printed = format!("{d:?}");
        assert!(
            !printed.contains("super-secret"),
            "a derived Debug is how the token reaches a log: {printed}"
        );
    }
}
