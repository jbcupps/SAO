use chrono::{Duration, Utc};
use axum::http::HeaderMap;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use uuid::Uuid;

use crate::security::{
    append_set_cookie, build_cookie, build_expired_cookie, CookieConfig,
};

pub const ACCESS_COOKIE_NAME: &str = "sao_access_token";
pub const REFRESH_COOKIE_NAME: &str = "sao_refresh_token";
pub const ACCESS_TOKEN_TTL_MINUTES: i64 = 30;
pub const REFRESH_TOKEN_TTL_DAYS: i64 = 7;

/// JWT claims for session tokens.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub username: String,
    pub role: String,
    pub exp: i64,
    pub iat: i64,
}

/// Generate a random JWT secret if not provided via env.
pub fn jwt_secret() -> [u8; 32] {
    if let Ok(secret) = std::env::var("SAO_JWT_SECRET") {
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        return key;
    }

    if let Some(key) = load_or_create_local_jwt_secret() {
        return key;
    }

    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    tracing::warn!(
        "No SAO_JWT_SECRET set and no local secret could be persisted, using random key (sessions won't survive restarts)"
    );
    key
}

fn load_or_create_local_jwt_secret() -> Option<[u8; 32]> {
    let data_root = default_data_root();
    let secret_path = data_root.join("jwt_secret.bin");

    if let Ok(bytes) = std::fs::read(&secret_path) {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            tracing::info!(
                path = %secret_path.display(),
                "Loaded persisted local JWT signing key"
            );
            return Some(key);
        }

        tracing::warn!(
            path = %secret_path.display(),
            byte_len = bytes.len(),
            "Ignoring invalid persisted JWT signing key length"
        );
    }

    if let Err(error) = std::fs::create_dir_all(&data_root) {
        tracing::warn!(
            path = %data_root.display(),
            error = %error,
            "Failed to create SAO data directory for JWT signing key persistence"
        );
        return None;
    }

    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);

    if let Err(error) = std::fs::write(&secret_path, key) {
        tracing::warn!(
            path = %secret_path.display(),
            error = %error,
            "Failed to persist local JWT signing key"
        );
        return None;
    }

    tracing::info!(
        path = %secret_path.display(),
        "Persisted local JWT signing key"
    );
    Some(key)
}

fn default_data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SAO_DATA_DIR") {
        return PathBuf::from(dir);
    }

    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sao")
}

/// Create a JWT access token (30 minute expiry).
pub fn create_access_token(
    user_id: Uuid,
    username: &str,
    role: &str,
    secret: &[u8; 32],
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        exp: (now + Duration::minutes(ACCESS_TOKEN_TTL_MINUTES)).timestamp(),
        iat: now.timestamp(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
}

/// Validate a JWT access token and return the claims.
pub fn validate_token(
    token: &str,
    secret: &[u8; 32],
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

/// Generate a random refresh token string.
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

/// Hash a refresh token for storage.
pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, result)
}

pub fn refresh_token_expires_at() -> chrono::DateTime<chrono::Utc> {
    Utc::now() + Duration::days(REFRESH_TOKEN_TTL_DAYS)
}

pub fn append_session_cookies(
    headers: &mut HeaderMap,
    cookie_config: &CookieConfig,
    access_token: &str,
    refresh_token: &str,
) {
    append_set_cookie(
        headers,
        &build_cookie(
            ACCESS_COOKIE_NAME,
            access_token,
            true,
            Some(std::time::Duration::from_secs(
                (ACCESS_TOKEN_TTL_MINUTES * 60) as u64,
            )),
            cookie_config,
        ),
    );
    append_set_cookie(
        headers,
        &build_cookie(
            REFRESH_COOKIE_NAME,
            refresh_token,
            true,
            Some(std::time::Duration::from_secs(
                (REFRESH_TOKEN_TTL_DAYS * 24 * 60 * 60) as u64,
            )),
            cookie_config,
        ),
    );
}

pub fn append_cleared_session_cookies(headers: &mut HeaderMap, cookie_config: &CookieConfig) {
    append_set_cookie(headers, &build_expired_cookie(ACCESS_COOKIE_NAME, cookie_config));
    append_set_cookie(headers, &build_expired_cookie(REFRESH_COOKIE_NAME, cookie_config));
}
