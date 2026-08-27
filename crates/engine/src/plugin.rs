//! Installing readingbuddy's KOReader plugin onto a mounted reader (item 15a).
//!
//! **This is the only code in the engine that writes to somebody else's
//! hardware**, and it is a module of its own for that reason — the same reason
//! `device.rs` was split out of `koreader.rs`. A scan is not an import, and an
//! install is not a scan: `device.rs` is read-only by construction and its
//! whole test suite rests on that. If you want to audit the claim that
//! readingbuddy touches nothing on a reader but its own directory, this file is
//! the only one you have to read.
//!
//! # What the KOReader source settles
//!
//! Read from `koreader/master`, which `docs/koreader-format.md` §1 makes the
//! order of authority: the source is the spec, a fixture is only ever evidence.
//!
//! - **`pluginloader.lua:178-200`** — plugins load from `DEFAULT_PLUGIN_PATH`
//!   (`plugins/`) plus `extra_plugin_paths`. On a device
//!   `datastorage.lua:getDataDir()` returns `"."` (cwd is the install
//!   directory) and the loader registers the extra path only `if data_dir ~=
//!   "."`. So on a device `{data_dir}/plugins/` **is** `koreader/plugins/`, and
//!   we never write `extra_plugin_paths` — that would be editing the user's
//!   `settings.reader.lua`, which the standing rule forbids, and it would buy
//!   nothing. On a desktop KOReader the same directory is
//!   `$XDG_CONFIG_HOME/koreader/plugins/` (or `$KO_HOME/plugins/`), which is
//!   the development loop and is **not** what these functions target.
//! - **A directory is a plugin if it matches `".+%.koplugin"`** and holds
//!   `main.lua`. `_meta.lua` is optional, its fields are merged into the module
//!   except `name`, and the loader gates on none of them — so our `version`
//!   rides along inert.
//! - **Our directory survives a KOReader OTA update.** The update is a plain
//!   tar unpack with no `--delete`; the only removal is a manifest diff,
//!   `grep -xvFf .../ota/package.index /tmp/package.index | xargs -r rm -vf`,
//!   identical in the Kobo, Kindle, Cervantes and remarkable startup scripts.
//!   It deletes files that were in the *old* KOReader manifest and are absent
//!   from the *new* one; a path that never appeared in any KOReader manifest is
//!   untouched. This is what makes the install durable, and it is written down
//!   here so nobody has to re-derive it.
//! - **`kosync.koplugin` keeps its credentials in `settings/kosync.lua`**,
//!   outside its own directory. That is the one convention we break: it would
//!   leave a token behind after an uninstall we have promised is exact.
//!
//! # Why sha256 and never mtime
//!
//! These are FAT32 volumes. Two-second timestamp granularity, no permissions,
//! no symlinks, case-insensitive. Anything resting on mtime is subtly wrong on
//! real hardware and perfectly fine in `tempfile`, which is the worst of both.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::device::{is_koreader_mount, koreader_dir};
use crate::error::Result;
use crate::files::sha256_of;
use crate::koreader::{eval_table, get_int, get_str, sandboxed_lua};

/// The directory we own on a reader. Nothing outside it is ever written.
pub const PLUGIN_DIR_NAME: &str = "readingbuddy.koplugin";

/// The version of the plugin this binary carries. An integer, because
/// `_meta.lua` is compared with `>` and nothing about a plugin on a reader
/// needs semver's ordering rules.
pub const PLUGIN_VERSION: i64 = 1;

/// Written by us at install; read by the plugin at startup.
const PAIRING_FILE: &str = "pairing.lua";
/// Written by us at install; read by us at uninstall. Never read on-device.
const MANIFEST_FILE: &str = "installed.lua";

/// The plugin, embedded.
///
/// `include_str!` and an explicit list rather than an `include_dir` dependency:
/// a new crate buys nothing here, and the shipped file list being a
/// compile-time constant is exactly what an exact uninstall needs.
const FILES: &[(&str, &str)] = &[
    (
        "_meta.lua",
        include_str!("../koplugin/readingbuddy.koplugin/_meta.lua"),
    ),
    (
        "main.lua",
        include_str!("../koplugin/readingbuddy.koplugin/main.lua"),
    ),
];

