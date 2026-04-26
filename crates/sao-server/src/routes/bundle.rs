//! GET /api/agents/:id/bundle — packages a config.json + Tauri MSI installer for the user to
//! run on their workstation. The config.json carries the entity's identity token and chosen
//! LLM defaults. The MSI is read from `SAO_ORION_INSTALLER_PATH`.

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
use crate::security::RequestAuditContext;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/agents/:id/bundle", get(download_bundle))
}

const README_TEXT: &str = "OrionII entity bundle\n\
=====================\n\
\n\
This ZIP contains:\n\
  config.json            -- Identity token + LLM provider defaults for this entity.\n\
  OrionII-Setup.msi      -- Tauri installer (Windows).\n\
  README-FIRST-RUN.txt   -- this file.\n\
\n\
First run\n\
---------\n\
1. Install: double-click OrionII-Setup.msi.\n\
2. Drop config.json into:\n\
       %APPDATA%\\OrionII\\config.json\n\
   (the installer also accepts it co-located with OrionII.exe).\n\
3. Launch OrionII. It will adopt the agent_id from config and phone home to SAO.\n\
\n\
Security\n\
--------\n\
The agent_token is a long-lived bearer credential. Treat config.json like an API key:\n\
do not share, do not commit. If lost, delete the agent in SAO and download a new bundle.\n";

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

    let installer_path = std::env::var("SAO_ORION_INSTALLER_PATH").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "OrionII installer is not staged on this SAO instance",
                "hint": "Set SAO_ORION_INSTALLER_PATH to a built .msi (run npm run tauri build in OrionII)",
            })),
        )
    })?;
    let installer_bytes = tokio::fs::read(&installer_path).await.map_err(|e| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": format!("Failed to read installer at {installer_path}: {e}"),
            })),
        )
    })?;

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

    let config = json!({
        "sao_base_url": sao_base_url,
        "agent_id": agent.id,
        "agent_token": minted.jwt,
        "default_provider": provider,
        "default_id_model": id_model,
        "default_ego_model": ego_model,
        "client_version_min": "0.1.0",
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
