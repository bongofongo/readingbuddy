mod book_files;
mod books;
mod device_books;
mod field_provenance;
mod flashcards;
mod highlights;
mod notes;
mod provenance;
mod ratings;
mod reading_events;
mod readings;
mod sidecar_seen;

pub use book_files::BookFile;
pub use books::{BookSort, MergeReport};
/// The merge table's three crate-internal readers: which columns it governs,
/// what a record says about one of them, and which it speaks to at all. Item
/// 30's enrichment assembles per-field attribution and prints held-back values,
/// and both have to name columns *this* table knows rather than strings that
/// look like column names.
pub(crate) use books::{
    field_pair, field_value, fields_said, merge_columns, merge_provider_record,
};
pub use device_books::LinkedBy;
pub use field_provenance::{FieldSource, Source};
pub use flashcards::FlashcardRow;
pub(crate) use highlights::DeviceDigest;
pub use highlights::{Highlight, NewHighlight};
pub use notes::{NewNoteMeta, NoteRecord, NoteSearchHit, OutgoingLink};
pub use provenance::BookTag;
pub use ratings::{Rating, RatingScale};
pub use reading_events::{
    ActivitySummary, Confidence, DayActivity, DayRange, FillStats, NewReadingEvent, ReadingEvent,
    RefillReport, SOURCE_KOREADER, SOURCE_VAULT, day_of_ko_datetime, day_of_unix,
};
pub use readings::{
    Reading, STATUS_ABANDONED, STATUS_FINISHED, STATUS_READING, ko_datetime_to_unix,
};
pub use sidecar_seen::SidecarFacts;

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::error::Result;

/// Concrete storage boundary wrapping SQLite. This struct IS the swap point:
/// a future backend replaces its internals, or grows into a trait when a
/// second backend actually exists.
#[derive(Debug, Clone)]
pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    /// Connect (creating the file if missing), enable foreign keys, run
    /// embedded migrations. Accepts any sqlx sqlite URL incl. "sqlite::memory:".
    pub async fn connect(db_url: &str) -> Result<Storage> {
        let opts = SqliteConnectOptions::from_str(db_url)?
            .create_if_missing(true)
            .foreign_keys(true);
        // In-memory databases are per-connection: a >1 pool would migrate one
        // connection and query a fresh empty one.
        let max = if db_url.contains(":memory:") { 1 } else { 5 };
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        tracing::info!(max_connections = max, "storage connected and migrated");
        Ok(Storage { pool })
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

pub(crate) fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Doubles as the FTS5-availability canary: migration includes a
    // CREATE VIRTUAL TABLE ... fts5 and fails here if the bundled
    // SQLite lacks it.
    #[tokio::test]
    async fn migrates_in_memory() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let n: (i64,) = sqlx::query_as("SELECT count(*) FROM books")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert_eq!(n.0, 0);
    }
}
