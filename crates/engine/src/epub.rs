use std::path::{Path, PathBuf};

use epub::doc::EpubDoc;

use crate::book::normalize_isbn;
use crate::error::{EngineError, Result};
use crate::notes::slugify;

#[derive(Debug, Default)]
pub struct EpubInfo {
    pub isbn: Option<String>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
}

/// Read epub metadata. Scans ALL identifier entries for the first one that
/// validates as an ISBN (identifiers are often UUIDs or urn:isbn: forms).
pub fn epub_info(path: &Path) -> Result<EpubInfo> {
    let doc =
        EpubDoc::new(path).map_err(|e| EngineError::Epub(format!("{}: {e}", path.display())))?;
    let mut info = EpubInfo::default();
    for (key, values) in doc.metadata.iter() {
        match key.as_str() {
            "identifier" => {
                if info.isbn.is_none() {
                    info.isbn = values.iter().find_map(|v| normalize_isbn(v));
                }
            }
            "title" => info.title = values.first().cloned(),
            "creator" => info.authors = values.clone(),
            "language" => info.language = values.first().cloned(),
            _ => {}
        }
    }
    Ok(info)
}

/// Extract the embedded cover image into `images_dir`; returns the written
/// path. Extension guessed from the cover's mime type.
pub fn extract_cover(path: &Path, images_dir: &Path) -> Result<Option<PathBuf>> {
    let mut doc =
        EpubDoc::new(path).map_err(|e| EngineError::Epub(format!("{}: {e}", path.display())))?;
    let Some((data, mime)) = doc.get_cover() else {
        return Ok(None);
    };
    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "jpg",
    };
    let title = doc.mdata("title").unwrap_or_else(|| "cover".to_string());
    std::fs::create_dir_all(images_dir)?;
    let file = images_dir.join(format!("{}.{ext}", slugify(&title)));
    std::fs::write(&file, &data)?;
    Ok(Some(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Uses the sample epubs checked into the repo (`epubs/`).
    fn fixture() -> Option<PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../epubs/pachinko.epub");
        p.exists().then_some(p)
    }

    #[test]
    fn reads_metadata_from_fixture() {
        let Some(path) = fixture() else {
            eprintln!("fixture epub missing; skipping");
            return;
        };
        let info = epub_info(&path).unwrap();
        assert!(info.title.is_some());
        // If an ISBN was found it must be a validated, normalized one.
        if let Some(isbn) = &info.isbn {
            assert!(normalize_isbn(isbn).is_some());
        }
    }

    #[test]
    fn extracts_cover_from_fixture() {
        let Some(path) = fixture() else {
            eprintln!("fixture epub missing; skipping");
            return;
        };
        let dir = std::env::temp_dir().join(format!("rb-epub-test-{}", std::process::id()));
        let cover = extract_cover(&path, &dir).unwrap();
        if let Some(c) = cover {
            assert!(c.exists());
            assert!(std::fs::metadata(&c).unwrap().len() > 0);
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
