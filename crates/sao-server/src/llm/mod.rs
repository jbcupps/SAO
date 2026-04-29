//! SAO-hosted LLM proxy. OrionII entities call POST /api/llm/generate; SAO holds the keys
//! and forwards to the configured provider. Keys never leave the server, calls are auditable,
//! revocation of an entity token instantly cuts off model access.

use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use thiserror::Error;

use crate::state::AppState;

pub mod anthropic;
pub mod gemini;
pub mod grok;
pub mod keys;
pub mod ollama;
pub mod openai;

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateRequest {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub system: String,
    pub prompt: String,
    /// Optional sampling temperature. When `None` (the default), the upstream provider
    /// uses its own model default. Required for GPT-5.x and reasoning models that
    /// reject any non-default value — see `TEMPERATURE_LOCKED_MODEL_PREFIXES`.
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub role: String,
}

/// Model-name prefixes whose upstream APIs reject any non-default `temperature`.
/// Dispatch strips temperature for these models even if the client passes one,
/// so older OrionII bundles don't 400 on every Ego call. Match is case-insensitive
/// prefix-match against `req.model`.
const TEMPERATURE_LOCKED_MODEL_PREFIXES: &[&str] = &["gpt-5", "o3", "o4-mini", "o4."];

fn temperature_is_locked(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    TEMPERATURE_LOCKED_MODEL_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateResponse {
    pub text: String,
    pub model: String,
    pub latency_ms: u64,
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("provider {0} is not enabled")]
    ProviderDisabled(String),
    #[error("provider {0} is not configured (missing base_url or api key)")]
    ProviderUnconfigured(String),
    #[error("provider {0} has no approved models configured")]
    NoApprovedModels(String),
    #[error("model {model} is not on the approved list for {provider}")]
    ModelNotApproved { provider: String, model: String },
    #[error("vault is sealed; cannot read provider key")]
    VaultSealed,
    #[error("provider HTTP call failed: {0}")]
    Http(String),
    #[error("provider returned an unparseable response: {0}")]
    BadResponse(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("provider returned error status {status}: {body}")]
    ProviderError { status: u16, body: String },
}

pub fn describe_transport_error(target: &str, err: &reqwest::Error) -> String {
    let mut chain: Vec<String> = Vec::new();
    let mut current: Option<&(dyn StdError + 'static)> = Some(err);

    while let Some(source) = current {
        let message = source.to_string();
        if !message.trim().is_empty() && !chain.iter().any(|entry| entry == &message) {
            chain.push(message);
        }
        current = source.source();
    }

    let detail = if chain.is_empty() {
        err.to_string()
    } else {
        chain.join(": ")
    };
    let lower = detail.to_ascii_lowercase();

    if err.is_timeout() {
        return format!("timeout while calling {target}: {detail}");
    }

    if err.is_connect() || lower.contains("ssl") || lower.contains("tls") {
        return format!(
            "HTTPS connection to {target} failed. This usually means a TLS handshake or network-path problem between SAO and the provider, not a bad model selection. Details: {detail}"
        );
    }

    format!("transport error while calling {target}: {detail}")
}

pub async fn dispatch(
    state: &AppState,
    req: &GenerateRequest,
) -> Result<GenerateResponse, LlmError> {
    let settings = crate::db::llm_providers::get(&state.inner.db, &req.provider)
        .await?
        .ok_or_else(|| LlmError::ProviderUnconfigured(req.provider.clone()))?;

    if !settings.enabled {
        return Err(LlmError::ProviderDisabled(req.provider.clone()));
    }

    let approved: Vec<String> =
        serde_json::from_value(settings.approved_models.clone()).unwrap_or_default();
    if approved.is_empty() {
        return Err(LlmError::NoApprovedModels(req.provider.clone()));
    }

    if !approved.iter().any(|m| m == &req.model) {
        return Err(LlmError::ModelNotApproved {
            provider: req.provider.clone(),
            model: req.model.clone(),
        });
    }

    // Belt-and-braces: strip temperature for known-locked models even if a client
    // (e.g. an older OrionII bundle) sends one. Without this, every Ego call against
    // gpt-5* fails with `400 unsupported_value`.
    let effective_req = if temperature_is_locked(&req.model) && req.temperature.is_some() {
        let mut stripped = req.clone();
        stripped.temperature = None;
        std::borrow::Cow::Owned(stripped)
    } else {
        std::borrow::Cow::Borrowed(req)
    };
    let req = effective_req.as_ref();

    let started = std::time::Instant::now();
    let text = match req.provider.as_str() {
        "ollama" => {
            let base = settings
                .base_url
                .clone()
                .ok_or_else(|| LlmError::ProviderUnconfigured("ollama".into()))?;
            ollama::generate(&base, req).await?
        }
        "openai" => {
            let key = keys::get_api_key(state, "openai")
                .await?
                .ok_or_else(|| LlmError::ProviderUnconfigured("openai".into()))?;
            openai::generate(&key, req).await?
        }
        "anthropic" => {
            let key = keys::get_api_key(state, "anthropic")
                .await?
                .ok_or_else(|| LlmError::ProviderUnconfigured("anthropic".into()))?;
            anthropic::generate(&key, req).await?
        }
        "grok" => {
            let key = keys::get_api_key(state, "grok")
                .await?
                .ok_or_else(|| LlmError::ProviderUnconfigured("grok".into()))?;
            grok::generate(&key, req).await?
        }
        "gemini" => {
            let key = keys::get_api_key(state, "gemini")
                .await?
                .ok_or_else(|| LlmError::ProviderUnconfigured("gemini".into()))?;
            gemini::generate(&key, req).await?
        }
        other => {
            return Err(LlmError::ProviderUnconfigured(other.to_string()));
        }
    };
    let latency_ms = started.elapsed().as_millis() as u64;

    Ok(GenerateResponse {
        text,
        model: req.model.clone(),
        latency_ms,
    })
}
