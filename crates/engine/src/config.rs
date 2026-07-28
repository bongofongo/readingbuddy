use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// sqlx sqlite URL, e.g. "sqlite://database/app.db"
    pub db_url: String,
    /// Directory for downloaded / extracted cover images.
    pub images_dir: PathBuf,
    /// Obsidian-openable vault directory for note markdown files.
    pub vault_dir: PathBuf,
    /// Content-addressed store for the ebook files readingbuddy owns
    /// (`<files_dir>/<ab>/<sha256>.<ext>`, migration `0010`).
    ///
    /// Beside `images_dir` under `database/` rather than next to the vault: the
    /// vault is the user's to open, edit and sync, and a directory of files
    /// named after their own hashes is ours to manage and meaningless to read.
    pub files_dir: PathBuf,
    /// Where a frontend should put its log and crash files.
    ///
    /// Under the data root rather than `$XDG_STATE_HOME` on purpose: the data
    /// root is already this app's single runtime-state anchor, it is already
    /// gitignored, and it moves with `--data-dir` — so a sandbox run writes its
    /// logs into the sandbox instead of the user's home.
    pub log_dir: PathBuf,
    /// Optional Google Books API key (keyless works at lower quota).
    pub google_api_key: Option<String>,
}

impl EngineConfig {
    /// Standard layout rooted at `data_root`: database/app.db, database/images/, vault/.
    pub fn rooted_at(data_root: impl Into<PathBuf>) -> Self {
        let root: PathBuf = data_root.into();
        EngineConfig {
            db_url: format!("sqlite://{}", root.join("database/app.db").display()),
            images_dir: root.join("database/images"),
            files_dir: root.join("database/files"),
            vault_dir: root.join("vault"),
            log_dir: root.join("logs"),
            google_api_key: std::env::var("GOOGLE_BOOKS_API_KEY").ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooted_at_puts_everything_under_the_given_root() {
        let c = EngineConfig::rooted_at("/tmp/rb");
        assert_eq!(c.db_url, "sqlite:///tmp/rb/database/app.db");
        assert_eq!(c.images_dir, PathBuf::from("/tmp/rb/database/images"));
        assert_eq!(c.files_dir, PathBuf::from("/tmp/rb/database/files"));
        assert_eq!(c.vault_dir, PathBuf::from("/tmp/rb/vault"));
        // Logs must follow --data-dir, or a sandbox run scribbles in $HOME.
        assert_eq!(c.log_dir, PathBuf::from("/tmp/rb/logs"));
    }

    #[test]
    fn a_relative_root_stays_relative() {
        let c = EngineConfig::rooted_at(".");
        assert_eq!(c.db_url, "sqlite://./database/app.db");
        assert_eq!(c.log_dir, PathBuf::from("./logs"));
    }
}
