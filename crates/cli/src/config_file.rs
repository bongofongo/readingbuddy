use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// User-level CLI configuration, stored as TOML at
/// `$XDG_CONFIG_HOME/readingbuddy/config.toml` (default `~/.config/...`).
/// Written with mode 600 — it holds secrets.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CliConfig {
    pub google_api_key: Option<String>,
}

pub fn config_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => std::env::home_dir()
            .context("cannot locate home directory")?
            .join(".config"),
    };
    Ok(base.join("readingbuddy").join("config.toml"))
}

pub fn load() -> Result<CliConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(CliConfig::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

pub fn save(config: &CliConfig) -> Result<PathBuf> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(config)?;
    std::fs::write(&path, raw)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

/// Show a secret without revealing it: `AIza…f3Qk` (first/last 4 chars).
pub fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 8 {
        "*".repeat(chars.len().max(4))
    } else {
        format!(
            "{}…{}",
            chars[..4].iter().collect::<String>(),
            chars[chars.len() - 4..].iter().collect::<String>()
        )
    }
}
