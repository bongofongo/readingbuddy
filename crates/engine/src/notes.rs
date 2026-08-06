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
    /// Private, the hub of the graph: cites highlights, links notes and other
    /// reflections, and is **openable mid-book** rather than written at the end.
    /// Supersedes the old `final` (migration `0007` rewrites those rows).
    Reflection,
    /// Public: prose for other people, and the only kind that carries a rating.
    /// Never derived from the reflection — a review is a rewrite for a different
    /// audience, not a subset of private thinking.
    Review,
}

impl NoteKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NoteKind::Note => "note",
            NoteKind::Session => "session",
            NoteKind::Reflection => "reflection",
            NoteKind::Review => "review",
        }
    }

    /// True for the two kinds that anchor to a reading and are opened through
    /// [`crate::Engine::open_reflection`] / [`crate::Engine::open_review`]
    /// rather than written like an ordinary note.
    pub fn is_anchored(&self) -> bool {
        matches!(self, NoteKind::Reflection | NoteKind::Review)
    }
}

impl std::str::FromStr for NoteKind {
    type Err = EngineError;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "note" => Ok(NoteKind::Note),
            "session" => Ok(NoteKind::Session),
            // `final` is what a reflection used to be called, and old vault
            // files still say it in their frontmatter. Parsed, never written.
            "reflection" | "final" => Ok(NoteKind::Reflection),
            "review" => Ok(NoteKind::Review),
            other => Err(EngineError::InvalidInput(format!(
                "unknown note kind: {other}"
            ))),
        }
    }
}

