pub mod googlebooks;
pub mod openlibrary;

use async_trait::async_trait;

use crate::book::Book;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProviderId {
    OpenLibrary,
    GoogleBooks,
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderId::OpenLibrary => write!(f, "openlibrary"),
            ProviderId::GoogleBooks => write!(f, "googlebooks"),
        }
    }
}

impl From<ProviderId> for crate::storage::Source {
    /// A provider, as `field_provenance` names it.
    ///
    /// An exhaustive `match` and not a string built from [`Display`], though the
    /// two spell the same words today: `Source`'s vocabulary is wider than this
    /// enum (calibre, epub, koreader, goodreads, the user) and going through
    /// text would let a fourth provider land as a source nothing in
    /// `field_provenance` recognises, silently. Here it is a compile error.
    fn from(id: ProviderId) -> Self {
        match id {
            ProviderId::OpenLibrary => crate::storage::Source::OpenLibrary,
            ProviderId::GoogleBooks => crate::storage::Source::GoogleBooks,
        }
    }
}

/// Fielded search request; any combination of fields may be set.
#[derive(Debug, Clone, Default)]
pub struct SearchRequest {
    /// Free-text query.
    pub query: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub publisher: Option<String>,
    /// Neither API has a first-class translator field; providers map this to
    /// their closest contributor/author facility and ranking rewards matches.
    pub translator: Option<String>,
    pub language: Option<String>,
    pub year: Option<i64>,
    pub isbn: Option<String>,
    pub limit: u32,
}

impl SearchRequest {
    pub fn is_empty(&self) -> bool {
        self.query.is_none()
            && self.title.is_none()
            && self.author.is_none()
            && self.publisher.is_none()
            && self.translator.is_none()
            && self.isbn.is_none()
    }
}

/// One raw result from one provider, position preserved for ranking.
#[derive(Debug, Clone)]
pub struct ProviderBook {
    pub book: Book,
    pub provider: ProviderId,
    pub position: usize,
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn search(&self, req: &SearchRequest) -> Result<Vec<ProviderBook>>;
    async fn by_isbn(&self, isbn: &str) -> Result<Option<ProviderBook>>;
}

/// Tidy a provider's subject list: trim, drop the empties, drop repeats.
///
/// **Shape only, never vocabulary.** The strings stay exactly as the provider
/// spelled them — Google's BISAC paths (`Fiction / Literary`) and OpenLibrary's
/// headings (`Korean Americans`) are two different systems of naming and
/// mapping one onto the other is a *controlled vocabulary*, which is the same
/// question `book_tags` deferred and is not answerable from two examples. The
/// merge sidesteps it by taking one provider's set whole rather than mixing
/// them; see `MERGE_RULES`.
///
/// Dedup is case-insensitive and keeps the **first** spelling, because a
/// provider that says both `Fiction` and `fiction` said one thing twice and the
/// order it published is the only ranking available.
///
/// **No cap.** OpenLibrary works genuinely carry a hundred subjects, many of
/// them cataloguing artifacts (`Accessible book`, `Protected DAISY`,
/// `In library`), and dropping the tail would be a silent truncation of exactly
/// the sort this repo bans elsewhere — a caller cannot tell a trimmed list from
/// a short one. Filtering the artifacts *is* a vocabulary decision, and it is
/// left undone on purpose so it can be made against real data rather than
/// against a guess.
pub fn normalize_subjects<I, S>(raw: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for s in raw {
        let t = s.as_ref().split_whitespace().collect::<Vec<_>>().join(" ");
        if t.is_empty() {
            continue;
        }
        if !out.iter().any(|kept| kept.eq_ignore_ascii_case(&t)) {
            out.push(t);
        }
    }
    out
}

/// Normalize language tags across providers: OpenLibrary speaks MARC
/// 3-letter codes ("eng"), Google Books BCP-47 2-letter ("en"). We store
/// 2-letter where known, pass through otherwise.
pub fn normalize_language(raw: &str) -> String {
    let l = raw.trim().to_lowercase();
    match l.as_str() {
        "eng" => "en",
        "fre" | "fra" => "fr",
        "ger" | "deu" => "de",
        "spa" => "es",
        "ita" => "it",
        "por" => "pt",
        "rus" => "ru",
        "jpn" => "ja",
        "chi" | "zho" => "zh",
        "kor" => "ko",
        "ara" => "ar",
        "dut" | "nld" => "nl",
        "swe" => "sv",
        "nor" => "no",
        "dan" => "da",
        "fin" => "fi",
        "pol" => "pl",
        "tur" => "tr",
        "heb" => "he",
        "lat" => "la",
        "grc" | "gre" | "ell" => "el",
        other => other,
    }
    .to_string()
}

/// Inverse mapping for building OpenLibrary queries from 2-letter input.
pub fn to_marc_language(raw: &str) -> String {
    let l = raw.trim().to_lowercase();
    match l.as_str() {
        "en" => "eng",
        "fr" => "fre",
        "de" => "ger",
        "es" => "spa",
        "it" => "ita",
        "pt" => "por",
        "ru" => "rus",
        "ja" => "jpn",
        "zh" => "chi",
        "ko" => "kor",
        "ar" => "ara",
        "nl" => "dut",
        "sv" => "swe",
        "no" => "nor",
        "da" => "dan",
        "fi" => "fin",
        "pl" => "pol",
        "tr" => "tur",
        "he" => "heb",
        "la" => "lat",
        "el" => "gre",
        other => other,
    }
    .to_string()
}

/// Extract the first 4-digit year from a free-form date string
/// ("Aug 25, 2017", "2017-08-25", "2017").
pub fn year_of_date(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i - start == 4 {
                return s[start..i].parse().ok();
            }
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_extraction() {
        assert_eq!(year_of_date("Aug 25, 2017"), Some(2017));
        assert_eq!(year_of_date("2017-08-25"), Some(2017));
        assert_eq!(year_of_date("2017"), Some(2017));
        assert_eq!(year_of_date("25th"), None);
        assert_eq!(year_of_date(""), None);
    }

    #[test]
    fn language_roundtrip() {
        assert_eq!(normalize_language("eng"), "en");
        assert_eq!(normalize_language("EN"), "en");
        assert_eq!(to_marc_language("en"), "eng");
        assert_eq!(normalize_language("xyz"), "xyz");
    }
}
