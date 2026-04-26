use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(dead_code)] // updated_by is captured for audit rather than read directly
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct LlmProviderSettingsRow {
    pub provider: String,
    pub enabled: bool,
    pub base_url: Option<String>,
    pub approved_models: serde_json::Value,
    pub default_model: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub updated_by: Option<Uuid>,
}

pub async fn list(pool: &PgPool) -> Result<Vec<LlmProviderSettingsRow>, sqlx::Error> {
    sqlx::query_as::<_, LlmProviderSettingsRow>(
        "SELECT provider, enabled, base_url, approved_models, default_model, updated_at, updated_by \
         FROM llm_provider_settings ORDER BY provider",
    )
    .fetch_all(pool)
    .await
}

pub async fn get(
    pool: &PgPool,
    provider: &str,
) -> Result<Option<LlmProviderSettingsRow>, sqlx::Error> {
    sqlx::query_as::<_, LlmProviderSettingsRow>(
        "SELECT provider, enabled, base_url, approved_models, default_model, updated_at, updated_by \
         FROM llm_provider_settings WHERE provider = $1",
    )
    .bind(provider)
    .fetch_optional(pool)
    .await
}

pub async fn upsert(
    pool: &PgPool,
    provider: &str,
    enabled: bool,
    base_url: Option<&str>,
    approved_models: &serde_json::Value,
    default_model: Option<&str>,
    updated_by: Uuid,
) -> Result<LlmProviderSettingsRow, sqlx::Error> {
    sqlx::query_as::<_, LlmProviderSettingsRow>(
        "INSERT INTO llm_provider_settings (provider, enabled, base_url, approved_models, default_model, updated_at, updated_by) \
         VALUES ($1, $2, $3, $4, $5, now(), $6) \
         ON CONFLICT (provider) DO UPDATE SET \
            enabled = EXCLUDED.enabled, \
            base_url = EXCLUDED.base_url, \
            approved_models = EXCLUDED.approved_models, \
            default_model = EXCLUDED.default_model, \
            updated_at = now(), \
            updated_by = EXCLUDED.updated_by \
         RETURNING provider, enabled, base_url, approved_models, default_model, updated_at, updated_by",
    )
    .bind(provider)
    .bind(enabled)
    .bind(base_url)
    .bind(approved_models)
    .bind(default_model)
    .bind(updated_by)
    .fetch_one(pool)
    .await
}
