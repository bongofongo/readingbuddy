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

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    fn row(word: &str, context: Option<&str>, title: &str) -> FlashcardRow {
        FlashcardRow {
            id: 1,
            word: word.to_string(),
            context: context.map(str::to_string),
            book_title: title.to_string(),
            exported: false,
        }
    }

    proptest! {
        /// Anki splits on tabs and newlines, so a card whose text contains
        /// either would silently become a malformed row — or two. `escape_tsv`
        /// handles this today; this pins it, because the failure mode is a
        /// corrupted import that looks fine until you open the deck.
        #[test]
        fn every_exported_row_has_exactly_three_fields(
            cards in proptest::collection::vec(
                (".{0,30}", proptest::option::of(".{0,30}"), ".{0,30}"),
                0..6,
            ),
        ) {
            let rows: Vec<FlashcardRow> = cards
                .iter()
                .map(|(w, c, t)| row(w, c.as_deref(), t))
                .collect();
            let tsv = export_tsv(&rows);

            let lines: Vec<&str> = tsv.lines().collect();
            // Two directive lines, then one line per card — no more, which is
            // what proves nothing smuggled in a newline.
            prop_assert_eq!(lines.len(), 2 + rows.len(), "row count changed: {:?}", tsv);
            prop_assert!(lines[0].starts_with('#') && lines[1].starts_with('#'));

            for line in lines.iter().skip(2) {
                prop_assert_eq!(
                    line.split('\t').count(),
                    3,
                    "not a 3-field row: {:?}", line
                );
            }
        }

        /// What `single_word` promises its one caller (the KOReader import,
        /// which turns the result straight into a flashcard).
        #[test]
        fn a_single_word_is_really_a_single_clean_word(s in ".{0,40}") {
            if let Some(w) = single_word(&s) {
                prop_assert!(!w.is_empty());
                prop_assert!(
                    !w.chars().any(char::is_whitespace),
                    "whitespace in {:?} from {:?}", w, s
                );
                // Edge punctuation is stripped from both ends.
                prop_assert!(w.chars().next().unwrap().is_alphanumeric(), "{:?}", w);
                prop_assert!(w.chars().last().unwrap().is_alphanumeric(), "{:?}", w);
                prop_assert!(s.contains(&w), "{:?} is not a substring of {:?}", w, s);
            }
        }

        /// Anything with a space in it is a passage, not a vocabulary word.
        #[test]
        fn multi_word_text_is_never_a_flashcard(a in "[a-z]{1,8}", b in "[a-z]{1,8}") {
            prop_assert_eq!(single_word(&format!("{a} {b}")), None);
        }

        #[test]
        fn single_word_never_panics(s in ".{0,100}") {
            let _ = single_word(&s);
        }
    }
}