#[derive(Debug, Default)]
pub struct NewNoteInput {
    pub book_id: Option<i64>,
    /// The reading this note belongs to. Always set for a reflection or a
    /// review; `None` for an ordinary note, which floats free of any one
    /// reading.
    pub reading_id: Option<i64>,
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
    if out.is_empty() {
        "untitled".to_string()
    } else {
        out
    }
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
    reading_id: Option<i64>,
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
        fm.push_str(&format!(
            "book-title: \"{}\"\n",
            b.display_title().replace('"', "'")
        ));
    }
    // Beside the book, because it is part of the same anchor: which book, and
    // which time through it.
    if let Some(r) = reading_id {
        fm.push_str(&format!("reading: {r}\n"));
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

/// True when a closing `---` is the whole line — i.e. what follows the three
/// dashes is a line break or the end of input. This is what separates a real
/// fence from a `----` horizontal rule or a `---foo`.
fn is_line_end(after: &str) -> bool {
    after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n")
}

/// Byte offset within `rest` just past a closing `---` fence, or `None`.
fn find_closing_fence(rest: &str) -> Option<usize> {
    // The fence can be the very first line — an empty frontmatter block,
    // `---\n---\n`. Searching only for `"\n---"` misses it, because there is no
    // preceding newline to match, and the whole file then reads as body.
    if let Some(after) = rest.strip_prefix("---")
        && is_line_end(after)
    {
        return Some("---".len());
    }
    let mut from = 0;
    while let Some(rel) = rest[from..].find("\n---") {
        let past = from + rel + "\n---".len();
        if is_line_end(&rest[past..]) {
            return Some(past);
        }
        from += rel + 1;
    }
    None
}

/// Split a note file into its raw frontmatter header (through the closing
/// `---` and any blank line after it) and the body. When there is no
/// frontmatter the header is empty and the body is the whole content.
pub fn frontmatter_and_body(content: &str) -> (&str, &str) {
    let Some(rest) = content.strip_prefix("---\n") else {
        return ("", content);
    };
    // The closing fence is a line that is exactly `---`. Searching for a bare
    // `"\n---"` also matched `\n----` (an ordinary markdown horizontal rule)
    // and `\n---foo`, either of which would be taken as the terminator and
    // mangle the split — losing part of the user's note body into the header.
    let Some(past_fence) = find_closing_fence(rest) else {
        return ("", content);
    };
    // Byte offset just past the closing `---`, then past its trailing newlines.
    let after_close = "---\n".len() + past_fence;
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
        words
            .join(" ")
            .trim_end_matches([',', '.', ';', ':'])
            .to_string()
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
    let stamp = now.format(format_description!(
        "[year][month][day][hour][minute][second]"
    ))?;
    let base = format!("{stamp}-{}", slugify(&title));
    let mut file_name = format!("{base}.md");
    let mut n = 1;
    while dir.join(&file_name).exists() {
        n += 1;
        file_name = format!("{base}-{n}.md");
    }
    let file = dir.join(&file_name);
    let rel_path = format!("{book_dir}/{file_name}");

    let created_str = now.format(&time::format_description::well_known::Rfc3339)?;
    let content = format!(
        "{}{}\n",
        frontmatter(
            book,
            input.reading_id,
            input.highlight_id,
            input.page,
            input.location.as_deref(),
            input.kind,
            &created_str,
        ),
        input.body.trim_end()
    );
    std::fs::write(&file, &content)?;

    // **Index the body that was actually written**, not the one that was
    // passed in. They differ by a trim and a newline, and until item 24 that
    // difference meant a note was never byte-identical to its own index — so
    // the first time anything compared the file against the index it found a
    // change that had not happened, and re-indexed the entire vault.
    let (_, written_body) = frontmatter_and_body(&content);
    let links = extract_wikilinks(written_body);
    let id = storage
        .insert_note(
            crate::storage::NewNoteMeta {
                book_id: input.book_id,
                reading_id: input.reading_id,
                highlight_id: input.highlight_id,
                page: input.page,
                location: input.location.as_deref(),
                file_path: &rel_path,
                title: &title,
                kind: input.kind.as_str(),
            },
            written_body,
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

// ---- vault coherence (item 24) ---------------------------------------------

/// What a sweep of the vault found, in the past tense.
///
/// **Not a to-do list.** `absent` counts notes whose file was not there when we
/// looked; it exists so the non-destructive ruling below can be *asserted*, and
/// so the engine can say something in its own log. No frontend turns it into "3
/// notes out of sync" — a stale index is the app's problem, not a chore
/// assigned to the reader, and there is no screen in this repo that counts what
/// the user has not done.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VaultReconcile {
    /// Notes whose file was looked at.
    pub checked: usize,
    /// Notes whose index was behind their file and has caught up.
    pub reindexed: usize,
    /// Notes whose file was not there. Nothing was written for any of them.
    pub absent: usize,
}

/// A note file's body, or `None` when there is no file there.
///
/// A read error that is *not* absence propagates: a permission problem is worth
/// hearing about, while a file that is simply gone is an ordinary state of the
/// vault (see [`reconcile_vault`]).
pub(crate) fn body_on_disk(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let (_, body) = frontmatter_and_body(&content);
            Ok(Some(body.to_string()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Bring one note's index in line with a body read from its file, and say
/// whether that changed anything.
///
/// **The one place a note's index catches up with its file** — the watcher, the
/// sweep and [`crate::Engine::refresh_note_from_disk`] all come through here,
/// so none of them can hold a different opinion about what a change is.
///
/// The comparison against what is already indexed is what makes seeing our own
/// writes free rather than merely harmless: `create_note` and
/// `update_note_body` write the file *and* the index, and the watcher then sees
/// the file event they caused. Without this the echo is one extra pointless
/// transaction per save; with it, it is one read and one `SELECT`. It also
/// absorbs the ordinary editor no-op — Obsidian rewrites a file on focus loss
/// whether or not a character changed.
///
/// Compared **trim-end**: an editor that adds a trailing newline on save has
/// not changed what the note says, and treating it as a change would re-index
/// the whole vault the first time one ran over it.
pub(crate) async fn reindex_from_body(
    storage: &Storage,
    note_id: i64,
    title: &str,
    body: &str,
) -> Result<bool> {
    let indexed = storage.indexed_body(note_id).await?;
    if indexed.as_deref().map(str::trim_end) == Some(body.trim_end()) {
        return Ok(false);
    }
    let links = extract_wikilinks(body);
    storage.reindex_note(note_id, title, body, &links).await?;
    Ok(true)
}

/// A path under the vault, as `notes.file_path` spells it.
///
/// `None` for anything that is not a markdown file inside the vault — an
/// editor's swap file, `.obsidian/workspace.json`, a directory. Components are
/// joined with `/` rather than the platform separator because that is what
/// `create_note` wrote, and a lookup that spelled it the other way would find
/// nothing on Windows and never say why.
pub(crate) fn vault_relative(vault: &Path, path: &Path) -> Option<String> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        return None;
    }
    let rest = path.strip_prefix(vault).ok()?;
    let parts: Vec<&str> = rest
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?;
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// Walk every note and bring the ones whose file has moved ahead back in line.
///
/// **This is the half a watcher cannot do**, and the reason it exists beside
/// one rather than instead of one: a watcher only ever sees the present, and
/// the ordinary case is a note edited in Obsidian on Tuesday and searched for
/// in readingbuddy on Thursday. It is also the whole of the answer on a
/// platform where `notify` will not start — `docs/decisions.md` makes absence a
/// first-class answer, and an unwatchable directory must still hold a working
/// vault.
///
/// **Two stages, the pattern `sidecar_seen` established.** `stat` first and
/// skip when the file is no newer than the index; only then read and compare.
/// One caveat, stated rather than papered over: `notes.last_modified` is unix
/// *seconds*, so an external edit landing in the same second as one of our own
/// index writes is skipped by this sweep. The watcher is what covers a live
/// edit; this covers a cold one, and a cold one is never in the same second.
pub(crate) async fn reconcile_vault(storage: &Storage, vault: &Path) -> Result<VaultReconcile> {
    let mut report = VaultReconcile::default();
    for note in storage.note_files().await? {
        let path = vault.join(&note.file_path);
        report.checked += 1;

        let Ok(meta) = std::fs::metadata(&path) else {
            // Absent, unreadable, or a directory where a note should be. None
            // of those is a deletion — see `VaultWatcher`'s ruling.
            report.absent += 1;
            continue;
        };
        let newer = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .is_none_or(|d| d.as_secs() as i64 > note.last_modified);
        if !newer {
            continue;
        }
        let Some(body) = body_on_disk(&path)? else {
            report.absent += 1;
            continue;
        };
        if reindex_from_body(storage, note.id, &note.title, &body).await? {
            report.reindexed += 1;
        }
    }
    // The path is deliberately not in here: a note's filename is its slugified
    // title, which `derive_title` takes from the first six words of the body.
    // A vault path is the user's prose and belongs no higher than `trace!`.
    tracing::debug!(
        checked = report.checked,
        reindexed = report.reindexed,
        absent = report.absent,
        "vault reconciled"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoteScope;

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
            vec![
                "Han".to_string(),
                "Zettelkasten".to_string(),
                "Pachinko".to_string()
            ]
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
            Some(11),
            Some(3),
            Some(42),
            Some("Chapter 2"),
            NoteKind::Session,
            "2026-07-23T10:00:00Z",
        );
        let content = format!("{fm}The body text with [[Link]].\n");
        let (pairs, body) = parse_frontmatter(&content);
        let get = |k: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("book"), Some("9781455563937"));
        assert_eq!(get("book-title"), Some("Pachinko"));
        assert_eq!(get("reading"), Some("11"));
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
        assert_eq!(
            format!("{header}rewritten\n"),
            "---\npage: 5\nkind: note\n---\n\nrewritten\n"
        );

        // No frontmatter: empty header, whole content is body.
        let (h2, b2) = frontmatter_and_body("just text\n");
        assert_eq!(h2, "");
        assert_eq!(b2, "just text\n");
    }

    /// A markdown horizontal rule is `----`, and a bare `find("\n---")` matches
    /// its first four bytes. Taking that as the closing fence swallows part of
    /// the user's note into the header — and `update_note_body` then writes the
    /// mangled split back to disk, so the damage is permanent.
    #[test]
    fn a_horizontal_rule_in_the_body_is_not_mistaken_for_the_closing_fence() {
        let content = "---\nkind: note\n---\n\nIntro paragraph.\n\n----\n\nAfter the rule.\n";
        let (header, body) = frontmatter_and_body(content);

        assert_eq!(header, "---\nkind: note\n---\n\n");
        assert_eq!(body, "Intro paragraph.\n\n----\n\nAfter the rule.\n");

        // Same for a fence-like line that merely starts with three dashes.
        let dashed = "---\nkind: note\n---\n\nSee ---foo below.\n";
        let (h, b) = frontmatter_and_body(dashed);
        assert_eq!(h, "---\nkind: note\n---\n\n");
        assert_eq!(b, "See ---foo below.\n");
    }

    /// Found by the partition property below, shrunk to this: the closing
    /// fence of an *empty* frontmatter block sits on the first line, where a
    /// search for "\n---" cannot see it — so the whole file read as body.
    #[test]
    fn an_empty_frontmatter_block_is_still_frontmatter() {
        let (h, b) = frontmatter_and_body("---\n---\n\nbody text\n");
        assert_eq!(h, "---\n---\n\n");
        assert_eq!(b, "body text\n");

        // And with nothing after it at all.
        let (h2, b2) = frontmatter_and_body("---\n---\n");
        assert_eq!(h2, "---\n---\n");
        assert_eq!(b2, "");
    }

    /// Whatever the input, the two halves must reassemble into exactly the
    /// original. `update_note_body` rewrites `header + new_body`, so any byte
    /// lost here is a byte lost from someone's note file.
    #[test]
    fn the_split_always_partitions_the_input() {
        for content in [
            "",
            "---",
            "---\n",
            "---\n---\n",
            "---\nkind: note\n---\n\nbody\n",
            "no frontmatter at all\n",
            "---\nkind: note\n\nunterminated frontmatter\n",
            "---\na: b\n---\n\n----\n\nrule\n",
            "---\r\na: b\r\n---\r\n\r\nCRLF body\r\n",
            "text\n---\nnot frontmatter, wrong position\n",
        ] {
            let (h, b) = frontmatter_and_body(content);
            assert_eq!(
                format!("{h}{b}"),
                content,
                "split lost or duplicated bytes for {content:?}"
            );
        }
    }

    #[test]
    fn derive_title_takes_leading_words() {
        assert_eq!(
            derive_title("Sunja's choice mirrors the whole diaspora, somehow."),
            "Sunja's choice mirrors the whole diaspora"
        );
        assert_eq!(derive_title(""), "Untitled");
    }

    #[tokio::test]
    async fn create_note_writes_file_and_indexes() {
        let vault = std::env::temp_dir().join(format!("rb-vault-test-{}", std::process::id()));
        std::fs::remove_dir_all(&vault).ok();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book_id = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    ..Default::default()
                },
                None,
            )
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

        let hits = s
            .search_marks("dignity", Some(crate::SearchSource::Note), None, 10)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].as_note().unwrap().id, created.id);

        std::fs::remove_dir_all(&vault).ok();
    }

    #[tokio::test]
    async fn anchor_lands_in_frontmatter_and_db() {
        let vault = std::env::temp_dir().join(format!("rb-vault-anchor-{}", std::process::id()));
        std::fs::remove_dir_all(&vault).ok();

        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let book_id = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    ..Default::default()
                },
                None,
            )
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

        let rec = s.list_notes(NoteScope::Book(book_id), None).await.unwrap();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].page, Some(128));
        assert_eq!(rec[0].location.as_deref(), Some("Chapter 3"));

        std::fs::remove_dir_all(&vault).ok();
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The strongest thing that can be said about the split, and the one
        /// that protects the user's writing: `update_note_body` rewrites
        /// `header + new_body`, so any byte this loses is a byte lost from a
        /// note file on disk.
        #[test]
        fn the_split_is_always_a_partition(content in ".{0,200}") {
            let (h, b) = frontmatter_and_body(&content);
            prop_assert_eq!(format!("{h}{b}"), content.clone());
        }

        /// Generated note-shaped input, so the property exercises the real
        /// parsing path rather than mostly hitting the no-frontmatter early
        /// return.
        #[test]
        fn note_shaped_input_splits_cleanly(
            keys in proptest::collection::vec("[a-z-]{1,10}", 0..5),
            body in ".{0,80}",
        ) {
            let mut content = String::from("---\n");
            for (i, k) in keys.iter().enumerate() {
                content.push_str(&format!("{k}: value{i}\n"));
            }
            content.push_str("---\n\n");
            content.push_str(&body);

            let (h, b) = frontmatter_and_body(&content);
            prop_assert_eq!(format!("{h}{b}"), content.clone());
            prop_assert!(h.starts_with("---\n"), "header lost its opening fence");
            prop_assert_eq!(b, body.as_str());
        }

        /// `slugify` names a file on disk. Empty, or `.`/`..`, or something
        /// with a separator in it would be a broken or dangerous path.
        #[test]
        fn a_slug_is_always_a_safe_filename(s in ".{0,120}") {
            let slug = slugify(&s);
            prop_assert!(!slug.is_empty());
            prop_assert!(!slug.starts_with('-') && !slug.ends_with('-'), "{:?}", slug);
            prop_assert!(!slug.contains("--"), "{:?}", slug);
            prop_assert!(slug != "." && slug != "..", "{:?}", slug);
            prop_assert!(
                !slug.contains('/') && !slug.contains('\\') && !slug.contains(':'),
                "path separator in {:?}", slug
            );
            // The 60 cap is a *byte* check applied after pushing, so multibyte
            // input can overshoot by up to one character's width. Assert the
            // real bound rather than a 60 that would be wrong.
            prop_assert!(slug.len() <= 60 + 4, "unbounded slug: {} bytes", slug.len());
            prop_assert_eq!(slugify(&slug), slug.clone(), "slugify is not idempotent");
        }

        #[test]
        fn wikilink_targets_are_always_substrings_of_the_body(body in ".{0,200}") {
            let links = extract_wikilinks(&body);
            let mut seen = Vec::new();
            for l in &links {
                prop_assert!(!l.is_empty());
                prop_assert!(body.contains(l.as_str()), "{:?} not in body", l);
                prop_assert!(!l.contains(']') && !l.contains('|') && !l.contains('#'), "{:?}", l);
                prop_assert!(!seen.contains(l), "duplicate target {:?}", l);
                seen.push(l.clone());
            }
        }

        #[test]
        fn well_formed_wikilinks_are_extracted(
            names in proptest::collection::vec("[a-z][a-z ]{0,10}", 1..4),
        ) {
            let body: String = names
                .iter()
                .map(|n| format!("[[{n}]] "))
                .collect();
            let got = extract_wikilinks(&body);
            let mut want: Vec<String> = Vec::new();
            for n in &names {
                let t = n.trim().to_string();
                if !t.is_empty() && !want.contains(&t) {
                    want.push(t);
                }
            }
            prop_assert_eq!(got, want);
        }
    }
}
