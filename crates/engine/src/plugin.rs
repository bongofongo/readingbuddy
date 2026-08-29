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
/// Written by **the plugin, on the device**; never by us, never hashed.
///
/// The learned half of the discovery ladder — where a computer was last
/// actually reached — as against `pairing.lua`, which is what we told the
/// reader over the cable. They are two files because they have two authors,
/// and the split is what makes the manifest check mean anything: every byte in
/// `installed.lua` is a byte we wrote, so a mismatch is a person's edit.
///
/// It must be **skipped by [`inspect`] and removed by [`uninstall`]**, and that
/// is not tidiness. `inspect` files anything absent from the manifest under
/// `unrecognised`, and `refuse_if_obstructed` turns that into a refusal of
/// *install and uninstall both* — so a plugin that cached its endpoint beside
/// itself would brick its own installer the first time it ran.
const ENDPOINT_FILE: &str = "endpoint.lua";

/// The three names in our directory that the manifest does not describe.
///
/// One list rather than three comparisons, because `inspect` and `uninstall`
/// have to agree about it exactly: a name skipped by one and not the other is
/// either a file that blocks an uninstall or a file left behind by one.
const UNHASHED_FILES: [&str; 3] = [MANIFEST_FILE, PAIRING_FILE, ENDPOINT_FILE];

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
    /// **Our** id for this reader, when one of its pairings is ours.
    ///
    /// [`inspect`] cannot tell whose is whose — it has no database — so it
    /// fills this with the file's first entry, which is exactly what it always
    /// meant while a reader could hold only one. `Engine::plugin_status`
    /// re-points it at the entry it actually has a row for, so a reader paired
    /// with somebody else's computer reports `None` here rather than a
    /// stranger's id.
    pub device_id: Option<String>,
    /// Every computer this reader is paired with, ours included, in file order.
    ///
    /// Not `device_id`'s replacement: that one answers *which of these is us*,
    /// and this answers *who else is there* — the question the single-computer
    /// file could not represent, and the reason a second install used to kill
    /// the first one's pairing in silence.
    pub pairings: Vec<Pairing>,
    /// Files of ours whose bytes no longer match the manifest.
    pub modified: Vec<String>,
    /// Files in our directory that are not ours.
    pub unrecognised: Vec<String>,
}

/// What an install onto this reader would do — the one question a screen
/// branches on (item 55).
///
/// The three predicates below it are still the truth and still overlap by
/// design; this collapses them into the **single mutually-exclusive** answer a
/// frontend needs, so that comparing `installed_version` against `our_version`
/// happens once, here, rather than once in `commands/ko.rs` and again in
/// TypeScript. That second spelling is what item 17 exists to prevent, and the
/// CLI had already grown the first.
///
/// **`Obstructed` wins over every version case**, because it is the one that
/// gates the action: a reader carrying a plugin we edited, a stranger's file in
/// our directory, or a version newer than ours is a reader we will not write
/// to, and *which* of those it is comes from `modified` / `unrecognised` /
/// `installed_version` beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginCondition {
    /// Nothing of ours is there. An install writes.
    Absent,
    /// Ours, at the version we carry. An install rewrites it in place.
    Current,
    /// Ours, older. An install upgrades.
    Upgradable,
    /// Ours, carrying no version at all — a `_meta.lua` we could not read a
    /// number out of. An install rewrites it, and this is deliberately not
    /// folded into `Upgradable`: "older than ours" is a fact and "we cannot
    /// tell" is not the same fact.
    Unversioned,
    /// We will not write here until the user resolves it.
    Obstructed,
}

