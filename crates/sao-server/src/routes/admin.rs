use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AdminUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // User management (admin only)
        .route("/api/admin/users", get(list_users))
        .route("/api/admin/users/:id/role", put(update_user_role))
        .route("/api/admin/users/:id", delete(delete_user))
        // OIDC provider management (admin only)
        .route("/api/admin/oidc/providers", post(create_oidc_provider))
        .route("/api/admin/oidc/providers", get(list_oidc_providers))
        .route("/api/admin/oidc/providers/:id", put(update_oidc_provider))
        .route(
            "/api/admin/oidc/providers/:id",
            delete(delete_oidc_provider),
        )
        // SAO admin entity overview (admin only)
        .route("/api/admin/admin-entity", get(get_admin_entity_overview))
        // Audit log (admin only)
        .route("/api/admin/audit", get(query_audit_log))
        // LLM provider configuration (admin only)
        .route("/api/admin/llm-providers", get(list_llm_providers))
        .route("/api/admin/llm-providers/:provider", put(update_llm_provider))
        .route(
            "/api/admin/llm-providers/ollama/probe",
            post(probe_ollama_models),
        )
        .route(
            "/api/admin/llm-providers/:provider/test",
            post(test_llm_provider),
        )
        // OrionII installer source registry (admin only)
        .route("/api/admin/installer-sources", get(list_installer_sources))
        .route("/api/admin/installer-sources", post(create_installer_source))
        .route(
            "/api/admin/installer-sources/probe",
            post(probe_installer_source),
        )
        .route(
            "/api/admin/installer-sources/:id",
            delete(delete_installer_source),
        )
        .route(
            "/api/admin/installer-sources/:id/set-default",
            post(set_default_installer_source),
        )
}

const SUPPORTED_PROVIDERS: &[&str] =
    &["openai", "anthropic", "ollama", "grok", "gemini"];
const KEY_BEARING_PROVIDERS: &[&str] = &["openai", "anthropic", "grok", "gemini"];

// --- User management ---