/// Why an install or an uninstall refused.
///
/// A refusal is a decision, not a fault, and every arm here is a line in
/// `docs/decisions.md` turned into something a caller can branch on. It is an
/// enum rather than an `EngineError::Other(String)` for exactly that reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginRefusal {
    /// The path is not a KOReader install. The gate `device.rs` was built for.
    #[error("{} is not a KOReader install", path.display())]
    NotAKoreaderMount { path: PathBuf },
    /// The mount is a symlink. `device.rs`'s rule, applied to a write.
    #[error("{} is a symlink, not a mounted reader", path.display())]
    MountIsASymlink { path: PathBuf },
    /// A newer plugin than ours is already there.
    #[error(
        "a newer plugin is already installed (version {installed}; this readingbuddy carries {ours})"
    )]
    NewerAlreadyInstalled { installed: i64, ours: i64 },
    /// Files of ours that were edited on the device since we wrote them.
    #[error("these files were edited on the device since readingbuddy wrote them: {}", paths.join(", "))]
    Modified { paths: Vec<String> },
    /// Something is in our directory that we never put there.
    #[error("these files are in the plugin directory and readingbuddy did not put them there: {}", paths.join(", "))]
    Unrecognised { paths: Vec<String> },
    /// Installed, but not by a readingbuddy that left a manifest behind.
    #[error(
        "{} holds a plugin with no readingbuddy manifest, so it cannot be removed exactly",
        path.display()
    )]
    NoManifest { path: PathBuf },
}

/// What is on the reader, and what we know about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginStatus {
    pub mount: PathBuf,
    /// Exactly where an install would write. Shown to the user *before* it
    /// happens — `docs/decisions.md` requires that, and a path in a prompt is
    /// the difference between an explicit action and an automatic one.
    pub plugin_dir: PathBuf,
    pub installed: bool,
    pub installed_version: Option<i64>,
    pub our_version: i64,
    /// The device's `pairing.lua` names a device we have a row for.
    pub paired: bool,
    pub device_id: Option<String>,
    /// Files of ours whose bytes no longer match the manifest.
    pub modified: Vec<String>,
    /// Files in our directory that are not ours.
    pub unrecognised: Vec<String>,
}

impl PluginStatus {
    /// An install here would land on top of a plugin that is already there.
    ///
    /// Says nothing about *versions* — a reinstall of the same version is one
    /// of these too, and calling that an upgrade is what made the CLI print
    /// "upgraded v1 → v1" on a real device. Ask [`Self::is_version_upgrade`]
    /// when the distinction is the point.
    pub fn is_reinstall(&self) -> bool {
        self.installed
    }

    /// An install here would replace an *older* plugin with ours.
    pub fn is_version_upgrade(&self) -> bool {
        self.installed && self.installed_version.is_some_and(|v| v < self.our_version)
    }

    /// Nothing readingbuddy can do to this reader until the user resolves it.
    pub fn is_obstructed(&self) -> bool {
        !self.modified.is_empty()
            || !self.unrecognised.is_empty()
            || self.installed_version.is_some_and(|v| v > self.our_version)
    }
}

/// What an install did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub plugin_dir: PathBuf,
    pub device_id: String,
    pub version: i64,
    /// Relative paths written, sorted. `pairing.lua` and the manifest included:
    /// an install's whole footprint, so a caller can print it.
    pub written: Vec<String>,
    pub upgraded_from: Option<i64>,
}

/// What an uninstall did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallReport {
    pub plugin_dir: PathBuf,
    pub removed: Vec<String>,
    /// The pairing row we dropped, if there was one.
    pub forgot_device: Option<String>,
}

/// Where our plugin lives on a mount, whether or not it is there yet.
///
/// `None` when the path is not a KOReader install — the same three layouts
/// `koreader_dir` knows (Kindle `koreader/`, Kobo `.adds/koreader`, PocketBook
/// `applications/koreader`).
pub fn plugin_dir(mount: &Path) -> Option<PathBuf> {
    koreader_dir(mount).map(|k| k.join("plugins").join(PLUGIN_DIR_NAME))
}