impl PluginStatus {
    /// What an install here would do. See [`PluginCondition`].
    pub fn condition(&self) -> PluginCondition {
        if self.is_obstructed() {
            return PluginCondition::Obstructed;
        }
        if !self.installed {
            return PluginCondition::Absent;
        }
        match self.installed_version {
            None => PluginCondition::Unversioned,
            Some(v) if v < self.our_version => PluginCondition::Upgradable,
            // `>` was taken by `is_obstructed` above, so this is equality.
            Some(_) => PluginCondition::Current,
        }
    }

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
    ///
    /// [`uninstall`] fills it from the file's first entry and
    /// `Engine::uninstall_plugin` re-points it, for [`PluginStatus::device_id`]'s
    /// reason: only the database knows which of a reader's computers we are.
    pub forgot_device: Option<String>,
    /// Every pairing this uninstall destroyed, ours and other computers'.
    ///
    /// Taking the plugin off ends *all* of them, because the file goes with it
    /// — so the report says so rather than naming only the one we had a row
    /// for. Nothing else on this volume is touched, which is the promise the
    /// whole-tree snapshot test checks.
    pub removed_pairings: Vec<String>,
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

/// One computer this reader is paired with, as `pairing.lua` records it.
///
/// **It carries no token**, and that is structural rather than an omission:
/// this type hangs off [`PluginStatus`], which crosses into a DTO, so a secret
/// field here would reach the wire the first time somebody surfaced the list.
/// The token stays in [`StoredPairing`], which is private to this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    /// Who this reader is **to that computer**. Each computer mints its own,
    /// so this doubles as the handle a computer recognises its own entry by.
    pub device_id: String,
    /// What to call the computer. The machine's hostname at install time, and
    /// `None` for an entry written before there was one — a menu falls back to
    /// the id's first bytes rather than inventing a name.
    pub name: Option<String>,
    pub paired_at: Option<i64>,
}

/// A [`Pairing`] plus the secret. Never leaves this module.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredPairing {
    device_id: String,
    token: String,
    name: Option<String>,
    paired_at: i64,
}

impl StoredPairing {
    fn public(&self) -> Pairing {
        Pairing {
            device_id: self.device_id.clone(),
            name: self.name.clone(),
            paired_at: Some(self.paired_at),
        }
    }
}

/// `s` as a Lua string literal.
///
/// Every value written here has been hex or nothing until now, so the old
/// `format!("\"{device_id}\"")` was safe by accident. It stops being safe the
/// moment an entry carries a **name**: a hostname is not our string, and one
/// containing a quote would write a `pairing.lua` that does not parse — which
/// the reader reports as *not paired* and the installer, reading its own
/// output back, reports as a fresh device. Escaping is the cheaper end of that.
fn lua_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            // A literal NUL is legal in a Lua string and unreadable in a file
            // somebody may open on a device; the numeric escape is both.
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\{:03}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Every computer the reader's `pairing.lua` names, with their tokens.
///
/// **Two shapes are accepted and only one is written.** Item 15a wrote a flat
/// table holding a single `device_id`/`token`/`paired_at`, on the reasoning
/// that there was one computer; that is exactly the bug 15b opens with, since
/// a second readingbuddy install overwrote the file and killed the first one's
/// pairing while its `paired_devices` row still claimed it. So the file is now
/// a list under `computers`, and a flat table is read as a one-entry list —
/// detected by *shape* rather than by a version number, because a number
/// nothing branches on is decoration and the reader's Lua has to make the same
/// test anyway.
///
/// An entry missing an id or a token is dropped rather than repaired: half a
/// credential proves nothing, and keeping it would let the installer decide it
/// had found its own entry and then write a token beside somebody else's name.
fn read_stored_pairings(dir: &Path) -> Result<Vec<StoredPairing>> {
    let Ok(src) = std::fs::read_to_string(dir.join(PAIRING_FILE)) else {
        return Ok(Vec::new());
    };
    let lua = sandboxed_lua()?;
    let root = eval_table(&lua, &src, PAIRING_FILE)?;

    let entry = |t: &mlua::Table| -> Option<StoredPairing> {
        let device_id = get_str(t, "device_id").filter(|s| !s.is_empty())?;
        let token = get_str(t, "token").filter(|s| !s.is_empty())?;
        Some(StoredPairing {
            device_id,
            token,
            name: get_str(t, "name").filter(|s| !s.is_empty()),
            paired_at: get_int(t, "paired_at").unwrap_or(0),
        })
    };

    if let Ok(mlua::Value::Table(list)) = root.get::<mlua::Value>("computers") {
        let mut out = Vec::new();
        for value in list.sequence_values::<mlua::Value>().flatten() {
            if let mlua::Value::Table(t) = value
                && let Some(p) = entry(&t)
            {
                out.push(p);
            }
        }
        return Ok(out);
    }
    Ok(entry(&root).into_iter().collect())
}

