//! GET /api/agents/:id/bundle — packages a config.json + Tauri MSI installer for the user to
//! run on their workstation. The config.json carries the entity's identity token and the
//! anchor SAO URL. The MSI is read from one of three sources, in order:
//!
//!   1. The agent's pinned installer in the local cache
//!      (`SAO_DATA_DIR/installers/<sha>/<filename>`). If the cache file is missing, SAO
//!      re-fetches from the original `installer_sources` row by sha lookup.
//!   2. A freshly-pinned default installer source (covers the case where an agent was
//!      created before any installer source was registered).
//!   3. `SAO_ORION_INSTALLER_PATH` env var — the legacy mount-based fallback.

use std::io::{Cursor, Write};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

use crate::auth::agent_tokens;
use crate::auth::middleware::AuthUser;
use crate::db::agents::AgentRow;
use crate::security::RequestAuditContext;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/agents/:id/bundle", get(download_bundle))
}

const README_TEXT: &str = "OrionII entity bundle\n\
=====================\n\
\n\
This ZIP contains:\n\
  config.json            -- SAO base URL + entity identity token (the anchor).\n\
  OrionII-Setup.msi      -- Tauri installer (Windows).\n\
  README-FIRST-RUN.txt   -- this file.\n\
\n\
First run\n\
---------\n\
1. Install: double-click OrionII-Setup.msi.\n\
2. Drop config.json into:\n\
       %APPDATA%\\OrionII\\config.json\n\
   (the installer also accepts it co-located with OrionII.exe).\n\
3. Launch OrionII. The app will:\n\
     a. Read the SAO URL + entity token from config.json.\n\
     b. Call GET {sao_base_url}/api/orion/birth to dynamically receive the\n\
        live agent metadata, default provider/model, scopes, current policy,\n\
        and personality seed in a single response.\n\
     c. Adopt the SAO-assigned identity and start chatting.\n\
\n\
Because the runtime config is fetched live, changes the admin makes in SAO\n\
(switching provider, swapping default model, updating policy) take effect on\n\
the next OrionII launch -- no re-bundling required.\n\
\n\
Security\n\
--------\n\
The agent_token is a long-lived bearer credential. Treat config.json like an\n\
API key: do not share, do not commit. If lost, delete the agent in SAO and\n\
download a new bundle (downloading also revokes any prior tokens for the agent).\n";

