//! Persisted settings the TUI reads and writes.
//!
//! Two files under `$XDG_CONFIG_HOME/readingbuddy/` (default `~/.config/...`):
//!
//! - `tui.toml` — TUI-only, non-secret (currently just the accent). Kept apart
//!   from the CLI file because the two crates use different structs and a
//!   full-overwrite `save` from either would drop the other's fields.
//! - `config.toml` — the **shared secret** file (mode 600) that also backs the
//!   CLI's `config set google-api-key`. Read/written here so a key entered in
//!   the TUI is visible to the CLI and vice versa.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    /// Accent color as `#RRGGBB` (None = the built-in default).
    pub accent: Option<String>,
    /// Ambient background motif by label (None / unknown = off).
    pub ambient: Option<String>,
    /// What the library list is ordered by, by label (None / unknown =
    /// most-recently-touched).
    pub library_sort: Option<String>,
}

/// Mirror of the CLI's `CliConfig` — only the fields we touch. Deserialize
/// ignores the rest, so reading is forgiving; we currently only own this key.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct SecretConfig {
    google_api_key: Option<String>,
}

fn config_dir() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => std::env::home_dir()
            .context("cannot locate home directory")?
            .join(".config"),
    };
    Ok(base.join("readingbuddy"))
}

fn tui_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("tui.toml"))
}

fn secret_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn load() -> Result<TuiConfig> {
    let path = tui_path()?;
    if !path.exists() {
        return Ok(TuiConfig::default());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

pub fn save(config: &TuiConfig) -> Result<PathBuf> {
    let path = tui_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(config)?;
    std::fs::write(&path, raw)?;
    Ok(path)
}

/// The persisted Google Books API key, if the shared config file holds one.
/// A missing/unreadable file is simply "no key", never an error.
pub fn load_google_key() -> Option<String> {
    let raw = std::fs::read_to_string(secret_path().ok()?).ok()?;
    toml::from_str::<SecretConfig>(&raw).ok()?.google_api_key
}

/// Write the Google Books API key to the shared config file (mode 600),
/// preserving the file's other keys.
pub fn save_google_key(key: &str) -> Result<PathBuf> {
    let path = secret_path()?;
    let mut cfg: SecretConfig = if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&raw).unwrap_or_default()
    } else {
        SecretConfig::default()
    };
    cfg.google_api_key = Some(key.to_string());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, toml::to_string_pretty(&cfg)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}
