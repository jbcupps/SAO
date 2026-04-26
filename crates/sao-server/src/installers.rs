//! Installer fetch + sha256-verified cache.
//!
//! Layout under SAO_DATA_DIR:
//!   installers/<sha256>/<filename>     -- the verified artifact
//!   installers/<sha256>/.url           -- the source URL for traceability
//!
//! Pinning a sha to a path means we can serve old agents the exact MSI they were created with
//! while the default rolls forward.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use crate::db::installer_sources::InstallerSourceRow;

#[allow(dead_code)] // NotConfigured reserved for callers that don't pre-check the DB
#[derive(Debug, Error)]
pub enum InstallerError {
    #[error("installer source not configured")]
    NotConfigured,
    #[error("installer download failed: {0}")]
    Http(String),
    #[error("installer hash mismatch (expected {expected}, got {actual})")]
    HashMismatch { expected: String, actual: String },
    #[error("installer io: {0}")]
    Io(#[from] std::io::Error),
}

/// Returns the local path to the cached installer for `source`, downloading if absent.
/// On success the file at the returned path is byte-for-byte the upstream artifact and its
/// sha256 matches `source.expected_sha256`.
pub async fn fetch_or_cache(source: &InstallerSourceRow) -> Result<PathBuf, InstallerError> {
    let cache_dir = installers_root().join(&source.expected_sha256);
    let target = cache_dir.join(&source.filename);

    if target.exists() {
        // Re-verify on every serve — cheap insurance against bit-rot or substitution.
        let actual = sha256_of_file(&target).await?;
        if actual.eq_ignore_ascii_case(&source.expected_sha256) {
            return Ok(target);
        }
        tracing::warn!(
            path = %target.display(),
            "Cached installer hash mismatch — refetching"
        );
        let _ = tokio::fs::remove_file(&target).await;
    }

    tokio::fs::create_dir_all(&cache_dir).await?;

    tracing::info!(url = %source.url, "Downloading OrionII installer");
    let resp = reqwest::Client::builder()
        .build()
        .map_err(|e| InstallerError::Http(e.to_string()))?
        .get(&source.url)
        .send()
        .await
        .map_err(|e| InstallerError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(InstallerError::Http(format!(
            "GET {} returned {}",
            source.url,
            resp.status()
        )));
    }

    let body = resp
        .bytes()
        .await
        .map_err(|e| InstallerError::Http(e.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(&body);
    let mut tmp = tokio::fs::File::create(&target).await?;
    tmp.write_all(&body).await?;
    tmp.flush().await?;
    drop(tmp);

    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(&source.expected_sha256) {
        let _ = tokio::fs::remove_file(&target).await;
        return Err(InstallerError::HashMismatch {
            expected: source.expected_sha256.clone(),
            actual,
        });
    }

    let url_marker = cache_dir.join(".url");
    let _ = tokio::fs::write(&url_marker, &source.url).await;

    Ok(target)
}

/// Locate a previously cached installer by its sha256 (used when serving a pinned bundle).
pub fn cached_path(sha256: &str, filename: &str) -> Option<PathBuf> {
    let p = installers_root().join(sha256).join(filename);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

/// Compute sha256 of a file you intend to register as a source. Useful for the admin flow:
/// admin uploads or points at a URL, server confirms the sha matches what they expect.
pub async fn sha256_of_url(url: &str) -> Result<String, InstallerError> {
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| InstallerError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(InstallerError::Http(format!(
            "GET {url} returned {}",
            resp.status()
        )));
    }
    let body = resp
        .bytes()
        .await
        .map_err(|e| InstallerError::Http(e.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(&body);
    Ok(format!("{:x}", hasher.finalize()))
}

async fn sha256_of_file(path: &Path) -> std::io::Result<String> {
    use tokio::io::AsyncReadExt;
    let mut f = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn installers_root() -> PathBuf {
    let base = std::env::var_os("SAO_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/sao"));
    base.join("installers")
}
