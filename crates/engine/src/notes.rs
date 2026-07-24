use std::path::{Path, PathBuf};

use regex::Regex;
use time::OffsetDateTime;
use time::macros::format_description;

use crate::book::Book;
use crate::error::{EngineError, Result};
use crate::storage::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteKind {
    #[default]
    Note,
    /// Small reading-session thought.
    Session,
    /// Final thoughts after finishing a book.
    Final,
}

impl NoteKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NoteKind::Note => "note",
            NoteKind::Session => "session",
            NoteKind::Final => "final",
        }
    }
}

impl std::str::FromStr for NoteKind {
    type Err = EngineError;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "note" => Ok(NoteKind::Note),
            "session" => Ok(NoteKind::Session),
            "final" => Ok(NoteKind::Final),
            other => Err(EngineError::Other(format!("unknown note kind: {other}"))),
        }
    }
}

#[derive(Debug, Default)]
pub struct NewNoteInput {
    pub book_id: Option<i64>,
    pub highlight_id: Option<i64>,
    /// Device/pdf page the note anchors to (auto-suggested from the book's
    /// current progress by the frontend, editable per note).
    pub page: Option<i64>,
    /// Free-form location: chapter name, "loc 1234", a percentage, etc.
    pub location: Option<String>,
    pub kind: NoteKind,
    pub title: Option<String>,
    pub body: String,
}

#[derive(Debug)]
pub struct CreatedNote {
    pub id: i64,
    pub title: String,
    /// Absolute path of the markdown file on disk.
    pub file: PathBuf,
    pub links: Vec<String>,
}

/// Obsidian-safe kebab slug: strips `: / \ # ^ [ ] |` and friends.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 60 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "untitled".to_string() } else { out }
}

/// Extract [[wikilink]] targets, handling `[[target|alias]]` and
/// `[[target#heading]]`; deduped, order preserved.
pub fn extract_wikilinks(body: &str) -> Vec<String> {
    let re = Regex::new(r"\[\[([^\]\|#]+)(?:#[^\]\|]*)?(?:\|[^\]]*)?\]\]").expect("static regex");
    let mut seen = Vec::new();
    for cap in re.captures_iter(body) {
        let target = cap[1].trim().to_string();
        if !target.is_empty() && !seen.contains(&target) {
            seen.push(target);
        }
    }
    seen
}

fn frontmatter(
    book: Option<&Book>,
    highlight_id: Option<i64>,
    page: Option<i64>,
    location: Option<&str>,
    kind: NoteKind,
    created: &str,
) -> String {
    let mut fm = String::from("---\n");
    if let Some(b) = book {
        let id = b
            .any_isbn()
            .map(str::to_string)
            .or(b.id.map(|i| i.to_string()))
            .unwrap_or_default();
        fm.push_str(&format!("book: {id}\n"));
        fm.push_str(&format!("book-title: \"{}\"\n", b.display_title().replace('"', "'")));
    }
    if let Some(h) = highlight_id {
        fm.push_str(&format!("highlight: {h}\n"));
    }
    if let Some(p) = page {
        fm.push_str(&format!("page: {p}\n"));
    }
    if let Some(l) = location {
        fm.push_str(&format!("location: \"{}\"\n", l.replace('"', "'")));
    }
    fm.push_str(&format!("kind: {}\n", kind.as_str()));
    fm.push_str(&format!("created: {created}\n"));
    fm.push_str("---\n\n");
    fm
}

/// Split a note file into its raw frontmatter header (through the closing
/// `---` and any blank line after it) and the body. When there is no
/// frontmatter the header is empty and the body is the whole content.
pub fn frontmatter_and_body(content: &str) -> (&str, &str) {
    let Some(rest) = content.strip_prefix("---\n") else {
        return ("", content);
    };
    let Some(end) = rest.find("\n---") else {
        return ("", content);
    };
    // Byte offset just past the closing `\n---` line's newlines.
    let after_close = "---\n".len() + end + "\n---".len();
    let body_start = after_close + content[after_close..].len()
        - content[after_close..].trim_start_matches('\n').len();
    (&content[..body_start], &content[body_start..])
}