/// The gate. Every public entry point calls this first, and it is the only
/// place the two refusals live.
fn check_mount(mount: &Path) -> Result<PathBuf> {
    // A symlink is checked before the contents, because the contents of a
    // symlink's target can be a perfectly good KOReader install that lives
    // somewhere we were never pointed at.
    if mount
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(PluginRefusal::MountIsASymlink {
            path: mount.to_path_buf(),
        }
        .into());
    }
    if !is_koreader_mount(mount) {
        return Err(PluginRefusal::NotAKoreaderMount {
            path: mount.to_path_buf(),
        }
        .into());
    }
    plugin_dir(mount).ok_or_else(|| {
        PluginRefusal::NotAKoreaderMount {
            path: mount.to_path_buf(),
        }
        .into()
    })
}

// ---- the four files we can read back off a device --------------------------

/// `path` → sha256, as recorded at install time.
type Manifest = BTreeMap<String, String>;

fn read_manifest(dir: &Path) -> Result<Option<(i64, Manifest)>> {
    let path = dir.join(MANIFEST_FILE);
    let Ok(src) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let lua = sandboxed_lua()?;
    let root = eval_table(&lua, &src, MANIFEST_FILE)?;
    let version = get_int(&root, "version").unwrap_or(0);
    let mut files = Manifest::new();
    if let Ok(mlua::Value::Table(t)) = root.get::<mlua::Value>("files") {
        for pair in t.pairs::<String, String>().flatten() {
            files.insert(pair.0, pair.1);
        }
    }
    Ok(Some((version, files)))
}

fn write_manifest(dir: &Path, version: i64, files: &Manifest) -> Result<()> {
    let mut s = String::from(
        "-- Written by readingbuddy. What it installed, and the sha256 of each\n\
         -- file as written, so that removing the plugin can remove exactly what\n\
         -- was put here and nothing a person added afterwards.\n\
         return {\n",
    );
    s.push_str(&format!("    version = {version},\n    files = {{\n"));
    for (name, hash) in files {
        s.push_str(&format!("        [\"{name}\"] = \"{hash}\",\n"));
    }
    s.push_str("    },\n}\n");
    std::fs::write(dir.join(MANIFEST_FILE), s)?;
    Ok(())
}

fn read_installed_version(dir: &Path) -> Result<Option<i64>> {
    let Ok(src) = std::fs::read_to_string(dir.join("_meta.lua")) else {
        return Ok(None);
    };
    let lua = sandboxed_lua()?;
    let root = eval_table(&lua, &src, "_meta.lua")?;
    Ok(get_int(&root, "version"))
}

fn read_device_id(dir: &Path) -> Result<Option<String>> {
    let Ok(src) = std::fs::read_to_string(dir.join(PAIRING_FILE)) else {
        return Ok(None);
    };
    let lua = sandboxed_lua()?;
    let root = eval_table(&lua, &src, PAIRING_FILE)?;
    Ok(get_str(&root, "device_id"))
}

fn write_pairing(dir: &Path, device_id: &str, token: &str, paired_at: i64) -> Result<()> {
    // No `host` and no `port`, deliberately. We do not know our LAN address,
    // there is no listener to name, and inventing the endpoint's shape before
    // the protocol exists is how it gets designed twice. The plugin treats a
    // missing endpoint as *not configured* and does nothing — which is
    // `docs/decisions.md`'s "fails closed" in its degenerate case.
    let body = format!(
        "-- Written by readingbuddy over USB. Do not edit.\n\
         return {{\n\
         \x20   device_id = \"{device_id}\",\n\
         \x20   token     = \"{token}\",\n\
         \x20   paired_at = {paired_at},\n\
         }}\n"
    );
    std::fs::write(dir.join(PAIRING_FILE), body)?;
    Ok(())
}

