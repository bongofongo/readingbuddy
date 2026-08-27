//! Tier 2: sidecars derived from real Project Gutenberg epubs.
//!
//! Output must be a pure function of `(seed, GENERATOR_VERSION, epub bytes)`.
//! Everything here exists to keep that true: a pinned PRNG algorithm, a fixed
//! date epoch, sorted iteration, and a lock file recording what produced what.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use sha2::{Digest, Sha256};

/// Bump deliberately when the *shape* of the output changes. That is the signal
/// to regenerate goldens; a seed change is not.
///
/// 2: the two layouts moved into `modern/` and `legacy/` subtrees. Emitted side
/// by side they were two sidecars claiming the *same* annotations of the same
/// book with different payloads, which no device can produce and which import
/// cannot converge on — see the comment at the write site.
pub const GENERATOR_VERSION: u32 = 2;

/// Fixed epoch for synthetic reading timestamps. Never `now()` — a clock in the
/// generator would churn every golden on every run.
const EPOCH: &str = "2026-01-01";

/// One paragraph located within the book, in the coordinates crengine uses.
struct Para {
    /// 1-based index of the spine item. This is crengine's `DocFragment[N]`.
    fragment: usize,
    /// Element path within the fragment body, e.g. `p[12]` or `div[2]/p[3]`.
    element: String,
    text: String,
    /// Nearest preceding heading in the same fragment.
    chapter: Option<String>,
}

pub struct Options {
    pub seed: u64,
    pub per_book: usize,
}

pub fn generate(
    manifest: &Path,
    epub_dir: &Path,
    out: &Path,
    opts: &Options,
) -> std::io::Result<usize> {
    let raw = std::fs::read_to_string(manifest)?;
    let man: serde_json::Value =
        serde_json::from_str(&raw).expect("corpus/manifest.json must be valid JSON");
    let books = man["books"].as_array().cloned().unwrap_or_default();

    std::fs::create_dir_all(out)?;
    let mut lock = BTreeMap::new();
    let mut written = 0;
    let mut missing = Vec::new();

    for b in &books {
        let id = b["id"].as_u64().unwrap_or(0);
        let title = b["title"].as_str().unwrap_or("Untitled");
        let author = b["author"].as_str().unwrap_or("");
        let epub_path = epub_dir.join(format!("{id}.epub"));

        if !epub_path.is_file() {
            missing.push(id);
            continue;
        }
        let bytes = std::fs::read(&epub_path)?;
        let epub_sha = hex(&Sha256::digest(&bytes));

        let paras = match extract_paragraphs(&epub_path) {
            Ok(p) if !p.is_empty() => p,
            Ok(_) => {
                eprintln!("  {id}: no paragraphs found, skipping");
                continue;
            }
            Err(e) => {
                eprintln!("  {id}: unreadable ({e}), skipping");
                continue;
            }
        };

        // Seeded per book, so adding or removing a book cannot shift what every
        // other book generates.
        let mut rng = ChaCha8Rng::seed_from_u64(opts.seed ^ id.wrapping_mul(0x9E37_79B9));

        // Prefixed with the Gutenberg id, because a slug alone is neither
        // unique nor sufficient: every non-Latin title (the Japanese, Chinese
        // and Russian entries) slugifies to nothing at all.
        let slug = format!("{id}-{}", slugify(title));

        // ONE highlight set, encoded two ways. Drawing separately would give
        // the two layouts different passages, and the whole point of emitting
        // both is that they are the same content in two encodings — which makes
        // the corpus a differential test of the modern and legacy parsers.
        //
        // **The two layouts go in separate trees, and that is not tidiness.**
        // They share `doc_props.title`, so side by side in one tree both match
        // the same library book; and they share `datetime`/`pos0`/`text`, so
        // their annotations share an `identity_hash`. But their payloads differ
        // — page is `3i` here and `7i` there, and only the legacy one carries
        // notes. Import would then refresh every row toward whichever sidecar
        // it read last, every time, for ever: each pass reports `updated`
        // rather than `skipped` and idempotency can never be observed. No
        // device produces that (a mid-migration file carries both layouts in
        // ONE file, where `annotations` wins), so the corpus must not either.
        let hs = highlights(&paras, opts.per_book, &mut rng);
        for (layout, body) in [
            ("modern", modern_sidecar(&hs, title, author)),
            ("legacy", legacy_sidecar(&hs, title, author)),
        ] {
            let dir = out.join(layout).join(format!("{slug}.sdr"));
            std::fs::create_dir_all(&dir)?;
            let file = dir.join("metadata.epub.lua");
            std::fs::write(&file, &body)?;
            lock.insert(
                format!("{layout}/{slug}.sdr/metadata.epub.lua"),
                hex(&Sha256::digest(body.as_bytes())),
            );
            written += 1;
        }
        lock.insert(format!("epub/{id}.epub"), epub_sha);
    }

    // The lock is what lets a test tell "not generated yet" apart from
    // "generated, then hand-edited" — the second is a silent corruption of the
    // corpus that would otherwise pass unnoticed.
    let lockfile = serde_json::json!({
        "generator_version": GENERATOR_VERSION,
        "seed": opts.seed,
        "per_book": opts.per_book,
        "files": lock,
    });
    std::fs::write(
        out.join("corpus.lock.json"),
        format!("{}\n", serde_json::to_string_pretty(&lockfile).unwrap()),
    )?;

    if !missing.is_empty() {
        eprintln!(
            "note: {} epub(s) absent — run scripts/fetch-corpus.sh: {:?}",
            missing.len(),
            missing
        );
    }
    Ok(written)
}

