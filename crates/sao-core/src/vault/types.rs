use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Types of secrets stored in the vault.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SecretType {
    Ed25519,
    ApiKey,
    Gpg,
    OauthToken,
    Other,
}

impl SecretType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::ApiKey => "api_key",
            Self::Gpg => "gpg",
            Self::OauthToken => "oauth_token",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for SecretType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SecretType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "ed25519" => Self::Ed25519,
            "api_key" => Self::ApiKey,
            "gpg" => Self::Gpg,
            "oauth_token" => Self::OauthToken,
            _ => Self::Other,
        })
    }
}

/// A secret encrypted at rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedSecret {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// Metadata about a stored secret (no sensitive data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub id: String,
    pub secret_type: SecretType,
    pub label: String,
    pub provider: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