/// `n` bytes of system randomness, hex.
///
/// Used for the device id (16) and the pairing token (32). One function for
/// both because the only difference that matters is the length: neither value
/// is guessable and neither carries structure.
///
/// A failure here is the operating system declining to give us randomness,
/// which is not a condition any caller can act on differently — the one place
/// `EngineError::Other` is the honest variant rather than a lazy one.
pub fn mint_id(n: usize) -> Result<String> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes)
        .map_err(|e| crate::error::EngineError::Other(format!("no system randomness: {e}")))?;
    use std::fmt::Write as _;
    Ok(bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    }))
}

// ---- the operations --------------------------------------------------------

/// Read what is on the reader. Never writes.
pub fn inspect(mount: &Path) -> Result<PluginStatus> {
    let dir = check_mount(mount)?;
    let mut status = PluginStatus {
        mount: mount.to_path_buf(),
        plugin_dir: dir.clone(),
        installed: false,
        installed_version: None,
        our_version: PLUGIN_VERSION,
        paired: false,
        device_id: None,
        modified: Vec::new(),
        unrecognised: Vec::new(),
    };
    if !dir.is_dir() {
        return Ok(status);
    }
    status.installed = dir.join("main.lua").is_file();
    status.installed_version = read_installed_version(&dir)?;
    status.device_id = read_device_id(&dir)?;

    if let Some((_, manifest)) = read_manifest(&dir)? {
        let mut present: BTreeMap<String, PathBuf> = BTreeMap::new();
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == MANIFEST_FILE || name == PAIRING_FILE {
                continue;
            }
            present.insert(name, entry.path());
        }
        for (name, expected) in &manifest {
            // A file we installed and that is *gone* is not an obstruction —
            // reinstalling puts it back. Only a file whose bytes changed is a
            // person's work we must not overwrite.
            if let Some(path) = present.remove(name)
                && sha256_of(&path)? != *expected
            {
                status.modified.push(name.clone());
            }
        }
        status.unrecognised = present.into_keys().collect();
    }
    Ok(status)
}

/// The refusals shared by install and uninstall, checked against a status.
fn refuse_if_obstructed(status: &PluginStatus) -> Result<()> {
    if !status.modified.is_empty() {
        return Err(PluginRefusal::Modified {
            paths: status.modified.clone(),
        }
        .into());
    }
    if !status.unrecognised.is_empty() {
        return Err(PluginRefusal::Unrecognised {
            paths: status.unrecognised.clone(),
        }
        .into());
    }
    Ok(())
}

/// Install (or upgrade) the plugin, and write the pairing the device will use.
///
/// `device_id`, `token` and `paired_at` are the caller's to supply — the engine
/// facade does it, so that the same values reach `paired_devices` and
/// `pairing.lua` and there is exactly one place they are generated.
///
/// `paired_at` is **when the pairing happened**, not when this call ran. An
/// upgrade passes the stored `installed_at` back in; stamping the clock here
/// would leave the device claiming a pairing time the database disagrees with.
pub fn install(
    mount: &Path,
    device_id: &str,
    token: &str,
    paired_at: i64,
) -> Result<InstallReport> {
    let status = inspect(mount)?;
    refuse_if_obstructed(&status)?;
    if let Some(installed) = status.installed_version
        && installed > PLUGIN_VERSION
    {
        return Err(PluginRefusal::NewerAlreadyInstalled {
            installed,
            ours: PLUGIN_VERSION,
        }
        .into());
    }

    let dir = &status.plugin_dir;
    std::fs::create_dir_all(dir)?;

    let mut manifest = Manifest::new();
    let mut written = Vec::new();
    for (name, body) in FILES {
        let path = dir.join(name);
        std::fs::write(&path, body)?;
        manifest.insert((*name).to_string(), sha256_of(&path)?);
        written.push((*name).to_string());
    }
    write_manifest(dir, PLUGIN_VERSION, &manifest)?;
    write_pairing(dir, device_id, token, paired_at)?;
    written.push(MANIFEST_FILE.to_string());
    written.push(PAIRING_FILE.to_string());
    written.sort();

    Ok(InstallReport {
        plugin_dir: dir.clone(),
        device_id: device_id.to_string(),
        version: PLUGIN_VERSION,
        written,
        upgraded_from: status.installed_version.filter(|_| status.installed),
    })
}

