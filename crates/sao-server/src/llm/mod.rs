//! SAO-hosted LLM proxy. OrionII entities call POST /api/llm/generate; SAO holds the keys
//! and forwards to the configured provider. Keys never leave the server, calls are auditable,
//! revocation of an entity token instantly cuts off model access.

use serde::{Deserialize, Serialize};
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
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub role: String,
}

fn default_temperature() -> f32 {
    0.2
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

    let approved: Vec<String> = serde_json::from_value(settings.approved_models.clone())
        .unwrap_or_default();
    if !approved.is_empty() && !approved.iter().any(|m| m == &req.model) {
        return Err(LlmError::ModelNotApproved {
            provider: req.provider.clone(),
            model: req.model.clone(),
        });
    }

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