async fn download_bundle(
    user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Extension(context): axum::extract::Extension<RequestAuditContext>,
    Path(id): Path<Uuid>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let agent = crate::db::agents::get_agent(&state.inner.db, id)
        .await
        .map_err(|e| internal(e.to_string()))?
        .ok_or_else(|| not_found("Agent not found"))?;

    if !user.is_admin() && agent.owner_user_id != Some(user.user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        ));
    }

    let provider = agent
        .default_provider
        .as_deref()
        .ok_or_else(|| bad_request("Agent has no default LLM provider configured"))?;
    let id_model = agent
        .default_id_model
        .as_deref()
        .ok_or_else(|| bad_request("Agent has no default Id model configured"))?;
    let ego_model = agent
        .default_ego_model
        .as_deref()
        .ok_or_else(|| bad_request("Agent has no default Ego model configured"))?;

    let provider_settings = crate::db::llm_providers::get(&state.inner.db, provider)
        .await
        .map_err(|e| internal(e.to_string()))?
        .ok_or_else(|| bad_request("Configured provider is not registered"))?;
    if !provider_settings.enabled {
        return Err(bad_request(
            "Configured provider is currently disabled by an administrator",
        ));
    }

    let installer_bytes = match resolve_installer(&state, &agent).await {
        Ok(bytes) => bytes,
        Err(message) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": message,
                    "hint": "Register an OrionII installer source under /admin/installer-sources, or set SAO_ORION_INSTALLER_PATH for the legacy mount-based flow.",
                })),
            ));
        }
    };

    // Revoke any prior tokens for this agent before issuing a new one — one live bundle per agent.
    let revoked = agent_tokens::revoke_for_agent(&state.inner.db, id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    let minted = agent_tokens::mint_entity_token(
        &state.inner.db,
        &state.inner.jwt_secret,
        agent.id,
        &agent.name,
        user.user_id,
    )
    .await
    .map_err(|e| internal(e.to_string()))?;

    let sao_base_url = std::env::var("SAO_PUBLIC_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3100".to_string());

    // Anchor-only: sao_base_url + agent_token are the minimum the entity needs to bootstrap.
    // Everything else (provider, models, policy, scopes, personality) comes from the live
    // GET /api/orion/birth call OrionII makes on every launch. The provider/model fields are
    // still embedded as a *fallback* for offline-from-SAO mode and for back-compat with
    // older clients that don't yet call birth.
    let config = json!({
        "sao_base_url": sao_base_url,
        "agent_id": agent.id,
        "agent_token": minted.jwt,
        "client_version_min": "0.1.0",
        "fallback": {
            "default_provider": provider,
            "default_id_model": id_model,
            "default_ego_model": ego_model,
        },
        "default_provider": provider,
        "default_id_model": id_model,
        "default_ego_model": ego_model,
    });

    let zip_bytes = build_zip(&config, &installer_bytes).map_err(|e| internal(e.to_string()))?;

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(user.user_id),
        Some(agent.id),
        "agents.bundle_downloaded",
        Some("agent"),
        Some(json!({
            "agent_id": agent.id,
            "token_jti": minted.jti,
            "token_expires_at": minted.expires_at,
            "revoked_prior_tokens": revoked,
            "request_id": context.request_id,
        })),
        context.client_ip.as_deref(),
        context.user_agent.as_deref(),
    )
    .await;

    let short_id = agent.id.to_string()[..8].to_string();
    let safe_name: String = agent
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let filename = format!("Orion-{}-{}.zip", safe_name, short_id);

    let mut response = Response::new(Body::from(zip_bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    Ok(response)
}

fn build_zip(config: &Value, installer: &[u8]) -> std::io::Result<Vec<u8>> {
    let buffer = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buffer);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let config_bytes = serde_json::to_vec_pretty(config)?;
    zip.start_file("config.json", opts)?;
    zip.write_all(&config_bytes)?;

    zip.start_file("OrionII-Setup.msi", opts)?;
    zip.write_all(installer)?;

    zip.start_file("README-FIRST-RUN.txt", opts)?;
    zip.write_all(README_TEXT.as_bytes())?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

fn internal(msg: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}

fn bad_request(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

fn not_found(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg })))
}

/// Resolve the bytes of the OrionII installer for `agent`, walking the three sources in order.
async fn resolve_installer(state: &AppState, agent: &AgentRow) -> Result<Vec<u8>, String> {
    // 1. Pinned source — agent already has installer coordinates.
    if let (Some(sha), Some(filename)) = (&agent.installer_sha256, &agent.installer_filename) {
        if let Some(path) = crate::installers::cached_path(sha, filename) {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                return Ok(bytes);
            }
        }
        // Cache miss — try to refetch from the installer_sources row that matches this sha.
        if let Ok(rows) = crate::db::installer_sources::list(&state.inner.db).await {
            if let Some(source) = rows
                .into_iter()
                .find(|r| r.expected_sha256.eq_ignore_ascii_case(sha))
            {
                match crate::installers::fetch_or_cache(&source).await {
                    Ok(path) => {
                        if let Ok(bytes) = tokio::fs::read(&path).await {
                            return Ok(bytes);
                        }
                    }
                    Err(e) => return Err(format!("Failed to refetch pinned installer: {e}")),
                }
            }
        }
    }

    // 2. Fall back to the current default — pin opportunistically so this agent stays stable
    //    on subsequent downloads even if the default rolls forward.
    if let Ok(Some(source)) =
        crate::db::installer_sources::get_default(&state.inner.db, "orion-msi").await
    {
        match crate::installers::fetch_or_cache(&source).await {
            Ok(path) => {
                let _ = crate::db::agents::set_installer_pin(
                    &state.inner.db,
                    agent.id,
                    &source.expected_sha256,
                    &source.filename,
                    &source.version,
                )
                .await;
                if let Ok(bytes) = tokio::fs::read(&path).await {
                    return Ok(bytes);
                }
            }
            Err(e) => return Err(format!("Failed to fetch default installer: {e}")),
        }
    }

    // 3. Legacy env-var fallback (file mounted into the container).
    if let Ok(path) = std::env::var("SAO_ORION_INSTALLER_PATH") {
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(bytes);
        }
    }

    Err("OrionII installer is not available. Register an installer source under /admin/installer-sources.".to_string())
}
