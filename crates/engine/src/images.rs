use std::path::{Path, PathBuf};

use reqwest::Client;
use url::Url;

use crate::error::{EngineError, Result};

/// Download an image URL into `images_dir`, named after the URL's last path
/// segment. Returns the written path.
pub async fn image_from_url(client: &Client, url_str: &str, images_dir: &Path) -> Result<PathBuf> {
    let url = Url::parse(url_str)?;
    let fname = url
        .path_segments()
        .and_then(|mut segs| segs.next_back().map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "cover.jpg".to_string());

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
        return Err(EngineError::Other(format!("empty image response from {url_str}")));
    }
    std::fs::write(&path, &bytes)?;
    Ok(path)
}