/// Remove exactly what we installed, and nothing else.
///
/// Refuses rather than guessing when the directory holds something we did not
/// write, or when a file of ours has been edited. Leaving a plugin in place is
/// recoverable; deleting a person's work is not.
pub fn uninstall(mount: &Path) -> Result<UninstallReport> {
    let status = inspect(mount)?;
    let dir = status.plugin_dir.clone();
    if !dir.is_dir() {
        return Ok(UninstallReport {
            plugin_dir: dir,
            removed: Vec::new(),
            forgot_device: status.device_id,
        });
    }
    let Some((_, manifest)) = read_manifest(&dir)? else {
        return Err(PluginRefusal::NoManifest { path: dir }.into());
    };
    refuse_if_obstructed(&status)?;

    let mut removed = Vec::new();
    for name in manifest.keys() {
        let path = dir.join(name);
        if path.is_file() {
            std::fs::remove_file(&path)?;
            removed.push(name.clone());
        }
    }
    for name in [PAIRING_FILE, MANIFEST_FILE] {
        let path = dir.join(name);
        if path.is_file() {
            std::fs::remove_file(&path)?;
            removed.push(name.to_string());
        }
    }
    // `remove_dir` and not `remove_dir_all`: if anything at all is left, the
    // directory stays and so does whatever is in it. The refusals above should
    // have caught that already — this is the belt to their braces, and it is
    // the line that makes "uninstall is exact" true even if they are wrong.
    let _ = std::fs::remove_dir(&dir);
    removed.sort();

    Ok(UninstallReport {
        plugin_dir: dir,
        removed,
        forgot_device: status.device_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::tests::install as fake_reader;

    fn mount() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fake_reader(tmp.path());
        tmp
    }

    fn dir_of(mount: &Path) -> PathBuf {
        plugin_dir(mount).unwrap()
    }

    /// Every file under `root` as (relative path, sha256), skipping our own
    /// directory. This is the instrument the strongest test in this module
    /// needs: "write only inside our own plugin directory" is a claim about
    /// every other byte on the volume, and only a whole-tree snapshot can check
    /// it.
    fn snapshot(root: &Path) -> BTreeMap<String, String> {
        fn walk(base: &Path, dir: &Path, skip: &Path, out: &mut BTreeMap<String, String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path == skip {
                    continue;
                }
                if path.is_dir() {
                    walk(base, &path, skip, out);
                } else {
                    let rel = path.strip_prefix(base).unwrap().display().to_string();
                    out.insert(rel, sha256_of(&path).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &dir_of(root), &mut out);
        out
    }

    fn install_here(m: &Path) -> InstallReport {
        install(m, "dev-1", "tok-1", 1_700_000_000).unwrap()
    }

    fn refusal(e: crate::error::EngineError) -> PluginRefusal {
        match e {
            crate::error::EngineError::PluginRefused(r) => r,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_install_writes_our_directory_and_touches_nothing_else() {
        let tmp = mount();
        let before = snapshot(tmp.path());

        let report = install_here(tmp.path());

        assert_eq!(
            snapshot(tmp.path()),
            before,
            "an install changed something outside its own directory"
        );
        assert!(dir_of(tmp.path()).join("main.lua").is_file());
        assert_eq!(
            report.written,
            vec!["_meta.lua", "installed.lua", "main.lua", "pairing.lua"]
        );
        assert_eq!(report.upgraded_from, None);
    }

    #[test]
    fn install_then_uninstall_leaves_the_reader_as_it_found_it() {
        let tmp = mount();
        let before = snapshot(tmp.path());

        install_here(tmp.path());
        let report = uninstall(tmp.path()).unwrap();

        assert_eq!(snapshot(tmp.path()), before);
        assert!(
            !dir_of(tmp.path()).exists(),
            "the directory itself is part of what we put there"
        );
        assert_eq!(report.forgot_device.as_deref(), Some("dev-1"));
    }

    #[test]
    fn an_ordinary_directory_is_refused() {
        let plain = tempfile::tempdir().unwrap();
        let err = refusal(install(plain.path(), "d", "t", 0).unwrap_err());
        assert!(matches!(err, PluginRefusal::NotAKoreaderMount { .. }));
        assert_eq!(
            std::fs::read_dir(plain.path()).unwrap().count(),
            0,
            "a refusal must not create anything"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_reader_is_refused() {
        let tmp = mount();
        let link_home = tempfile::tempdir().unwrap();
        let link = link_home.path().join("KOBOeReader");
        std::os::unix::fs::symlink(tmp.path(), &link).unwrap();

        // The target really is a KOReader install: the refusal is about the
        // path we were handed, not about what is at the end of it.
        assert!(is_koreader_mount(&link));
        let err = refusal(install(&link, "d", "t", 0).unwrap_err());
        assert!(matches!(err, PluginRefusal::MountIsASymlink { .. }));
        assert!(!dir_of(tmp.path()).exists());
    }

    #[test]
    fn a_newer_plugin_is_not_overwritten() {
        let tmp = mount();
        install_here(tmp.path());
        let dir = dir_of(tmp.path());

        // A readingbuddy from the future got here first. Rewrite `_meta.lua`
        // *and* its manifest hash, so this is a clean newer install rather than
        // an edited file — the two refusals must not be confusable.
        let newer = format!("return {{ version = {} }}\n", PLUGIN_VERSION + 1);
        std::fs::write(dir.join("_meta.lua"), &newer).unwrap();
        let mut manifest = read_manifest(&dir).unwrap().unwrap().1;
        manifest.insert(
            "_meta.lua".into(),
            sha256_of(&dir.join("_meta.lua")).unwrap(),
        );
        write_manifest(&dir, PLUGIN_VERSION + 1, &manifest).unwrap();

        let err = refusal(install(tmp.path(), "d", "t", 0).unwrap_err());
        assert_eq!(
            err,
            PluginRefusal::NewerAlreadyInstalled {
                installed: PLUGIN_VERSION + 1,
                ours: PLUGIN_VERSION,
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("_meta.lua")).unwrap(),
            newer,
            "the refusal wrote anyway"
        );
    }

    #[test]
    fn a_file_the_user_edited_is_never_overwritten_and_never_deleted() {
        let tmp = mount();
        install_here(tmp.path());
        let dir = dir_of(tmp.path());
        let theirs = "-- I fixed a bug in your plugin\n";
        std::fs::write(dir.join("main.lua"), theirs).unwrap();

        let status = inspect(tmp.path()).unwrap();
        assert_eq!(status.modified, vec!["main.lua"]);
        assert!(status.is_obstructed());

        for err in [
            install(tmp.path(), "d", "t", 0).unwrap_err(),
            uninstall(tmp.path()).unwrap_err(),
        ] {
            assert!(matches!(refusal(err), PluginRefusal::Modified { .. }));
        }
        assert_eq!(
            std::fs::read_to_string(dir.join("main.lua")).unwrap(),
            theirs
        );
    }

    #[test]
    fn something_we_did_not_write_blocks_both_operations() {
        let tmp = mount();
        install_here(tmp.path());
        let dir = dir_of(tmp.path());
        std::fs::write(dir.join("notes.txt"), "mine\n").unwrap();

        let status = inspect(tmp.path()).unwrap();
        assert_eq!(status.unrecognised, vec!["notes.txt"]);

        let err = refusal(uninstall(tmp.path()).unwrap_err());
        assert!(matches!(err, PluginRefusal::Unrecognised { .. }));
        assert!(dir.join("notes.txt").is_file());
        assert!(dir.join("main.lua").is_file(), "nothing was removed");
    }

    #[test]
    fn a_plugin_with_no_manifest_is_not_ours_to_remove() {
        let tmp = mount();
        let dir = dir_of(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.lua"), "-- somebody else's\n").unwrap();

        let err = refusal(uninstall(tmp.path()).unwrap_err());
        assert!(matches!(err, PluginRefusal::NoManifest { .. }));
        assert!(dir.join("main.lua").is_file());
    }

    #[test]
    fn uninstalling_a_reader_that_never_had_it_is_not_an_error() {
        let tmp = mount();
        let report = uninstall(tmp.path()).unwrap();
        assert!(report.removed.is_empty());
        assert_eq!(report.forgot_device, None);
    }

    #[test]
    fn an_upgrade_reports_what_it_replaced() {
        let tmp = mount();
        install_here(tmp.path());
        let again = install(tmp.path(), "dev-1", "tok-1", 1_700_000_001).unwrap();
        assert_eq!(again.upgraded_from, Some(PLUGIN_VERSION));
        assert_eq!(
            inspect(tmp.path()).unwrap().device_id.as_deref(),
            Some("dev-1")
        );
    }

    /// The reason `_meta.lua` may never grow a `require("gettext")` — which
    /// every KOReader plugin's does, and which a later contributor will add by
    /// reflex. Read the shipped bytes through the same VM the installer uses.
    #[test]
    fn the_shipped_meta_is_readable_by_the_sidecar_sandbox() {
        let lua = sandboxed_lua().unwrap();
        let meta = FILES.iter().find(|(n, _)| *n == "_meta.lua").unwrap().1;
        let t = eval_table(&lua, meta, "_meta.lua").unwrap();
        assert_eq!(get_int(&t, "version"), Some(PLUGIN_VERSION));
        // Comment lines are skipped: the header of that file *explains* why
        // there is no `require` in it, so a bare `contains` fails on the
        // explanation and passes on nothing.
        let code = meta
            .lines()
            .filter(|l| !l.trim_start().starts_with("--"))
            .collect::<String>();
        assert!(
            !code.contains("require("),
            "a `require` in _meta.lua makes the installed version unreadable"
        );
    }

    /// `pairing.lua` is written by us and read by on-device Lua, so it has to
    /// be a table literal in both directions.
    #[test]
    fn the_pairing_we_write_reads_back() {
        let tmp = mount();
        install(tmp.path(), "abc123", "s3cret", 1_700_000_000).unwrap();
        let src = std::fs::read_to_string(dir_of(tmp.path()).join(PAIRING_FILE)).unwrap();

        let lua = sandboxed_lua().unwrap();
        let t = eval_table(&lua, &src, PAIRING_FILE).unwrap();
        assert_eq!(get_str(&t, "device_id").as_deref(), Some("abc123"));
        assert_eq!(get_str(&t, "token").as_deref(), Some("s3cret"));
        assert_eq!(get_int(&t, "paired_at"), Some(1_700_000_000));
        assert!(
            !src.contains("host") && !src.contains("port"),
            "no endpoint is written until there is a listener to name"
        );
    }

    #[test]
    fn minted_ids_are_the_length_asked_for_and_not_each_other() {
        let a = mint_id(16).unwrap();
        let b = mint_id(16).unwrap();
        assert_eq!(a.len(), 32);
        assert_eq!(mint_id(32).unwrap().len(), 64);
        assert_ne!(a, b);
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Install then uninstall is the identity on the rest of the
            /// volume, whatever else is on it. The example-based test above
            /// fixes one tree; this one says the property does not depend on
            /// which.
            #[test]
            fn install_and_uninstall_restore_any_tree(
                extras in proptest::collection::vec("[a-z]{1,8}", 0..6)
            ) {
                let tmp = mount();
                for (i, name) in extras.iter().enumerate() {
                    let d = tmp.path().join("koreader").join("plugins").join(format!("{name}.koplugin"));
                    std::fs::create_dir_all(&d).unwrap();
                    std::fs::write(d.join("main.lua"), format!("-- plugin {i}\n")).unwrap();
                }
                let before = snapshot(tmp.path());
                install_here(tmp.path());
                uninstall(tmp.path()).unwrap();
                prop_assert_eq!(snapshot(tmp.path()), before);
            }
        }
    }
}
