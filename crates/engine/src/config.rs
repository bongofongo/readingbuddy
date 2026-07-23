use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// sqlx sqlite URL, e.g. "sqlite://database/app.db"
    pub db_url: String,
    /// Directory for downloaded / extracted cover images.
    pub images_dir: PathBuf,
    /// Obsidian-openable vault directory for note markdown files.
    pub vault_dir: PathBuf,
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
            vault_dir: root.join("vault"),
            google_api_key: std::env::var("GOOGLE_BOOKS_API_KEY").ok(),
        }
    }
}
