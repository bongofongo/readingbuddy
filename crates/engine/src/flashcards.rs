use crate::storage::FlashcardRow;

/// If a highlight is a single word (after trimming edge punctuation and
/// quotes), return the cleaned word — it's a flashcard candidate.
pub fn single_word(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.split_whitespace().count() != 1 {
        return None;
    }
    let cleaned = trimmed.trim_matches(|c: char| !c.is_alphanumeric());
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.to_string())
}

fn escape_tsv(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// Anki-importable TSV: word, context, book title. Header directives tell
/// Anki's importer the separator and to treat fields as plain text.
pub fn export_tsv(cards: &[FlashcardRow]) -> String {
    let mut out = String::from("#separator:tab\n#html:false\n");
    for c in cards {
        out.push_str(&format!(
            "{}\t{}\t{}\n",
            escape_tsv(&c.word),
            escape_tsv(c.context.as_deref().unwrap_or("")),
            escape_tsv(&c.book_title),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_word_detection() {
        assert_eq!(single_word("pachinko"), Some("pachinko".to_string()));
        assert_eq!(single_word("  «mot»  "), Some("mot".to_string()));
        assert_eq!(single_word("\"verdict,\""), Some("verdict".to_string()));
        assert_eq!(single_word("well-known"), Some("well-known".to_string()));
        assert_eq!(single_word("two words"), None);
        assert_eq!(single_word(""), None);
        assert_eq!(single_word("..."), None);
    }

    #[test]
    fn tsv_escapes_separators() {
        let cards = vec![FlashcardRow {
            id: 1,
            word: "mot".into(),
            context: Some("line one\nline\ttwo".into()),
            book_title: "A Book".into(),
            exported: false,
        }];
        let tsv = export_tsv(&cards);
        let lines: Vec<&str> = tsv.lines().collect();
        assert_eq!(lines[0], "#separator:tab");
        assert_eq!(lines[1], "#html:false");
        assert_eq!(lines[2], "mot\tline one line two\tA Book");
    }
}
