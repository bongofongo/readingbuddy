//! The readers we have installed the plugin onto (`paired_devices`, migration
//! `0019`, `0020`).
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
    /// When *everything* this reader had was last brought across (migration
    /// `0020`).
    ///
    /// `None` is ordinary and is **not** *never synced*: the column arrived
    /// with no back-fill because nothing recorded which device a past
    /// `sync_device` read from. Phrase it as *not since we started recording*.
    pub last_synced_at: Option<i64>,
    /// When this reader last reached us **over the network** (migration
    /// `0021`).
    ///
    /// The fourth timestamp here and a next-door question to the other three:
    /// `last_seen_at` is a cable, this is a LAN. Folding them would tell
    /// somebody whose reader pushed from the next room that it had been plugged
    /// in. `None` carries `0020`'s phrasing — *not since we started recording*.
    pub last_wireless_at: Option<i64>,
    /// Where it reached us from. A breadcrumb, exactly like `last_mount_path`,
    /// and **nothing may join on it**: a lease moves and the reader is the
    /// same reader at a different number.
    pub last_lan_addr: Option<String>,
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
            .field("last_synced_at", &self.last_synced_at)
            .field("last_wireless_at", &self.last_wireless_at)
            .field("last_lan_addr", &self.last_lan_addr)
            .finish()
    }
}

const COLUMNS: &str = "device_id, label, token, plugin_version, installed_at, \
                       last_mount_path, last_seen_at, last_synced_at, \
                       last_wireless_at, last_lan_addr";