async fn list_users(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    match crate::db::users::list_users(&state.inner.db).await {
        Ok(users) => (StatusCode::OK, Json(json!({ "users": users }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

async fn update_user_role(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRoleRequest>,
) -> (StatusCode, Json<Value>) {
    if req.role != "user" && req.role != "admin" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Role must be 'user' or 'admin'" })),
        );
    }

    match crate::db::users::update_user_role(&state.inner.db, id, &req.role).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.update_role",
                Some("user"),
                Some(json!({ "target_user_id": id, "new_role": req.role })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "updated": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn delete_user(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    // Prevent self-deletion
    if id == admin.user_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Cannot delete your own account" })),
        );
    }

    match crate::db::users::delete_user(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.delete_user",
                Some("user"),
                Some(json!({ "deleted_user_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "deleted": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

// --- OIDC provider management ---

#[derive(Deserialize)]
struct CreateOidcProviderRequest {
    name: String,
    issuer_url: String,
    client_id: String,
    client_secret: Option<String>,
    scopes: Option<String>,
}

async fn create_oidc_provider(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<CreateOidcProviderRequest>,
) -> (StatusCode, Json<Value>) {
    // Encrypt client secret if provided
    let encrypted_secret = if let Some(ref secret) = req.client_secret {
        let vs = state.inner.vault_state.read().await;
        match vs.vmk() {
            Some(vmk) => match vmk.encrypt(secret.as_bytes()) {
                Ok((ct, nonce)) => {
                    let mut combined = ct;
                    combined.extend_from_slice(&nonce);
                    Some(combined)
                }
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("Failed to encrypt secret: {}", e) })),
                    );
                }
            },
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "Vault is sealed" })),
                );
            }
        }
    } else {
        None
    };

    let scopes = req.scopes.as_deref().unwrap_or("openid profile email");

    match crate::db::oidc::create_provider(
        &state.inner.db,
        &req.name,
        &req.issuer_url,
        &req.client_id,
        encrypted_secret.as_deref(),
        scopes,
    )
    .await
    {
        Ok(id) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.create_oidc_provider",
                Some("oidc_provider"),
                Some(json!({ "provider_name": req.name, "provider_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::CREATED, Json(json!({ "id": id })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn list_oidc_providers(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    match crate::db::oidc::list_providers(&state.inner.db).await {
        Ok(providers) => {
            // Strip encrypted secrets from response
            let sanitized: Vec<Value> = providers
                .iter()
                .map(|p| {
                    json!({
                        "id": p.id,
                        "name": p.name,
                        "issuer_url": p.issuer_url,
                        "client_id": p.client_id,
                        "has_client_secret": p.client_secret_encrypted.is_some(),
                        "scopes": p.scopes,
                        "enabled": p.enabled,
                        "created_at": p.created_at,
                        "updated_at": p.updated_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "providers": sanitized })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateOidcProviderRequest {
    name: Option<String>,
    issuer_url: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    scopes: Option<String>,
    enabled: Option<bool>,
}

async fn update_oidc_provider(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateOidcProviderRequest>,
) -> (StatusCode, Json<Value>) {
    let encrypted_secret = if let Some(ref secret) = req.client_secret {
        let vs = state.inner.vault_state.read().await;
        match vs.vmk() {
            Some(vmk) => match vmk.encrypt(secret.as_bytes()) {
                Ok((ct, nonce)) => {
                    let mut combined = ct;
                    combined.extend_from_slice(&nonce);
                    Some(combined)
                }
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("Failed to encrypt secret: {}", e) })),
                    );
                }
            },
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "Vault is sealed" })),
                );
            }
        }
    } else {
        None
    };

    match crate::db::oidc::update_provider(
        &state.inner.db,
        id,
        req.name.as_deref(),
        req.issuer_url.as_deref(),
        req.client_id.as_deref(),
        encrypted_secret.as_deref(),
        req.scopes.as_deref(),
        req.enabled,
    )
    .await
    {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.update_oidc_provider",
                Some("oidc_provider"),
                Some(json!({ "provider_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "updated": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Provider not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn delete_oidc_provider(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::oidc::delete_provider(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.delete_oidc_provider",
                Some("oidc_provider"),
                Some(json!({ "provider_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "deleted": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Provider not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

// --- Audit log ---

#[derive(Deserialize)]
struct AuditLogQuery {
    user_id: Option<Uuid>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn query_audit_log(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
    Query(params): Query<AuditLogQuery>,
) -> (StatusCode, Json<Value>) {
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);

    match crate::db::admin::query_audit_log(&state.inner.db, params.user_id, limit, offset).await {
        Ok(entries) => (StatusCode::OK, Json(json!({ "audit_log": entries }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn get_admin_entity_overview(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    match crate::db::admin_entity::get_admin_entity_overview(&state.inner.db).await {
        Ok(Some(overview)) => (StatusCode::OK, Json(json!(overview))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "SAO admin entity not configured" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

// --- LLM provider configuration ---

async fn list_llm_providers(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    let rows = match crate::db::llm_providers::list(&state.inner.db).await {
        Ok(rows) => rows,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    // Surface "key present" without revealing the key itself.
    let with_key_status: Vec<Value> = {
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let has_api_key = matches!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*)::bigint FROM vault_secrets \
                     WHERE provider = $1 AND label = 'api_key' AND secret_type = 'api_key'",
                )
                .bind(&row.provider)
                .fetch_one(&state.inner.db)
                .await,
                Ok(count) if count > 0
            );
            out.push(json!({
                "provider": row.provider,
                "enabled": row.enabled,
                "base_url": row.base_url,
                "approved_models": row.approved_models,
                "default_model": row.default_model,
                "updated_at": row.updated_at,
                "has_api_key": has_api_key,
            }));
        }
        out
    };

    (
        StatusCode::OK,
        Json(json!({ "providers": with_key_status })),
    )
}

#[derive(Deserialize)]
struct UpdateLlmProviderRequest {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    approved_models: Option<serde_json::Value>,
    #[serde(default)]
    default_model: Option<String>,
    /// If present, replaces the API key in the vault. Never returned by GET.
    #[serde(default)]
    api_key: Option<String>,
}

async fn update_llm_provider(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Json(req): Json<UpdateLlmProviderRequest>,
) -> (StatusCode, Json<Value>) {
    if !SUPPORTED_PROVIDERS.contains(&provider.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Unknown provider" })),
        );
    }

    if provider == "ollama" && req.enabled && req.base_url.as_deref().unwrap_or("").trim().is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Ollama requires a base_url when enabled" })),
        );
    }

    // If an API key was supplied, store it in the vault under (provider, label='api_key').
    if let Some(api_key) = req.api_key.as_deref() {
        if !KEY_BEARING_PROVIDERS.contains(&provider.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("{} does not require an api_key", provider) })),
            );
        }
        let vs = state.inner.vault_state.read().await;
        let Some(vmk) = vs.vmk() else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "Vault is sealed" })),
            );
        };
        let (ciphertext, nonce) = match vmk.encrypt(api_key.trim().as_bytes()) {
            Ok(out) => out,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Encryption failed: {}", e) })),
                );
            }
        };
        // Replace any previous key for this provider.
        let _ = sqlx::query(
            "DELETE FROM vault_secrets \
             WHERE provider = $1 AND label = 'api_key' AND secret_type = 'api_key'",
        )
        .bind(&provider)
        .execute(&state.inner.db)
        .await;
        if let Err(e) = crate::db::vault::create_secret(
            &state.inner.db,
            None,
            "api_key",
            "api_key",
            Some(&provider),
            &ciphertext,
            &nonce,
            Some(json!({ "kind": "llm_provider_api_key" })),
        )
        .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    }

    let approved_models = req
        .approved_models
        .clone()
        .unwrap_or_else(|| json!([]));

    let saved = match crate::db::llm_providers::upsert(
        &state.inner.db,
        &provider,
        req.enabled,
        req.base_url.as_deref(),
        &approved_models,
        req.default_model.as_deref(),
        admin.user_id,
    )
    .await
    {
        Ok(row) => row,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(admin.user_id),
        None,
        "admin.llm_providers.update",
        Some("llm_provider"),
        Some(json!({
            "provider": provider,
            "enabled": req.enabled,
            "default_model": req.default_model,
            "set_api_key": req.api_key.is_some(),
        })),
        None,
        None,
    )
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "provider": saved.provider,
            "enabled": saved.enabled,
            "base_url": saved.base_url,
            "approved_models": saved.approved_models,
            "default_model": saved.default_model,
            "updated_at": saved.updated_at,
        })),
    )
}

