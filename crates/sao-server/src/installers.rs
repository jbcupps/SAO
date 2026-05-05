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

/// Installer kind for OrionII Windows MSI artifacts (the only kind today).
pub const KIND_ORION_MSI: &str = "orion-msi";

/// OLE2 compound document file signature. All Microsoft Installer (.msi)
/// packages start with these eight bytes; msiexec rejects anything else
/// with "This installation package could not be opened."
const MSI_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

#[allow(dead_code)] // NotConfigured reserved for callers that don't pre-check the DB
#[derive(Debug, Error)]
pub enum InstallerError {
    #[error("installer source not configured")]
    NotConfigured,
    #[error("installer download failed: {0}")]
    Http(String),
    #[error("installer hash mismatch (expected {expected}, got {actual})")]
    HashMismatch { expected: String, actual: String },
    #[error("installer content rejected: {reason} (looks like {looks_like})")]
    InvalidContent { reason: String, looks_like: String },
    #[error("installer io: {0}")]
    Io(#[from] std::io::Error),
}

/// Best-effort hint for "what file format does this byte stream look like?"
/// used purely to give operators a clear error when they paste the wrong URL
/// (e.g. registering a GitHub source-tarball as if it were an MSI).
pub fn sniff_format(bytes: &[u8]) -> &'static str {
    if bytes.is_empty() {
        return "empty body";
    }
    if bytes.len() >= MSI_MAGIC.len() && bytes[..MSI_MAGIC.len()] == MSI_MAGIC {
        return "Windows Installer (.msi / OLE2 compound document)";
    }
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return "ZIP archive (PK\\x03\\x04) — likely a GitHub source-code archive, not a built MSI";
    }
    if bytes.starts_with(b"MZ") {
        return "Windows PE executable (MZ) — likely a .exe installer, not an .msi";
    }
    if bytes.starts_with(b"%PDF-") {
        return "PDF document";
    }
    if bytes.starts_with(b"\x7FELF") {
        return "Linux ELF executable";
    }
    let prefix = &bytes[..bytes.len().min(512)];
    let snippet = std::str::from_utf8(prefix).unwrap_or("");
    let lower = snippet.to_ascii_lowercase();
    if lower.contains("<!doctype html") || lower.contains("<html") {
        "HTML document — the URL probably returned an error page or a sign-in interstitial"
    } else if lower.contains("not found") || lower.contains("404") {
        "HTTP error response body"
    } else if prefix
        .iter()
        .all(|b| b.is_ascii() && (*b == b'\n' || *b == b'\r' || *b >= 0x20))
    {
        "ASCII text"
    } else {
        "unknown / binary blob"
    }
}

