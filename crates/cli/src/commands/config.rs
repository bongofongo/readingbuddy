use anyhow::{Result, bail};
use clap::{Subcommand, ValueEnum};

use crate::config_file::{self, mask};

#[derive(Clone, Copy, ValueEnum)]
pub enum ConfigKey {
    /// Google Books API key (create one at console.cloud.google.com, enable "Books API")
    GoogleApiKey,
}

#[derive(Subcommand)]
pub enum ConfigCmd {
    /// Store a value. Omit VALUE to enter it at a hidden prompt
    /// (keeps the key out of your shell history).
    Set {
        key: ConfigKey,
        value: Option<String>,
        /// Check the key against the live Google Books API before saving
        #[arg(long)]
        verify: bool,
    },
    /// Show the effective configuration (secrets masked) and where each
    /// value comes from
    Get,
    /// Remove a stored value
    Unset { key: ConfigKey },
    /// Print the config file location
    Path,
}

/// `cli_key` is the already-resolved --google-api-key flag / env value, used
/// by `get` to report the effective setting.
pub async fn run(cmd: ConfigCmd, cli_key: Option<&str>) -> Result<()> {
    match cmd {
        ConfigCmd::Set { key: ConfigKey::GoogleApiKey, value, verify } => set_google_key(value, verify).await,
        ConfigCmd::Get => get(cli_key),
        ConfigCmd::Unset { key: ConfigKey::GoogleApiKey } => unset_google_key(),
        ConfigCmd::Path => {
            println!("{}", config_file::config_path()?.display());
            Ok(())
        }
    }
}

async fn set_google_key(value: Option<String>, verify: bool) -> Result<()> {
    let value = match value {
        Some(v) => v,
        None => rpassword::prompt_password("Google Books API key (input hidden): ")?,
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        bail!("empty key, nothing saved");
    }

    if verify {
        println!("verifying against Google Books...");
        match readingbuddy::verify_google_key(&value).await {
            Ok(()) => println!("ok ✓"),
            Err(reason) => bail!("key rejected: {reason}\nnothing saved (drop --verify to save anyway)"),
        }
    }

    let mut cfg = config_file::load()?;
    cfg.google_api_key = Some(value);
    let path = config_file::save(&cfg)?;
    println!("saved {} to {} (file mode 600)", mask(cfg.google_api_key.as_deref().unwrap()), path.display());
    if !verify {
        println!("tip: `readingbuddy config set google-api-key --verify` checks the key live");
    }
    Ok(())
}

fn get(cli_key: Option<&str>) -> Result<()> {
    let file = config_file::load()?;
    let path = config_file::config_path()?;

    // Precedence: --flag / GOOGLE_BOOKS_API_KEY env (clap merges those) > config file.
    let (effective, source) = match (cli_key, file.google_api_key.as_deref()) {
        (Some(k), _) => (Some(k), "command line / environment"),
        (None, Some(k)) => (Some(k), "config file"),
        (None, None) => (None, ""),
    };
    match effective {
        Some(k) => println!("google-api-key: {}  (from {source})", mask(k)),
        None => println!(
            "google-api-key: not set — Google Books runs keyless (low quota).\n\
             set one with: readingbuddy config set google-api-key"
        ),
    }
    println!("config file:    {}{}", path.display(), if path.exists() { "" } else { " (does not exist yet)" });
    Ok(())
}

fn unset_google_key() -> Result<()> {
    let mut cfg = config_file::load()?;
    if cfg.google_api_key.take().is_none() {
        println!("google-api-key was not set in the config file.");
        return Ok(());
    }
    let path = config_file::save(&cfg)?;
    println!("removed google-api-key from {}", path.display());
    Ok(())
}