#[derive(Deserialize)]
struct ProbeOllamaRequest {
    base_url: String,
}

async fn probe_ollama_models(
    AdminUser(_admin): AdminUser,
    Json(req): Json<ProbeOllamaRequest>,
) -> (StatusCode, Json<Value>) {
    if req.base_url.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "base_url is required" })),
        );
    }

    match crate::llm::ollama::list_models(req.base_url.trim()).await {
        Ok(models) => (StatusCode::OK, Json(json!({ "models": models }))),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize, Default)]
struct TestProviderRequest {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
}

async fn test_llm_provider(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Json(req): Json<TestProviderRequest>,
) -> (StatusCode, Json<Value>) {
    if !SUPPORTED_PROVIDERS.contains(&provider.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Unknown provider" })),
        );
    }

    let settings = match crate::db::llm_providers::get(&state.inner.db, &provider).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Provider is not registered" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    let model = req
        .model
        .clone()
        .or_else(|| settings.default_model.clone())
        .unwrap_or_default();
    if model.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "No model specified and no default_model set" })),
        );
    }

    // Bypass approved-model gate on test calls so admins can probe new models before saving them.
    let test_req = crate::llm::GenerateRequest {
        provider: provider.clone(),
        model: model.clone(),
        system: "You are a connectivity test for SAO. Reply briefly.".to_string(),
        prompt: req
            .prompt
            .clone()
            .unwrap_or_else(|| "ping".to_string()),
        temperature: 0.0,
        role: "test".to_string(),
    };

    let started = std::time::Instant::now();
    let result = match provider.as_str() {
        "ollama" => match settings.base_url.clone() {
            Some(url) => crate::llm::ollama::generate(&url, &test_req).await,
            None => Err(crate::llm::LlmError::ProviderUnconfigured("ollama".into())),
        },
        "openai" => match crate::llm::keys::get_api_key(&state, "openai").await {
            Ok(Some(key)) => crate::llm::openai::generate(&key, &test_req).await,
            Ok(None) => Err(crate::llm::LlmError::ProviderUnconfigured("openai".into())),
            Err(e) => Err(e),
        },
        "anthropic" => match crate::llm::keys::get_api_key(&state, "anthropic").await {
            Ok(Some(key)) => crate::llm::anthropic::generate(&key, &test_req).await,
            Ok(None) => Err(crate::llm::LlmError::ProviderUnconfigured("anthropic".into())),
            Err(e) => Err(e),
        },
        "grok" => match crate::llm::keys::get_api_key(&state, "grok").await {
            Ok(Some(key)) => crate::llm::grok::generate(&key, &test_req).await,
            Ok(None) => Err(crate::llm::LlmError::ProviderUnconfigured("grok".into())),
            Err(e) => Err(e),
        },
        "gemini" => match crate::llm::keys::get_api_key(&state, "gemini").await {
            Ok(Some(key)) => crate::llm::gemini::generate(&key, &test_req).await,
            Ok(None) => Err(crate::llm::LlmError::ProviderUnconfigured("gemini".into())),
            Err(e) => Err(e),
        },
        _ => unreachable!("provider validated above"),
    };
    let latency_ms = started.elapsed().as_millis() as u64;

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(admin.user_id),
        None,
        "admin.llm_providers.test",
        Some("llm_provider"),
        Some(json!({
            "provider": provider,
            "model": model,
            "ok": result.is_ok(),
            "latency_ms": latency_ms,
        })),
        None,
        None,
    )
    .await;

    match result {
        Ok(text) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "provider": provider,
                "model": model,
                "latency_ms": latency_ms,
                "preview": text.chars().take(240).collect::<String>(),
            })),
        ),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "ok": false,
                "provider": provider,
                "model": model,
                "latency_ms": latency_ms,
                "error": e.to_string(),
            })),
        ),
    }
}

