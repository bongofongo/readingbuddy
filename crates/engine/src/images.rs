use std::path::{Path, PathBuf};

use reqwest::Client;
use url::Url;

use crate::error::{EngineError, Result};

/// Pick the on-disk filename for a downloaded image, from the URL's last path
/// segment.
///
/// Split out from [`image_from_url`] so the guard below is reachable from a
/// test: the name is attacker-influenced (it comes from whatever URL a metadata
/// provider handed us) and is about to be joined onto a directory we own.
/// `Url::parse` already normalizes `..` away, so this is believed unreachable
/// today — but "safe because of what a dependency happens to do" is not a
/// property to leave unasserted on a path write.
fn filename_from_url(url: &Url) -> Result<String> {
    let fname = url
        .path_segments()
        .and_then(|mut segs| segs.next_back().map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "cover.jpg".to_string());

    // Must be exactly one plain component — no separators, no `..`, no root.
    let p = Path::new(&fname);
    let plain = p.components().count() == 1 && p.file_name().is_some_and(|n| n == fname.as_str());
    if !plain {
        return Err(EngineError::InvalidInput(format!(
            "refusing image filename derived from URL: {fname:?}"
        )));
    }
    Ok(fname)
}

/// Download an image URL into `images_dir`, named after the URL's last path
/// segment. Returns the written path.
pub async fn image_from_url(client: &Client, url_str: &str, images_dir: &Path) -> Result<PathBuf> {
    let url = Url::parse(url_str)?;
    let fname = filename_from_url(&url)?;

    std::fs::create_dir_all(images_dir)?;
    let path = images_dir.join(fname);

    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if bytes.is_empty() {
        return Err(EngineError::Other(format!(
            "empty image response from {url_str}"
        )));
    }
    std::fs::write(&path, &bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_of(u: &str) -> Result<String> {
        filename_from_url(&Url::parse(u).unwrap())
    }

    #[test]
    fn ordinary_urls_yield_their_last_segment() {
        assert_eq!(name_of("https://x.test/covers/abc.jpg").unwrap(), "abc.jpg");
        assert_eq!(
            name_of("https://x.test/a/b/c/9780141439549.png").unwrap(),
            "9780141439549.png"
        );
        // No usable segment at all -> the documented fallback.
        assert_eq!(name_of("https://x.test/").unwrap(), "cover.jpg");
    }

    /// The reason the guard exists: whatever the URL says, the result must be a
    /// single filename that cannot climb out of `images_dir`.
    #[test]
    fn a_remote_url_can_never_escape_the_images_dir() {
        let dir = Path::new("/data/images");
        for u in [
            "https://x.test/../../etc/passwd",
            "https://x.test/a/../../../../etc/passwd",
            "https://x.test/%2e%2e%2f%2e%2e%2fetc/passwd",
            "https://x.test/foo/%2Fetc%2Fpasswd",
        ] {
            let parsed = Url::parse(u).unwrap();
            match filename_from_url(&parsed) {
                // Refused outright: fine.
                Err(_) => {}
                // Accepted: then it must genuinely stay put.
                Ok(name) => {
                    let joined = dir.join(&name);
                    assert_eq!(
                        joined.parent(),
                        Some(dir),
                        "{u} produced {name:?}, which lands outside images_dir at {joined:?}"
                    );
                    assert!(!name.contains('/'), "{u} produced a separator: {name:?}");
                }
            }
        }
    }
}