/// Walk the spine in order and collect every paragraph, with the crengine
/// coordinates a KOReader xpointer is built from.
///
/// The mapping is mechanical: `DocFragment[N]` is the N-th spine item (1-based,
/// in OPF spine order), and within it `p[M]` / `div[k]/p[M]` addresses the
/// element.
fn extract_paragraphs(path: &Path) -> Result<Vec<Para>, String> {
    let mut doc = epub::doc::EpubDoc::new(path).map_err(|e| e.to_string())?;
    // `spine` is Vec<SpineItem>; the idref is the manifest key we resolve.
    let spine: Vec<String> = doc.spine.iter().map(|it| it.idref.clone()).collect();
    let mut out = Vec::new();

    for (idx, item) in spine.iter().enumerate() {
        let Some((content, _mime)) = doc.get_resource(item) else {
            continue;
        };
        let xhtml = String::from_utf8_lossy(&content).into_owned();
        out.extend(paragraphs_of(&xhtml, idx + 1));
    }
    Ok(out)
}

/// Pull-parse one XHTML fragment. Deliberately simple: epub content is
/// well-formed XML, and only `<p>`, `<div>` and `<h1..h3>` matter here.
fn paragraphs_of(xhtml: &str, fragment: usize) -> Vec<Para> {
    let mut reader = quick_xml::Reader::from_str(xhtml);
    reader.config_mut().trim_text(true);

    let mut out = Vec::new();
    let mut buf = Vec::new();
    let mut p_index = 0usize;
    let mut div_index = 0usize;
    let mut in_div = false;
    let mut chapter: Option<String> = None;

    let mut capturing: Option<&'static str> = None;
    let mut current = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                match name.as_str() {
                    "p" => {
                        p_index += 1;
                        capturing = Some("p");
                        current.clear();
                    }
                    "div" | "section" => {
                        div_index += 1;
                        in_div = true;
                    }
                    "h1" | "h2" | "h3" => {
                        capturing = Some("h");
                        current.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if capturing.is_some() {
                    current.push_str(&t.xml10_content().unwrap_or_default());
                    current.push(' ');
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                match name.as_str() {
                    "p" => {
                        let text = squash(&current);
                        // Very short paragraphs make poor highlights.
                        if text.chars().count() >= 40 {
                            let element = if in_div {
                                format!("div[{div_index}]/p[{p_index}]")
                            } else {
                                format!("p[{p_index}]")
                            };
                            out.push(Para {
                                fragment,
                                element,
                                text,
                                chapter: chapter.clone(),
                            });
                        }
                        capturing = None;
                    }
                    "h1" | "h2" | "h3" => {
                        let t = squash(&current);
                        if !t.is_empty() {
                            chapter = Some(t);
                        }
                        capturing = None;
                    }
                    "div" | "section" => in_div = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pick `n` paragraphs and slice a highlight out of each.
///
/// All slicing is on **character** boundaries, which is not optional: the
/// Japanese and Chinese books in the manifest are multi-byte throughout, and
/// byte slicing would panic on most of them.
/// (fragment, element path, start char, end char, text, chapter)
type Highlight = (usize, String, usize, usize, String, Option<String>);

fn highlights(paras: &[Para], n: usize, rng: &mut ChaCha8Rng) -> Vec<Highlight> {
    let mut out = Vec::new();
    if paras.is_empty() {
        return out;
    }
    for i in 0..n {
        let p = &paras[(rng.next_u32() as usize) % paras.len()];
        let chars: Vec<char> = p.text.chars().collect();
        if chars.len() < 10 {
            continue;
        }
        let start = (rng.next_u32() as usize) % (chars.len() - 5);
        // Every sixth highlight is a single word, so the flashcard path gets
        // real coverage — including on the space-less CJK texts, where the
        // whole slice counts as one "word".
        let len = if i % 6 == 0 {
            1 + (rng.next_u32() as usize) % 12
        } else {
            20 + (rng.next_u32() as usize) % 160
        };
        let end = (start + len).min(chars.len());
        let text: String = chars[start..end].iter().collect();
        if text.trim().is_empty() {
            continue;
        }
        out.push((
            p.fragment,
            p.element.clone(),
            start,
            end,
            text.trim().to_string(),
            p.chapter.clone(),
        ));
    }
    out
}

fn modern_sidecar(hs: &[Highlight], title: &str, author: &str) -> String {
    let mut s = String::from(
        "-- GENERATED from a Project Gutenberg epub by `corpus gen-corpus`.\n\
         -- Modern `annotations` layout. Do not hand-edit.\n\
         return {\n    [\"annotations\"] = {\n",
    );
    for (i, (frag, elem, start, end, text, chapter)) in hs.iter().enumerate() {
        // pageno is a monotone counter, not a real page: crengine paginates
        // from font size and screen geometry, which an epub simply does not
        // determine. A counter is the honest choice.
        let pageno = (i + 1) * 3;
        let chapter_line = chapter
            .as_ref()
            .map(|c| format!("            [\"chapter\"] = \"{}\",\n", lua_escape(c)))
            .unwrap_or_default();
        let (hh, mm) = ((i / 60) % 24, i % 60);
        let text = lua_escape(text);
        s.push_str(&format!(
            "        [{}] = {{\n\
             {chapter_line}\
                 [\"datetime\"] = \"{EPOCH} {hh:02}:{mm:02}:00\",\n\
                 [\"pageno\"] = {pageno},\n\
                 [\"pos0\"] = \"/body/DocFragment[{frag}]/body/{elem}/text().{start}\",\n\
                 [\"pos1\"] = \"/body/DocFragment[{frag}]/body/{elem}/text().{end}\",\n\
                 [\"text\"] = \"{text}\",\n\
             }},\n",
            i + 1,
        ));
    }
    s.push_str(&format!(
        "    }},\n    [\"doc_props\"] = {{\n        [\"title\"] = \"{}\",\n        [\"authors\"] = \"{}\",\n    }},\n}}\n",
        lua_escape(title),
        lua_escape(author),
    ));
    s
}

fn legacy_sidecar(hs: &[Highlight], title: &str, author: &str) -> String {
    let mut s = String::from(
        "-- GENERATED from a Project Gutenberg epub by `corpus gen-corpus`.\n\
         -- Legacy `highlight` + `bookmarks` layout. Do not hand-edit.\n\
         return {\n    [\"highlight\"] = {\n",
    );
    for (i, (frag, elem, start, end, text, chapter)) in hs.iter().enumerate() {
        let page = (i + 1) * 7;
        let chapter_line = chapter
            .as_ref()
            .map(|c| format!("                [\"chapter\"] = \"{}\",\n", lua_escape(c)))
            .unwrap_or_default();
        let (hh, mm) = ((i / 60) % 24, i % 60);
        let text = lua_escape(text);
        s.push_str(&format!(
            "        [{page}] = {{\n            [1] = {{\n\
             {chapter_line}\
                     [\"datetime\"] = \"{EPOCH} {hh:02}:{mm:02}:00\",\n\
                     [\"pos0\"] = \"/body/DocFragment[{frag}]/body/{elem}/text().{start}\",\n\
                     [\"pos1\"] = \"/body/DocFragment[{frag}]/body/{elem}/text().{end}\",\n\
                     [\"text\"] = \"{text}\",\n\
                 }},\n        }},\n",
        ));
    }
    s.push_str("    },\n    [\"bookmarks\"] = {\n");
    // A note on every third highlight, joined to it by datetime — the legacy
    // format's one genuinely awkward feature.
    for (i, (_, _, _, _, text, _)) in hs.iter().enumerate().filter(|(i, _)| i % 3 == 0) {
        let (hh, mm) = ((i / 60) % 24, i % 60);
        let notes = lua_escape(text);
        s.push_str(&format!(
            "        [{n}] = {{\n\
                 [\"datetime\"] = \"{EPOCH} {hh:02}:{mm:02}:00\",\n\
                 [\"notes\"] = \"{notes}\",\n\
                 [\"text\"] = \"A note about passage {n}.\",\n\
             }},\n",
            n = i + 1,
        ));
    }
    s.push_str(&format!(
        "    }},\n    [\"doc_props\"] = {{\n        [\"title\"] = \"{}\",\n        [\"authors\"] = \"{}\",\n    }},\n}}\n",
        lua_escape(title),
        lua_escape(author),
    ));
    s
}

/// Escape for a Lua double-quoted string. The corpus is full of quotation
/// marks, backslashes and newlines straight out of nineteenth-century prose.
fn lua_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.extend(c.to_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    let t = out.trim_matches('-').to_string();
    if t.is_empty() { "untitled".into() } else { t }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn default_epub_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/epub")
}

pub fn default_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.json")
}

pub fn default_out() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/generated")
}
