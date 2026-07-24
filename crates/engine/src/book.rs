use time::OffsetDateTime;

/// Central domain struct. Identity: internal `id` is canonical; ISBNs are
/// unique lookup keys (a book may carry either, both, or neither).
#[derive(Debug, Clone, Default)]
pub struct Book {
    pub id: Option<i64>,
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub authors: Vec<String>,
    pub translators: Vec<String>,
    pub publisher: Option<String>,
    pub publish_year: Option<i64>,
    pub language: Option<String>,
    pub isbn_10: Option<String>,
    pub isbn_13: Option<String>,
    pub openlibrary_key: Option<String>,
    pub googlebooks_id: Option<String>,
    pub cover_url: Option<String>,
    pub cover_path: Option<String>,
    pub page_count: Option<i64>,
    pub description: Option<String>,
    pub first_sentence: Option<String>,
    pub current_page: Option<i64>,
    pub finished: bool,
    pub date_started: Option<i64>,
    pub date_finished: Option<i64>,
    pub created_at: Option<OffsetDateTime>,
    pub last_modified: Option<OffsetDateTime>,
}

impl Book {
    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or("(untitled)")
    }

    pub fn display_authors(&self) -> String {
        if self.authors.is_empty() {
            "(unknown)".to_string()
        } else {
            self.authors.join(", ")
        }
    }

    /// Any ISBN, preferring 13.
    pub fn any_isbn(&self) -> Option<&str> {
        self.isbn_13.as_deref().or(self.isbn_10.as_deref())
    }

    /// Canonical dedup key: ISBN-13, converting a lone ISBN-10 when possible.
    pub fn canonical_isbn13(&self) -> Option<String> {
        if let Some(i13) = &self.isbn_13 {
            return Some(i13.clone());
        }
        self.isbn_10.as_deref().and_then(isbn10_to_13)
    }
}

/// Normalize a raw ISBN-ish string (hyphens/spaces stripped, `urn:isbn:`
/// prefixes dropped, X uppercased) and validate its checksum.
/// Returns None if it isn't a valid ISBN-10 or ISBN-13.
pub fn normalize_isbn(raw: &str) -> Option<String> {
    let stripped = raw.trim().rsplit(':').next().unwrap_or(raw);
    let cleaned: String = stripped
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    match cleaned.len() {
        10 if is_valid_isbn10(&cleaned) => Some(cleaned),
        13 if is_valid_isbn13(&cleaned) => Some(cleaned),
        _ => None,
    }
}

fn is_valid_isbn10(s: &str) -> bool {
    let mut sum: u32 = 0;
    for (i, c) in s.chars().enumerate() {
        let v = match c {
            '0'..='9' => c as u32 - '0' as u32,
            'X' if i == 9 => 10,
            _ => return false,
        };
        sum += (10 - i as u32) * v;
    }
    sum.is_multiple_of(11)
}

fn is_valid_isbn13(s: &str) -> bool {
    let mut sum: u32 = 0;
    for (i, c) in s.chars().enumerate() {
        let Some(v) = c.to_digit(10) else {
            return false;
        };
        sum += if i % 2 == 0 { v } else { 3 * v };
    }
    sum.is_multiple_of(10)
}

/// Convert a valid ISBN-10 to its ISBN-13 (978-prefix) form.
pub fn isbn10_to_13(isbn10: &str) -> Option<String> {
    if isbn10.len() != 10 || !is_valid_isbn10(isbn10) {
        return None;
    }
    let body = format!("978{}", &isbn10[..9]);
    let mut sum: u32 = 0;
    for (i, c) in body.chars().enumerate() {
        let v = c.to_digit(10)?;
        sum += if i % 2 == 0 { v } else { 3 * v };
    }
    let check = (10 - sum % 10) % 10;
    Some(format!("{body}{check}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hyphens_and_prefixes() {
        assert_eq!(
            normalize_isbn("urn:isbn:978-1-4555-6393-7"),
            Some("9781455563937".to_string())
        );
        assert_eq!(
            normalize_isbn(" 0-306-40615-2 "),
            Some("0306406152".to_string())
        );
    }

    #[test]
    fn keeps_leading_zero_and_x_check_digit() {
        // Both impossible under the old i64 representation.
        assert_eq!(
            normalize_isbn("0-19-853453-1"),
            Some("0198534531".to_string())
        );
        assert_eq!(normalize_isbn("080442957x"), Some("080442957X".to_string()));
    }

    #[test]
    fn rejects_bad_checksums_and_shapes() {
        assert_eq!(normalize_isbn("9781455563938"), None); // bad check digit
        assert_eq!(normalize_isbn("0306406153"), None); // bad check digit
        assert_eq!(normalize_isbn("12345"), None);
        assert_eq!(normalize_isbn("not-an-isbn"), None);
        assert_eq!(normalize_isbn("123456789X123"), None); // X only legal in isbn10
    }

    #[test]
    fn converts_10_to_13() {
        assert_eq!(
            isbn10_to_13("0306406152"),
            Some("9780306406157".to_string())
        );
        assert_eq!(
            isbn10_to_13("155404295X"),
            Some("9781554042951".to_string())
        );
    }
}