fn row_to_device(row: &sqlx::sqlite::SqliteRow) -> Result<PairedDevice> {
    Ok(PairedDevice {
        device_id: row.try_get("device_id")?,
        label: row.try_get("label")?,
        token: row.try_get("token")?,
        plugin_version: row.try_get("plugin_version")?,
        installed_at: row.try_get("installed_at")?,
        last_mount_path: row.try_get("last_mount_path")?,
        last_seen_at: row.try_get("last_seen_at")?,
        last_synced_at: row.try_get("last_synced_at")?,
        last_wireless_at: row.try_get("last_wireless_at")?,
        last_lan_addr: row.try_get("last_lan_addr")?,
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
    /// **An existing label wins over the mount's name.** This was the other way
    /// round — `COALESCE(excluded.label, paired_devices.label)` — which was
    /// harmless while the only writer of a label was this function, and became a
    /// bug the moment item 55 let a reader be renamed: every plugin upgrade
    /// would have quietly restored `KOBOeReader` over whatever the user called
    /// it. The mount's directory name is a *default*, so it fills an empty
    /// label and never replaces one.
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
                 label           = COALESCE(paired_devices.label, excluded.label),
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

    /// Record that this reader was in our hands just now.
    ///
    /// **The write behind `Engine::plugin_status`**, and the reason it is a
    /// method rather than a line in one caller: *when did I last have this
    /// device* is asked by three frontends, and a stamp each of them had to
    /// remember to make would be a column that meant something different
    /// depending on which one you had been using.
    ///
    /// `mount_path` is refreshed at the same time because the two facts are one
    /// observation — *seen, there*. It stays advisory (see the migration): a
    /// reader that has moved ports is the same reader.
    ///
    /// Returns whether there was a row. `false` is ordinary: a `pairing.lua`
    /// naming a device we have never heard of is a reader paired with some
    /// other copy of readingbuddy, and stamping nothing is the correct answer.
    pub async fn touch_device_seen(
        &self,
        device_id: &str,
        mount_path: Option<&str>,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE paired_devices
                SET last_seen_at    = ?,
                    last_mount_path = COALESCE(?, last_mount_path)
              WHERE device_id = ?",
        )
        .bind(now_unix())
        .bind(mount_path)
        .bind(device_id)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Record that *everything* this reader had was brought across.
    ///
    /// Separate from [`Storage::touch_device_seen`] on purpose — see migration
    /// `0020`. Plugging a reader in to charge it is not a sync, and one column
    /// doing both jobs would tell that user their highlights had come across.
    ///
    /// Only `Engine::sync_mount` calls it. A one-book pull leaves the question
    /// this column answers unchanged, and `sync_device` cannot name a device
    /// anyway.
    pub async fn stamp_device_sync(&self, device_id: &str) -> Result<bool> {
        let done = sqlx::query("UPDATE paired_devices SET last_synced_at = ? WHERE device_id = ?")
            .bind(now_unix())
            .bind(device_id)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Record that this reader reached us **over the network** (migration
    /// `0021`).
    ///
    /// [`Storage::touch_device_seen`]'s sibling, and separate from it for
    /// exactly the reason that one is separate from
    /// [`Storage::stamp_device_sync`]: until item 15b, *we saw this reader* and
    /// *this reader was plugged in* were one event, and they are now two. A
    /// reader that pushed from the next room was never in anybody's hand, so
    /// stamping `last_seen_at` here would report a cable that did not happen.
    ///
    /// It stamps **no sync**. What arrived over the wire is whatever the reader
    /// chose to send, and `0020`'s argument is unchanged one transport over: a
    /// push of one book leaves *is this reader's reading here* exactly where it
    /// was. Whoever brings *everything* across calls `stamp_device_sync` beside
    /// this, the way `sync_mount` already does over a cable.
    ///
    /// `addr` refreshes with the time because the two are one observation —
    /// *reached, from there* — and it stays advisory: see the migration, and
    /// `last_mount_path` before it. `None` leaves the previous hint alone, so a
    /// contact whose source address we could not name does not erase the one
    /// that worked.
    ///
    /// Returns whether there was a row. `false` is ordinary and is the answer
    /// for a reader paired with some other copy of readingbuddy.
    pub async fn stamp_wireless_contact(
        &self,
        device_id: &str,
        addr: Option<&str>,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE paired_devices
                SET last_wireless_at = ?,
                    last_lan_addr    = COALESCE(?, last_lan_addr)
              WHERE device_id = ?",
        )
        .bind(now_unix())
        .bind(addr)
        .bind(device_id)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Give a reader a name.
    ///
    /// The label is the only field here the user owns, and it exists because
    /// the default is the mount's directory name — `Kindle`, `KOBOeReader` —
    /// which is a fact about a filesystem and not about a thing you carry. An
    /// empty or whitespace-only name **clears** it rather than storing a blank:
    /// a device with no name falls back to something a frontend can draw, and a
    /// row holding `"   "` would not.
    ///
    /// Returns whether there was a row to rename.
    pub async fn set_device_label(&self, device_id: &str, label: &str) -> Result<bool> {
        let trimmed = label.trim();
        let value = (!trimmed.is_empty()).then_some(trimmed);
        let done = sqlx::query("UPDATE paired_devices SET label = ? WHERE device_id = ?")
            .bind(value)
            .bind(device_id)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
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
    async fn a_rename_survives_a_plugin_upgrade() {
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
        assert!(
            s.set_device_label("dev-1", "the bedside Kobo")
                .await
                .unwrap()
        );

        // The upgrade offers the mount's directory name, exactly as
        // `install_plugin_at` does. It must lose.
        s.record_pairing(
            "dev-1",
            Some("KOBOeReader"),
            "tok",
            2,
            Some("/mnt/k"),
            1_700_000_000,
        )
        .await
        .unwrap();

        let d = s.paired_device("dev-1").await.unwrap().unwrap();
        assert_eq!(
            d.label.as_deref(),
            Some("the bedside Kobo"),
            "an upgrade must not restore the mount's directory name over a rename"
        );
        assert_eq!(d.plugin_version, 2, "and the upgrade still happened");
    }

    #[tokio::test]
    async fn a_blank_name_clears_rather_than_stores() {
        let s = store().await;
        s.record_pairing("dev-1", Some("Kindle"), "tok", 1, None, 1_700_000_000)
            .await
            .unwrap();
        assert!(s.set_device_label("dev-1", "   ").await.unwrap());
        assert_eq!(s.paired_device("dev-1").await.unwrap().unwrap().label, None);

        // And a cleared label is an *empty* one, so the next install fills it.
        s.record_pairing("dev-1", Some("Kindle"), "tok", 1, None, 1_700_000_000)
            .await
            .unwrap();
        assert_eq!(
            s.paired_device("dev-1")
                .await
                .unwrap()
                .unwrap()
                .label
                .as_deref(),
            Some("Kindle")
        );
    }

    #[tokio::test]
    async fn seeing_a_device_is_not_syncing_with_it() {
        let s = store().await;
        s.record_pairing("dev-1", None, "tok", 1, Some("/mnt/k"), 1_700_000_000)
            .await
            .unwrap();
        // `installed_at` is the caller's; `last_seen_at` was stamped from the
        // clock at insert, so it is already set. `last_synced_at` is not.
        assert_eq!(
            s.paired_device("dev-1")
                .await
                .unwrap()
                .unwrap()
                .last_synced_at,
            None
        );

        assert!(
            s.touch_device_seen("dev-1", Some("/mnt/elsewhere"))
                .await
                .unwrap()
        );
        let seen = s.paired_device("dev-1").await.unwrap().unwrap();
        assert_eq!(
            seen.last_synced_at, None,
            "plugging a reader in to charge it is not a sync"
        );
        assert_eq!(seen.last_mount_path.as_deref(), Some("/mnt/elsewhere"));
        assert_eq!(
            seen.installed_at, 1_700_000_000,
            "and the pairing did not restart"
        );

        assert!(s.stamp_device_sync("dev-1").await.unwrap());
        assert!(
            s.paired_device("dev-1")
                .await
                .unwrap()
                .unwrap()
                .last_synced_at
                .is_some()
        );
    }

    /// The fourth timestamp, and the pair of confusions migration `0021` exists
    /// to prevent: a reader that reached us over the LAN was **not in anybody's
    /// hand**, and what it sent is **not everything it had**.
    #[tokio::test]
    async fn reaching_us_over_the_network_is_neither_a_cable_nor_a_sync() {
        let s = store().await;
        s.record_pairing("dev-1", None, "tok", 1, Some("/mnt/k"), 1_700_000_000)
            .await
            .unwrap();
        let before = s.paired_device("dev-1").await.unwrap().unwrap();
        assert_eq!(before.last_wireless_at, None);
        assert_eq!(before.last_lan_addr, None);

        assert!(
            s.stamp_wireless_contact("dev-1", Some("192.168.1.20"))
                .await
                .unwrap()
        );
        let after = s.paired_device("dev-1").await.unwrap().unwrap();
        assert!(after.last_wireless_at.is_some());
        assert_eq!(after.last_lan_addr.as_deref(), Some("192.168.1.20"));
        assert_eq!(
            after.last_seen_at, before.last_seen_at,
            "a push from the next room is not a reader in your hand"
        );
        assert_eq!(
            after.last_synced_at, None,
            "what it chose to send is not everything it had"
        );
        assert_eq!(
            after.last_mount_path.as_deref(),
            Some("/mnt/k"),
            "and nothing over the wire says where a cable last was"
        );

        // A contact whose source address we could not name keeps the hint that
        // did work — `touch_device_seen`'s rule for `last_mount_path`, one
        // column over.
        assert!(s.stamp_wireless_contact("dev-1", None).await.unwrap());
        assert_eq!(
            s.paired_device("dev-1")
                .await
                .unwrap()
                .unwrap()
                .last_lan_addr
                .as_deref(),
            Some("192.168.1.20")
        );
    }

    #[tokio::test]
    async fn a_device_we_never_paired_with_stamps_nothing() {
        let s = store().await;
        // A `pairing.lua` naming somebody else's copy of readingbuddy. Every
        // writer here answers `false` rather than inventing a row.
        assert!(!s.touch_device_seen("dev-x", Some("/mnt/k")).await.unwrap());
        assert!(!s.stamp_device_sync("dev-x").await.unwrap());
        assert!(!s.set_device_label("dev-x", "nope").await.unwrap());
        assert!(
            !s.stamp_wireless_contact("dev-x", Some("10.0.0.1"))
                .await
                .unwrap()
        );
        assert!(s.list_paired_devices().await.unwrap().is_empty());
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