/// Verify that `bytes` is acceptable for the given installer `kind`.
///
/// For `KIND_ORION_MSI` this enforces the OLE2 compound-document magic bytes
/// at the start of the file, so a Windows host will be able to open the
/// resulting bundle with `msiexec`. Returns a categorical
/// `InvalidContent` error with a clear `looks_like` hint on rejection.
pub fn validate_artifact_for_kind(kind: &str, bytes: &[u8]) -> Result<(), InstallerError> {
    match kind {
        KIND_ORION_MSI => {
            if bytes.len() < MSI_MAGIC.len() || bytes[..MSI_MAGIC.len()] != MSI_MAGIC {
                return Err(InstallerError::InvalidContent {
                    reason: "URL did not return a valid Windows Installer (.msi) package".into(),
                    looks_like: sniff_format(bytes).to_string(),
                });
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Returns the local path to the cached installer for `source`, downloading if absent.
/// On success the file at the returned path is byte-for-byte the upstream artifact, its
/// sha256 matches `source.expected_sha256`, and its file format matches the kind of
/// installer the row claims to be (e.g. OLE2 magic for `orion-msi`).
pub async fn fetch_or_cache(source: &InstallerSourceRow) -> Result<PathBuf, InstallerError> {
    let cache_dir = installers_root().join(&source.expected_sha256);
    let target = cache_dir.join(&source.filename);

    if target.exists() {
        // Re-verify on every serve — cheap insurance against bit-rot or substitution.
        let actual = sha256_of_file(&target).await?;
        if actual.eq_ignore_ascii_case(&source.expected_sha256) {
            // Also re-validate the kind-specific file signature: an attacker (or a
            // bug in an earlier release) could have planted bytes that pass sha
            // but aren't the expected format.
            let header = read_header(&target).await?;
            if let Err(e) = validate_artifact_for_kind(&source.kind, &header) {
                tracing::warn!(
                    path = %target.display(),
                    error = %e,
                    "Cached installer failed kind-specific validation — refetching",
                );
                let _ = tokio::fs::remove_file(&target).await;
            } else {
                return Ok(target);
            }
        } else {
            tracing::warn!(
                path = %target.display(),
                "Cached installer hash mismatch — refetching"
            );
            let _ = tokio::fs::remove_file(&target).await;
        }
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

    // Validate format BEFORE writing to disk so we never cache non-installer bytes.
    validate_artifact_for_kind(&source.kind, &body)?;

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

async fn read_header(path: &Path) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut f = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; 4096];
    let n = f.read(&mut buf).await?;
    buf.truncate(n);
    Ok(buf)
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

/// Result of probing an upstream installer URL for the admin UI: sha256, byte
/// count, and a best-effort format hint. The probe is non-destructive — it does
/// not persist anything — but it does run kind-specific magic-byte validation
/// for the caller's chosen `kind`, populating `format_ok` so the admin UI can
/// gate the "Register" button on the artifact actually being an installer.
#[derive(Debug, Clone)]
pub struct InstallerProbe {
    pub sha256: String,
    pub size_bytes: u64,
    pub format_hint: String,
    pub format_ok: bool,
    pub format_error: Option<String>,
}

/// Compute sha256 of a file you intend to register as a source and run the
/// kind-specific format check. The format check is advisory at probe time
/// (returned as `format_ok`) so the admin still sees the sha for diagnostic
/// purposes, but the caller can refuse to persist a source whose
/// `format_ok=false`.
pub async fn probe_url(url: &str, kind: &str) -> Result<InstallerProbe, InstallerError> {
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
    let sha256 = format!("{:x}", hasher.finalize());
    let format_hint = sniff_format(&body).to_string();
    let (format_ok, format_error) = match validate_artifact_for_kind(kind, &body) {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };
    Ok(InstallerProbe {
        sha256,
        size_bytes: body.len() as u64,
        format_hint,
        format_ok,
        format_error,
    })
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

#[cfg(test)]
mod tests {
    use super::{
        sniff_format, validate_artifact_for_kind, InstallerError, KIND_ORION_MSI, MSI_MAGIC,
    };

    fn msi_header() -> Vec<u8> {
        let mut b = Vec::with_capacity(64);
        b.extend_from_slice(&MSI_MAGIC);
        b.extend_from_slice(&[0u8; 56]);
        b
    }

    #[test]
    fn validator_accepts_msi_magic_for_orion_msi_kind() {
        assert!(validate_artifact_for_kind(KIND_ORION_MSI, &msi_header()).is_ok());
    }

    #[test]
    fn validator_rejects_zip_archive_for_orion_msi_kind() {
        // GitHub source-tarball signature — the operator pitfall this guards against.
        let zip = b"PK\x03\x04...rest of zip...";
        match validate_artifact_for_kind(KIND_ORION_MSI, zip) {
            Err(InstallerError::InvalidContent { looks_like, .. }) => {
                assert!(
                    looks_like.contains("ZIP"),
                    "hint should mention ZIP, got {looks_like}",
                );
            }
            other => panic!("expected InvalidContent error, got {other:?}"),
        }
    }

    #[test]
    fn validator_rejects_pe_executable_for_orion_msi_kind() {
        // .exe self-installer (NSIS / Inno) — also not an .msi
        let pe = b"MZ\x90\x00\x03\x00...PE header...";
        match validate_artifact_for_kind(KIND_ORION_MSI, pe) {
            Err(InstallerError::InvalidContent { looks_like, .. }) => {
                assert!(
                    looks_like.contains("PE") || looks_like.contains(".exe"),
                    "hint should mention PE/.exe, got {looks_like}",
                );
            }
            other => panic!("expected InvalidContent error, got {other:?}"),
        }
    }

    #[test]
    fn validator_rejects_html_error_page_for_orion_msi_kind() {
        let html = b"<!DOCTYPE html><html><head><title>Not Found</title></head></html>";
        match validate_artifact_for_kind(KIND_ORION_MSI, html) {
            Err(InstallerError::InvalidContent { looks_like, .. }) => {
                assert!(
                    looks_like.to_ascii_lowercase().contains("html"),
                    "hint should mention HTML, got {looks_like}",
                );
            }
            other => panic!("expected InvalidContent error, got {other:?}"),
        }
    }

    #[test]
    fn validator_rejects_truncated_input_for_orion_msi_kind() {
        let too_short = vec![0xD0, 0xCF, 0x11]; // first 3 bytes only
        assert!(validate_artifact_for_kind(KIND_ORION_MSI, &too_short).is_err());
    }

    #[test]
    fn validator_is_a_no_op_for_unknown_kinds() {
        // Forward-compatible: unknown kinds opt out of the magic check.
        assert!(validate_artifact_for_kind("future-kind", b"anything").is_ok());
    }

    #[test]
    fn sniff_format_recognises_common_signatures() {
        assert!(sniff_format(&msi_header()).contains("Windows Installer"));
        assert!(sniff_format(b"PK\x03\x04anything").contains("ZIP"));
        assert!(sniff_format(b"MZ\x90\x00").contains("PE"));
        assert!(sniff_format(b"%PDF-1.4").contains("PDF"));
        assert!(sniff_format(b"<!DOCTYPE html><html>")
            .to_ascii_lowercase()
            .contains("html"));
        assert_eq!(sniff_format(&[]), "empty body");
    }
}