/// Write the whole list back.
///
/// There is no partial write: the file is small, it is ours, and rewriting it
/// whole is what lets the installer keep another computer's entry byte-for-byte
/// without parsing its way around it.
fn write_pairings(dir: &Path, computers: &[StoredPairing]) -> Result<()> {
    // Still no `host` and no `port` in an entry, and now for a sharper reason
    // than 15a's "there is no listener yet": the endpoint the device *learns*
    // is the device's fact and lives in `endpoint.lua`, which the device owns
    // and we never hash. `name` is the wired hint — a hostname is stable across
    // a DHCP lease, which is the rung of the ladder an address cannot be.
    let mut body = String::from(
        "-- Written by readingbuddy over USB. Do not edit.\n\
         --\n\
         -- A *list*, because a reader can be paired with more than one computer.\n\
         -- Each entry is one computer: the id this reader has to that computer,\n\
         -- the secret it proves itself with, and a name to show in the menu. An\n\
         -- installer rewrites its own entry and leaves every other one alone.\n\
         return {\n\
         \x20   computers = {\n",
    );
    for c in computers {
        body.push_str("        {\n");
        body.push_str(&format!(
            "            device_id = {},\n",
            lua_quote(&c.device_id)
        ));
        body.push_str(&format!(
            "            token     = {},\n",
            lua_quote(&c.token)
        ));
        if let Some(name) = &c.name {
            body.push_str(&format!("            name      = {},\n", lua_quote(name)));
        }
        body.push_str(&format!("            paired_at = {},\n", c.paired_at));
        body.push_str("        },\n");
    }
    body.push_str("    },\n}\n");
    std::fs::write(dir.join(PAIRING_FILE), body)?;
    Ok(())
}