// --- OrionII installer source registry ---

async fn list_installer_sources(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    match crate::db::installer_sources::list(&state.inner.db).await {
        Ok(rows) => (StatusCode::OK, Json(json!({ "sources": rows }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct ProbeInstallerRequest {
    url: String,
}

/// Compute the sha256 of an upstream URL without persisting anything. Lets the admin
/// confirm the hash before they commit it as `expected_sha256`.
async fn probe_installer_source(
    AdminUser(_admin): AdminUser,
    Json(req): Json<ProbeInstallerRequest>,
) -> (StatusCode, Json<Value>) {
    if req.url.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "url is required" })),
        );
    }
    match crate::installers::sha256_of_url(req.url.trim()).await {
        Ok(sha) => (
            StatusCode::OK,
            Json(json!({ "url": req.url, "sha256": sha })),
        ),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct CreateInstallerRequest {
    #[serde(default = "default_kind")]
    kind: String,
    url: String,
    filename: String,
    version: String,
    expected_sha256: String,
    #[serde(default)]
    is_default: bool,
}

fn default_kind() -> String {
    "orion-msi".to_string()
}

async fn create_installer_source(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<CreateInstallerRequest>,
) -> (StatusCode, Json<Value>) {
    if req.kind != "orion-msi" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Only kind=orion-msi is supported today" })),
        );
    }
    for (name, value) in [
        ("url", &req.url),
        ("filename", &req.filename),
        ("version", &req.version),
        ("expected_sha256", &req.expected_sha256),
    ] {
        if value.trim().is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("{name} is required") })),
            );
        }
    }
    if req.expected_sha256.trim().len() != 64
        || !req.expected_sha256.chars().all(|c| c.is_ascii_hexdigit())
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "expected_sha256 must be a 64-char hex digest" })),
        );
    }

    let row = match crate::db::installer_sources::insert(
        &state.inner.db,
        &req.kind,
        req.url.trim(),
        req.filename.trim(),
        req.version.trim(),
        &req.expected_sha256.trim().to_lowercase(),
        req.is_default,
        admin.user_id,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    // Pre-warm the cache so the next bundle download is instant. Failure here is non-fatal.
    let pre_warm = match crate::installers::fetch_or_cache(&row).await {
        Ok(path) => json!({ "ok": true, "cache_path": path.display().to_string() }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    };

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(admin.user_id),
        None,
        "admin.installer_sources.create",
        Some("installer_source"),
        Some(json!({
            "id": row.id,
            "kind": row.kind,
            "version": row.version,
            "is_default": row.is_default,
            "pre_warm": pre_warm,
        })),
        None,
        None,
    )
    .await;

    (
        StatusCode::CREATED,
        Json(json!({ "source": row, "pre_warm": pre_warm })),
    )
}

async fn delete_installer_source(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::installer_sources::delete(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.installer_sources.delete",
                Some("installer_source"),
                Some(json!({ "id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "deleted": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Installer source not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn set_default_installer_source(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::installer_sources::set_default(&state.inner.db, id).await {
        Ok(Some(row)) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.installer_sources.set_default",
                Some("installer_source"),
                Some(json!({ "id": row.id, "kind": row.kind, "version": row.version })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "source": row })))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Installer source not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}
