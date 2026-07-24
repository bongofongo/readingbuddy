//! TUI-local persisted settings.
//!
//! Kept in its **own** file — `$XDG_CONFIG_HOME/readingbuddy/tui.toml` — rather
//! than sharing the CLI's `config.toml`: the two crates use different structs,
//! and a full-overwrite `save` from either would drop the other's fields (the
//! CLI file holds the Google key). No secrets here, so no mode-600.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    /// Accent color as `#RRGGBB` (None = the built-in default).
    pub accent: Option<String>,
}

fn config_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => std::env::home_dir()
            .context("cannot locate home directory")?
            .join(".config"),
    };
    Ok(base.join("readingbuddy").join("tui.toml"))
}

pub fn load() -> Result<TuiConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(TuiConfig::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

pub fn save(config: &TuiConfig) -> Result<PathBuf> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(config)?;
    std::fs::write(&path, raw)?;
    Ok(path)
}