/// What to call this computer in the reader's menu.
///
/// The hostname, because it is the name the *router* already knows this machine
/// by — which is the same string rung 4 of the discovery ladder resolves — so
/// one value serves the label and the lookup rather than two that can disagree.
/// `None` when the OS will not say, which is ordinary and not an error: an
/// entry with no name is drawn from its id.
pub fn this_computer() -> Option<String> {
    let name = gethostname::gethostname().to_string_lossy().into_owned();
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
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
        pairings: Vec::new(),
        modified: Vec::new(),
        unrecognised: Vec::new(),
    };
    if !dir.is_dir() {
        return Ok(status);
    }
    status.installed = dir.join("main.lua").is_file();
    status.installed_version = read_installed_version(&dir)?;
    status.pairings = read_stored_pairings(&dir)?
        .iter()
        .map(StoredPairing::public)
        .collect();
    status.device_id = status.pairings.first().map(|p| p.device_id.clone());

    if let Some((_, manifest)) = read_manifest(&dir)? {
        let mut present: BTreeMap<String, PathBuf> = BTreeMap::new();
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if UNHASHED_FILES.contains(&name.as_str()) {
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
///
/// **Our entry is rewritten and every other computer's is kept.** This used to
/// overwrite `pairing.lua` whole, so plugging a reader into a second
/// readingbuddy install silently killed the first one's pairing while its
/// `paired_devices` row went on claiming it — a failure with no symptom on
/// either machine until something tried to use the token. The entry we rewrite
/// is the one carrying `device_id`; the caller resolved that against its own
/// database, which is the only place the answer exists.
pub fn install(
    mount: &Path,
    device_id: &str,
    token: &str,
    paired_at: i64,
    name: Option<&str>,
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

    // Read the list back off the device rather than trusting `status`: the
    // public `Pairing` deliberately drops the token, and rewriting the file
    // from it would blank every other computer's secret.
    let mut computers = read_stored_pairings(dir)?;
    let ours = StoredPairing {
        device_id: device_id.to_string(),
        token: token.to_string(),
        name: name.map(str::to_string),
        paired_at,
    };
    match computers.iter().position(|c| c.device_id == device_id) {
        // In place, so the menu's order does not shuffle on every upgrade.
        Some(i) => computers[i] = ours,
        None => computers.push(ours),
    }
    write_pairings(dir, &computers)?;
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
    let removed_pairings: Vec<String> = status
        .pairings
        .iter()
        .map(|p| p.device_id.clone())
        .collect();
    if !dir.is_dir() {
        return Ok(UninstallReport {
            plugin_dir: dir,
            removed: Vec::new(),
            forgot_device: status.device_id,
            removed_pairings,
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
    // `endpoint.lua` is in this list because the device wrote it and nobody
    // else will: leaving it would break "uninstall is exact" with a file whose
    // whole content is where to find this computer.
    for name in UNHASHED_FILES {
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
        removed_pairings,
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
        install(m, "dev-1", "tok-1", 1_700_000_000, Some("desk")).unwrap()
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
        let err = refusal(install(plain.path(), "d", "t", 0, None).unwrap_err());
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
        let err = refusal(install(&link, "d", "t", 0, None).unwrap_err());
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

        let err = refusal(install(tmp.path(), "d", "t", 0, None).unwrap_err());
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
            install(tmp.path(), "d", "t", 0, None).unwrap_err(),
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
        let again = install(tmp.path(), "dev-1", "tok-1", 1_700_000_001, Some("desk")).unwrap();
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
        install(tmp.path(), "abc123", "s3cret", 1_700_000_000, Some("desk")).unwrap();
        let dir = dir_of(tmp.path());
        let src = std::fs::read_to_string(dir.join(PAIRING_FILE)).unwrap();

        let read = read_stored_pairings(&dir).unwrap();
        assert_eq!(
            read,
            vec![StoredPairing {
                device_id: "abc123".into(),
                token: "s3cret".into(),
                name: Some("desk".into()),
                paired_at: 1_700_000_000,
            }]
        );
        assert!(
            !src.contains("host") && !src.contains("port"),
            "no endpoint is written until there is a listener to name"
        );
    }

    /// The bug this stage exists to fix, end to end and on one volume.
    ///
    /// Two readingbuddy installs, one reader. The second used to read the
    /// file's single `device_id`, find no row for it, mint a fresh identity and
    /// **overwrite** — after which the first machine held a `paired_devices`
    /// row for a token the device no longer had, with nothing on either side
    /// looking wrong until something tried to use it.
    #[test]
    fn a_second_computer_does_not_steal_the_reader_from_the_first() {
        let tmp = mount();
        install(tmp.path(), "dev-a", "tok-a", 1_700_000_000, Some("laptop")).unwrap();
        install(tmp.path(), "dev-b", "tok-b", 1_700_000_100, Some("desktop")).unwrap();

        let dir = dir_of(tmp.path());
        let stored = read_stored_pairings(&dir).unwrap();
        assert_eq!(
            stored
                .iter()
                .map(|c| c.device_id.as_str())
                .collect::<Vec<_>>(),
            vec!["dev-a", "dev-b"],
        );
        assert_eq!(
            stored[0].token, "tok-a",
            "the first computer's secret survived the second computer's install"
        );
        assert_eq!(stored[0].name.as_deref(), Some("laptop"));
        assert_eq!(stored[0].paired_at, 1_700_000_000);

        // And re-installing the first rewrites *its* entry in place, so the
        // menu's order does not shuffle under the user on every upgrade.
        install(tmp.path(), "dev-a", "tok-a", 1_700_000_000, Some("laptop2")).unwrap();
        let again = read_stored_pairings(&dir).unwrap();
        assert_eq!(
            again
                .iter()
                .map(|c| c.device_id.as_str())
                .collect::<Vec<_>>(),
            vec!["dev-a", "dev-b"],
        );
        assert_eq!(again[0].name.as_deref(), Some("laptop2"));
        assert_eq!(again[1].token, "tok-b");
    }

    /// Item 15a's flat file is still on every reader paired before this, so it
    /// is read as the one-entry list it always meant — and an install onto one
    /// *upgrades the shape* rather than adding a second entry for the computer
    /// that is already there.
    #[test]
    fn the_flat_file_item_15a_wrote_is_read_as_one_computer() {
        let tmp = mount();
        install_here(tmp.path());
        let dir = dir_of(tmp.path());
        std::fs::write(
            dir.join(PAIRING_FILE),
            "-- Written by readingbuddy over USB. Do not edit.\n\
             return {\n\
             \x20   device_id = \"old-id\",\n\
             \x20   token     = \"old-token\",\n\
             \x20   paired_at = 1690000000,\n\
             }\n",
        )
        .unwrap();

        let status = inspect(tmp.path()).unwrap();
        assert_eq!(status.device_id.as_deref(), Some("old-id"));
        assert_eq!(status.pairings.len(), 1);
        assert_eq!(status.pairings[0].name, None, "15a wrote no name");
        assert_eq!(status.pairings[0].paired_at, Some(1_690_000_000));

        install(
            tmp.path(),
            "old-id",
            "old-token",
            1_690_000_000,
            Some("desk"),
        )
        .unwrap();
        let stored = read_stored_pairings(&dir).unwrap();
        assert_eq!(stored.len(), 1, "the same computer, in the list shape");
        assert_eq!(stored[0].token, "old-token");
        assert_eq!(stored[0].paired_at, 1_690_000_000);
    }

    /// Half a credential proves nothing, so it is dropped rather than kept —
    /// otherwise an installer can decide it has found its own entry and write a
    /// token beside a name that is not its own.
    #[test]
    fn an_entry_missing_its_id_or_its_token_is_not_a_computer() {
        let tmp = mount();
        install_here(tmp.path());
        let dir = dir_of(tmp.path());
        std::fs::write(
            dir.join(PAIRING_FILE),
            "return { computers = {\n\
             \x20 { device_id = \"a\", token = \"t\" },\n\
             \x20 { device_id = \"b\" },\n\
             \x20 { token = \"t\" },\n\
             \x20 { device_id = \"\", token = \"t\" },\n\
             \x20 \"not a table\",\n\
             } }\n",
        )
        .unwrap();
        let stored = read_stored_pairings(&dir).unwrap();
        assert_eq!(
            stored
                .iter()
                .map(|c| c.device_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a"],
        );
    }

    /// A hostname is not our string. Until this stage every value written into
    /// `pairing.lua` was hex, so raw interpolation was safe by accident; a name
    /// carrying a quote would write a file that does not parse, which the
    /// reader reports as *not paired* and the installer reads back as a fresh
    /// device.
    #[test]
    fn a_name_that_could_break_the_file_is_escaped() {
        let tmp = mount();
        let hostile = "o\"liver's\\box\nnewline";
        install(tmp.path(), "dev-1", "tok-1", 1_700_000_000, Some(hostile)).unwrap();
        let stored = read_stored_pairings(&dir_of(tmp.path())).unwrap();
        assert_eq!(stored[0].name.as_deref(), Some(hostile));
    }

    /// The collision this item opens with: a cache file beside the plugin used
    /// to land in `unrecognised`, which `refuse_if_obstructed` turns into a
    /// refusal of **install and uninstall both** — a plugin bricking its own
    /// installer the first time it wrote down where to find us.
    #[test]
    fn the_endpoint_the_device_writes_blocks_neither_operation() {
        let tmp = mount();
        install_here(tmp.path());
        let dir = dir_of(tmp.path());
        let learned = "return { [\"dev-1\"] = { host = \"192.168.1.20\", port = 51861 } }\n";
        std::fs::write(dir.join(ENDPOINT_FILE), learned).unwrap();

        let status = inspect(tmp.path()).unwrap();
        assert!(status.unrecognised.is_empty(), "{status:?}");
        assert!(!status.is_obstructed());

        // An install leaves it alone: it is the device's learned state and an
        // upgrade is not a reason to forget where we live.
        install_here(tmp.path());
        assert_eq!(
            std::fs::read_to_string(dir.join(ENDPOINT_FILE)).unwrap(),
            learned
        );

        // An uninstall takes it, because "uninstall is exact" cannot leave
        // behind a file whose whole content is where to find this computer.
        let report = uninstall(tmp.path()).unwrap();
        assert!(report.removed.contains(&ENDPOINT_FILE.to_string()));
        assert!(!dir.exists());
    }

    /// The pure function the on-device menu runs, executed against exactly the
    /// bytes the installer writes.
    ///
    /// `main.lua` is 90 lines that no test has ever run: the only Lua a test
    /// parsed was `_meta.lua`, and that test protects the version-reading trick
    /// rather than the plugin's behaviour. Defensible for a menu entry, and not
    /// defensible for the selection logic a push verb will branch on. `require`
    /// is stubbed with a table that answers every index and every call with
    /// itself, which is enough to get the module's return value back without
    /// KOReader present.
    fn run_plugin_lua(script: &str) -> mlua::Result<String> {
        let lua = mlua::Lua::new();
        let stub = lua.create_table()?;
        let meta = lua.create_table()?;
        let s = stub.clone();
        meta.set(
            "__index",
            lua.create_function(move |_, (_t, _k): (mlua::Table, mlua::Value)| Ok(s.clone()))?,
        )?;
        let s = stub.clone();
        meta.set(
            "__call",
            lua.create_function(move |_, _: mlua::MultiValue| Ok(s.clone()))?,
        )?;
        stub.set_metatable(Some(meta));
        let s = stub.clone();
        lua.globals().set(
            "require",
            lua.create_function(move |_, _: String| Ok(s.clone()))?,
        )?;

        let main = FILES.iter().find(|(n, _)| *n == "main.lua").unwrap().1;
        let module: mlua::Table = lua.load(main).set_name("main.lua").eval()?;
        lua.globals().set("RB", module)?;
        // Stringified *inside* the VM's lifetime: an `mlua::Value` handed back
        // outlives the `Lua` it points into and panics with "Lua instance is
        // destroyed" on the first read of it.
        lua.load(script).eval::<mlua::Value>().map(|v| match v {
            mlua::Value::String(s) => s.to_string_lossy().to_string(),
            mlua::Value::Integer(i) => i.to_string(),
            other => format!("{other:?}"),
        })
    }

    #[test]
    fn every_shipped_lua_file_compiles() {
        // The floor of the Lua gate: a syntax error in a file we `include_str!`
        // is a compile-time constant that bricks a reader at runtime, and
        // nothing else in this repo would have noticed.
        let lua = mlua::Lua::new();
        for (name, body) in FILES {
            lua.load(*body)
                .set_name(*name)
                .into_function()
                .unwrap_or_else(|e| panic!("{name} does not compile: {e}"));
        }
    }

    #[test]
    fn the_plugin_reads_every_pairing_shape_the_installer_can_write() {
        let tmp = mount();
        install(tmp.path(), "dev-a", "tok-a", 1_700_000_000, Some("laptop")).unwrap();
        install(tmp.path(), "dev-b", "tok-b", 1_700_000_100, None).unwrap();
        let list = std::fs::read_to_string(dir_of(tmp.path()).join(PAIRING_FILE)).unwrap();

        // The list shape, straight off the device: both computers, in order,
        // and the nameless one drawn from its id exactly as the menu draws it.
        let names = run_plugin_lua(&format!(
            "local raw = load({list:?})()
             local out = {{}}
             for _, e in ipairs(RB.normalisePairings(raw)) do
                 table.insert(out, RB.computerName(e))
             end
             return table.concat(out, ',')"
        ))
        .unwrap();
        assert_eq!(names, "laptop,dev-b");

        // Item 15a's flat shape is one computer, and a table that names nobody
        // is zero — never a nil the caller has to test for.
        for (src, want) in [
            (r#"return { device_id = "x", token = "t" }"#, 1),
            (r#"return { device_id = "x" }"#, 0),
            (r#"return { computers = {} }"#, 0),
            (r#"return { }"#, 0),
            (r#"return "nonsense""#, 0),
        ] {
            let n =
                run_plugin_lua(&format!("return #RB.normalisePairings(load({src:?})())")).unwrap();
            assert_eq!(n, want.to_string(), "{src}");
        }
    }

    /// The discovery ladder's **ordering and fallback**, run as the device runs
    /// it. This is the part with bugs in it, and a ladder that can only be
    /// exercised by carrying a laptop between subnets is a ladder with no
    /// tests — `watch.rs`'s sentence about `notify`, one subsystem over.
    #[test]
    fn the_ladder_tries_the_cheap_rungs_first_and_degrades() {
        let rungs = |cached: &str, name: &str| {
            run_plugin_lua(&format!(
                "local out = {{}}
                 for _, r in ipairs(RB.ladder({cached}, {name})) do
                     table.insert(out, r.kind .. ':' .. r.host)
                 end
                 return table.concat(out, ' ')"
            ))
            .unwrap()
        };

        // Everything known: the cached address, then the two broadcasts — the
        // global one and the cached /24's, for the APs that filter one and pass
        // the other — then the name, bare and with each suffix.
        assert_eq!(
            rungs(r#"{ host = "192.168.1.20", port = 51000 }"#, r#""desk""#),
            "cached:192.168.1.20 broadcast:255.255.255.255 broadcast:192.168.1.255 \
             dns:desk dns:desk.lan dns:desk.home.arpa"
        );

        // A first run has no cache, so the ladder starts at the broadcast — and
        // there is no /24 to derive a directed broadcast from.
        assert_eq!(
            rungs("nil", r#""desk""#),
            "broadcast:255.255.255.255 dns:desk dns:desk.lan dns:desk.home.arpa"
        );

        // An entry written before there were names (item 15a's flat file) still
        // gets the rungs that do not need one, rather than no ladder at all.
        assert_eq!(
            rungs(r#"{ host = "10.0.0.5" }"#, "nil"),
            "cached:10.0.0.5 broadcast:255.255.255.255 broadcast:10.0.0.255"
        );

        // A cached *hostname* has no subnet to broadcast into, so the directed
        // rung is skipped rather than built out of a regex that half-matched.
        assert_eq!(
            rungs(r#"{ host = "desk.lan" }"#, "nil"),
            "cached:desk.lan broadcast:255.255.255.255"
        );

        // A hostname that is really an address would make the DNS rungs a
        // second copy of rung 1 — and `10.0.0.5.lan` resolves nowhere.
        assert_eq!(rungs("nil", r#""10.0.0.5""#), "broadcast:255.255.255.255");
    }

    /// `endpoint.lua` round-trips through the plugin's own writer and reader,
    /// and the installer's parser is not involved — this file is the device's.
    #[test]
    fn the_endpoint_cache_round_trips_and_is_stable() {
        let script = "local written = RB.serialiseEndpoints({\n\
             \x20 [\"bbb\"] = { host = \"10.0.0.9\", port = 51001, seen_at = 20 },\n\
             \x20 [\"aaa\"] = { host = \"192.168.1.20\", port = 51000, seen_at = 10 },\n\
             })\n\
             local back = RB.parseEndpoints(load(written)())\n\
             local again = RB.serialiseEndpoints(back)\n\
             return (written == again and 'stable ' or 'UNSTABLE ')\n\
                 .. back.aaa.host .. ':' .. back.aaa.port\n\
                 .. ' ' .. back.bbb.host .. ':' .. back.bbb.port";
        assert_eq!(
            run_plugin_lua(script).unwrap(),
            "stable 192.168.1.20:51000 10.0.0.9:51001"
        );

        // Absent, unreadable and half-written are one answer, and the answer is
        // a table — never a nil the caller has to test for.
        for bad in [
            "nil",
            r#""not a table""#,
            r#"{ ["a"] = { port = 1 } }"#,
            r#"{ ["a"] = { host = "" } }"#,
            r#"{ [1] = { host = "10.0.0.1" } }"#,
        ] {
            let n = run_plugin_lua(&format!(
                "local n = 0
                 for _ in pairs(RB.parseEndpoints({bad})) do n = n + 1 end
                 return n"
            ))
            .unwrap();
            assert_eq!(n, "0", "{bad}");
        }
    }

    /// The challenge strings the plugin builds must be byte-identical to the
    /// ones the listener signs — they are the only thing keeping the two
    /// implementations of the same protocol honest, and they are built by
    /// `string.format` on one side and `format!` on the other.
    #[test]
    fn the_challenges_the_plugin_builds_match_the_listeners() {
        use crate::wireless::{body_challenge, here_challenge, open_challenge};
        let built = run_plugin_lua(
            "return string.format('here:%d:%s:%d', 1, 'n1', 51000) .. '|' ..
                    string.format('open:%d:%s', 1, 'n1') .. '|' ..
                    string.format('body:%d:%s:%s', 1, 'n1', 'deadbeef')",
        )
        .unwrap();
        assert_eq!(
            built,
            format!(
                "{}|{}|{}",
                here_challenge("n1", 51000),
                open_challenge("n1"),
                body_challenge("n1", "deadbeef")
            )
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
                // The device learns an endpoint between the two, which is the
                // ordinary case the moment stage 2 lands — and it is the file
                // that used to make `uninstall` refuse, so the property has to
                // be asserted with one present rather than beside it.
                std::fs::write(
                    dir_of(tmp.path()).join(ENDPOINT_FILE),
                    "return { [\"dev-1\"] = { host = \"10.0.0.2\", port = 1 } }\n",
                ).unwrap();
                uninstall(tmp.path()).unwrap();
                prop_assert_eq!(snapshot(tmp.path()), before);
            }
        }
    }
}