/// Split a note file into (frontmatter key/value pairs, body).
pub fn parse_frontmatter(content: &str) -> (Vec<(String, String)>, &str) {
    let (header, body) = frontmatter_and_body(content);
    if header.is_empty() {
        return (Vec::new(), body);
    }
    let pairs = header
        .lines()
        .filter(|l| *l != "---")
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            Some((k.trim().to_string(), v.trim().trim_matches('"').to_string()))
        })
        .collect();
    (pairs, body)
}

fn derive_title(body: &str) -> String {
    let words: Vec<&str> = body.split_whitespace().take(6).collect();
    if words.is_empty() {
        "Untitled".to_string()
    } else {
        words.join(" ").trim_end_matches([',', '.', ';', ':']).to_string()
    }
}

/// Write the markdown file into the vault and index it (metadata, FTS,
/// wikilink edges) in the database.
pub async fn create_note(
    storage: &Storage,
    vault_dir: &Path,
    book: Option<&Book>,
    input: NewNoteInput,
) -> Result<CreatedNote> {
    let title = input
        .title
        .clone()
        .unwrap_or_else(|| derive_title(&input.body));

    let book_dir = book
        .map(|b| slugify(b.display_title()))
        .unwrap_or_else(|| "unsorted".to_string());
    let dir = vault_dir.join(&book_dir);
    std::fs::create_dir_all(&dir)?;

    let now = OffsetDateTime::now_utc();
    let stamp = now
        .format(format_description!(
            "[year][month][day][hour][minute][second]"
        ))
        .map_err(|e| EngineError::Other(format!("timestamp format: {e}")))?;
    let base = format!("{stamp}-{}", slugify(&title));
    let mut file_name = format!("{base}.md");
    let mut n = 1;
    while dir.join(&file_name).exists() {
        n += 1;
        file_name = format!("{base}-{n}.md");
    }
    let file = dir.join(&file_name);
    let rel_path = format!("{book_dir}/{file_name}");

    let created_str = now
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| EngineError::Other(format!("timestamp format: {e}")))?;
    let content = format!(
        "{}{}\n",
        frontmatter(
            book,
            input.highlight_id,
            input.page,
            input.location.as_deref(),
            input.kind,
            &created_str,
        ),
        input.body.trim_end()
    );
    std::fs::write(&file, &content)?;

    let links = extract_wikilinks(&input.body);
    let id = storage
        .insert_note(
            crate::storage::NewNoteMeta {
                book_id: input.book_id,
                highlight_id: input.highlight_id,
                page: input.page,
                location: input.location.as_deref(),
                file_path: &rel_path,
                title: &title,
                kind: input.kind.as_str(),
            },
            &input.body,
            &links,
        )
        .await?;

    Ok(CreatedNote {
        id,
        title,
        file,
        links,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_obsidian_safe() {
        assert_eq!(slugify("Sunja's Choice: part 1"), "sunja-s-choice-part-1");
        assert_eq!(slugify("a/b\\c#d^e[f]g|h"), "a-b-c-d-e-f-g-h");
        assert_eq!(slugify("  ---  "), "untitled");
        assert!(slugify(&"x".repeat(200)).len() <= 60);
    }

    #[test]
    fn wikilink_extraction_handles_aliases_and_headings() {
        let body = "See [[Han]] and [[Zettelkasten|the method]], plus [[Pachinko#Chapter 2]].\
                    Repeat [[Han]]. Not a link: [single].";
        assert_eq!(
            extract_wikilinks(body),
            vec!["Han".to_string(), "Zettelkasten".to_string(), "Pachinko".to_string()]
        );
    }

    #[test]
    fn frontmatter_roundtrip() {
        let book = Book {
            id: Some(7),
            title: Some("Pachinko".into()),
            isbn_13: Some("9781455563937".into()),
            ..Default::default()
        };
        let fm = frontmatter(
            Some(&book),
            Some(3),
            Some(42),
            Some("Chapter 2"),
            NoteKind::Session,
            "2026-07-23T10:00:00Z",
        );
        let content = format!("{fm}The body text with [[Link]].\n");
        let (pairs, body) = parse_frontmatter(&content);
        let get = |k: &str| pairs.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
        assert_eq!(get("book"), Some("9781455563937"));
        assert_eq!(get("book-title"), Some("Pachinko"));
        assert_eq!(get("highlight"), Some("3"));
        assert_eq!(get("page"), Some("42"));
        assert_eq!(get("location"), Some("Chapter 2"));
        assert_eq!(get("kind"), Some("session"));
        assert_eq!(get("created"), Some("2026-07-23T10:00:00Z"));
        assert_eq!(body, "The body text with [[Link]].\n");
    }

    #[test]
    fn frontmatter_and_body_splits_and_preserves_header() {
        let content = "---\npage: 5\nkind: note\n---\n\nThe body\nspans lines.\n";
        let (header, body) = frontmatter_and_body(content);
        assert_eq!(header, "---\npage: 5\nkind: note\n---\n\n");
        assert_eq!(body, "The body\nspans lines.\n");
        // Reassembling with a new body keeps the header verbatim.
        assert_eq!(format!("{header}rewritten\n"), "---\npage: 5\nkind: note\n---\n\nrewritten\n");

        // No frontmatter: empty header, whole content is body.
        let (h2, b2) = frontmatter_and_body("just text\n");
        assert_eq!(h2, "");
        assert_eq!(b2, "just text\n");
    }

    #[test]
    fn derive_title_takes_leading_words() {
        assert_eq!(derive_title("Sunja's choice mirrors the whole diaspora, somehow."),
                   "Sunja's choice mirrors the whole diaspora");
        assert_eq!(derive_title(""), "Untitled");
    }

    #[tokio::test]
    async fn create_note_writes_file_and_indexes() {
        let vault = std::env::temp_dir().join(format!("rb-vault-test-{}", std::process::id()));
        std::fs::remove_dir_all(&vault).ok();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book_id = s
            .upsert_book(&Book {
                title: Some("Pachinko".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        let book = s.get_book(book_id).await.unwrap().unwrap();

        let created = create_note(
            &s,
            &vault,
            Some(&book),
            NewNoteInput {
                book_id: Some(book_id),
                body: "Sunja's dignity under [[Han]] pressure.".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert!(created.file.exists());
        let content = std::fs::read_to_string(&created.file).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("kind: note"));
        assert_eq!(created.links, vec!["Han".to_string()]);

        let hits = s.search_notes("dignity", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.id, created.id);

        std::fs::remove_dir_all(&vault).ok();
    }

    #[tokio::test]
    async fn anchor_lands_in_frontmatter_and_db() {
        let vault = std::env::temp_dir().join(format!("rb-vault-anchor-{}", std::process::id()));
        std::fs::remove_dir_all(&vault).ok();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book_id = s
            .upsert_book(&Book {
                title: Some("Pachinko".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        let book = s.get_book(book_id).await.unwrap().unwrap();

        let created = create_note(
            &s,
            &vault,
            Some(&book),
            NewNoteInput {
                book_id: Some(book_id),
                page: Some(128),
                location: Some("Chapter 3".into()),
                body: "The register shifts here.".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let content = std::fs::read_to_string(&created.file).unwrap();
        assert!(content.contains("page: 128"));
        assert!(content.contains("location: \"Chapter 3\""));

        let rec = s.list_notes(Some(book_id)).await.unwrap();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].page, Some(128));
        assert_eq!(rec[0].location.as_deref(), Some("Chapter 3"));

        std::fs::remove_dir_all(&vault).ok();
    }
}
