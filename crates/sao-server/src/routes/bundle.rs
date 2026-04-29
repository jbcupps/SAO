//! GET /api/agents/:id/bundle — packages a config.json + Tauri MSI installer for the user to
//! run on their workstation. The config.json carries the entity's identity token and the
//! anchor SAO URL derived from the request host. deployment.json carries non-secret support
//! metadata for the one-click installer flow. The MSI is read from one of three sources, in order:
//!
//!   1. The agent's pinned installer in the local cache
//!      (`SAO_DATA_DIR/installers/<sha>/<filename>`). If the cache file is missing, SAO
//!      re-fetches from the original `installer_sources` row by sha lookup.
//!   2. A freshly-pinned default installer source (covers the case where an agent was
//!      created before any installer source was registered).
//!   3. `SAO_ORION_INSTALLER_PATH` env var — the legacy mount-based fallback.

use std::io::{Cursor, Write};

use axum::Json;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Router,
};
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
  config.json            -- SAO base URL, entity token, and OrionII bus transport intent.\n\
  deployment.json        -- non-secret install manifest for support/debugging.\n\
  OrionII-Setup.msi      -- Tauri installer (Windows, includes local nats-server sidecar).\n\
  Install-OrionII.cmd    -- double-click installer launcher for Windows.\n\
  Install-OrionII.ps1    -- helper used by the launcher.\n\
  README-FIRST-RUN.txt   -- this file.\n\
\n\
First run\n\
---------\n\
1. Extract this ZIP.\n\
2. Double-click Install-OrionII.cmd.\n\
3. The launcher will copy config.json into %APPDATA%\\OrionII, run the MSI,\n\
   update any existing OrionII install, and start OrionII. No JSON copy/paste is required.\n\
   If someone accidentally double-clicks OrionII-Setup.msi directly from the\n\
   extracted bundle, the MSI also self-enrolls from the sibling config.json.\n\
4. OrionII will:\n\
     a. Read the SAO URL + entity token from config.json.\n\
     b. Call GET {sao_base_url}/api/orion/birth to dynamically receive the\n\
        live agent metadata, default provider/model, scopes, current policy,\n\
        and personality seed in a single response.\n\
     c. Start OrionII's entity-internal EventBus. This bundle requests the\n\
        packaged NATS JetStream transport for durable local topics; if the\n\
        broker cannot start, OrionII falls back to its in-memory bus and stays alive.\n\
     d. Adopt the SAO-assigned identity and start chatting.\n\
\n\
Because the runtime config is fetched live, changes the admin makes in SAO\n\
(switching provider, swapping default model, updating policy) take effect on\n\
the next OrionII launch -- no re-bundling required.\n\
\n\
Architecture note\n\
-----------------\n\
SAO does not participate in OrionII's internal bus. Mentor, Id, Ego, and local\n\
Superego run inside OrionII and communicate by topic there. SAO remains the\n\
external governance, birth, LLM-proxy, and sanitized-egress HTTP seam.\n\
\n\
Security\n\
--------\n\
The agent_token is a long-lived bearer credential. Treat config.json like an\n\
API key: do not share, do not commit. If lost, delete the agent in SAO and\n\
download a new bundle (downloading also revokes any prior tokens for the agent).\n";

const INSTALL_CMD: &str = "@echo off\r\n\
setlocal\r\n\
set SCRIPT_DIR=%~dp0\r\n\
powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%SCRIPT_DIR%Install-OrionII.ps1\"\r\n\
if errorlevel 1 (\r\n\
  echo.\r\n\
  echo OrionII installation did not complete. Press any key to close this window.\r\n\
  pause >nul\r\n\
  exit /b 1\r\n\
)\r\n";

const INSTALL_PS1: &str = r#"$ErrorActionPreference = "Stop"

$bundleDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$configSource = Join-Path $bundleDir "config.json"
$deploymentSource = Join-Path $bundleDir "deployment.json"
$msiPath = Join-Path $bundleDir "OrionII-Setup.msi"
$configDir = Join-Path $env:APPDATA "OrionII"
$configTarget = Join-Path $configDir "config.json"
$deploymentTarget = Join-Path $configDir "deployment.json"

if (-not (Test-Path $configSource)) {
    throw "This bundle is missing config.json. Download a fresh OrionII bundle from SAO."
}

if (-not (Test-Path $msiPath)) {
    throw "This bundle is missing OrionII-Setup.msi. Download a fresh OrionII bundle from SAO."
}

New-Item -ItemType Directory -Force -Path $configDir | Out-Null
Copy-Item -Force $configSource $configTarget
if (Test-Path $deploymentSource) {
    Copy-Item -Force $deploymentSource $deploymentTarget
}
Write-Host "Saved OrionII enrollment config to $configTarget"

if (Get-Process -Name "orionii" -ErrorAction SilentlyContinue) {
    Write-Host "Closing running OrionII before install/update..."
    Get-Process -Name "orionii" -ErrorAction SilentlyContinue | ForEach-Object {
        try {
            if ($_.MainWindowHandle -ne 0) {
                [void]$_.CloseMainWindow()
            }
        }
        catch {
            # Best effort; force-stop below if the window did not close.
        }
    }
    Start-Sleep -Seconds 2
    Get-Process -Name "orionii" -ErrorAction SilentlyContinue | Stop-Process -Force
}

$uninstallRoots = @(
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*"
)
$existingProducts = @(Get-ItemProperty $uninstallRoots -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -eq "OrionII" -and $_.PSChildName -match "^\{[0-9A-Fa-f-]+\}$" })

foreach ($product in $existingProducts) {
    $version = if ($product.DisplayVersion) { $product.DisplayVersion } else { "unknown version" }
    Write-Host "Removing existing OrionII $version before install/update..."
    $remove = Start-Process -FilePath "msiexec.exe" `
        -ArgumentList @("/x", $product.PSChildName, "/passive", "/norestart") `
        -Wait `
        -PassThru
    if ($remove.ExitCode -notin @(0, 3010, 1605)) {
        throw "Existing OrionII uninstall exited with code $($remove.ExitCode)."
    }
}

Write-Host "Starting OrionII installer..."
$installArgs = @("/i", "`"$msiPath`"", "/passive", "/norestart")
$install = Start-Process -FilePath "msiexec.exe" -ArgumentList $installArgs -Wait -PassThru
if ($install.ExitCode -notin @(0, 3010)) {
    throw "OrionII installer exited with code $($install.ExitCode)."
}
if ($install.ExitCode -eq 3010) {
    Write-Host "OrionII installed; Windows reported that a restart may be required."
}

$programRoots = @($env:ProgramFiles, ${env:ProgramFiles(x86)}) |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
$candidates = $programRoots |
    ForEach-Object { Join-Path $_ "OrionII\orionii.exe" } |
    Where-Object { Test-Path $_ }

if ($candidates.Count -gt 0) {
    Write-Host "Launching OrionII..."
    Start-Process -FilePath $candidates[0]
}
else {
    Write-Host "OrionII installed. Launch it from the Start menu."
}
"#;

async fn download_bundle(
    user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Extension(context): axum::extract::Extension<RequestAuditContext>,
    headers: HeaderMap,
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

    let sao_base_url = public_base_url(&headers);
    let bus_transport = json!({
        "kind": "nats_jetstream",
        "port": 4222,
    });
    let available_llm_providers = crate::db::llm_providers::list_enabled_catalog(&state.inner.db)
        .await
        .map_err(|e| internal(e.to_string()))?;

    // Anchor-only: sao_base_url + agent_token are the minimum the entity needs to bootstrap.
    // Everything else (provider, models, policy, scopes, personality) comes from the live
    // GET /api/orion/birth call OrionII makes on every launch. The provider/model fields are
    // still embedded as a *fallback* for offline-from-SAO mode and for back-compat with
    // older clients that don't yet call birth. bus_transport is OrionII-local intent only:
    // SAO stays outside the entity bus and continues to interact over HTTP.
    let config = json!({
        "sao_base_url": sao_base_url,
        "agent_id": agent.id,
        "agent_name": agent.name.clone(),
        "agent_token": minted.jwt,
        "client_version_min": "0.1.0",
        "bus_transport": bus_transport,
        "fallback": {
            "default_provider": provider,
            "default_id_model": id_model,
            "default_ego_model": ego_model,
        },
        "available_llm_providers": available_llm_providers,
        "default_provider": provider,
        "default_id_model": id_model,
        "default_ego_model": ego_model,
    });
    let deployment = json!({
        "kind": "orionii.sao.deployment",
        "schema_version": 1,
        "downloaded_from": sao_base_url,
        "agent_id": agent.id,
        "agent_name": agent.name.clone(),
        "installer": {
            "file": "OrionII-Setup.msi",
            "launcher": "Install-OrionII.cmd",
        },
        "runtime_config": {
            "file": "config.json",
            "target": "%APPDATA%\\OrionII\\config.json",
            "contains_secret": true,
        },
        "bus_transport": bus_transport,
        "sao_http_seam": {
            "birth": "/api/orion/birth",
            "llm": "/api/llm/generate",
            "policy": "/api/orion/policy",
            "egress": "/api/orion/egress",
        },
        "architecture": {
            "orionii_hosts_entity_bus": true,
            "sao_participates_in_entity_bus": false,
            "note": "SAO remains external to OrionII's Mentor/Id/Ego/local Superego bus; the seam is HTTP only.",
        }
    });

    let zip_bytes =
        build_zip(&config, &deployment, &installer_bytes).map_err(|e| internal(e.to_string()))?;

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
            "orion_bus_transport": "nats_jetstream",
            "sao_base_url": sao_base_url,
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

fn build_zip(config: &Value, deployment: &Value, installer: &[u8]) -> std::io::Result<Vec<u8>> {
    let buffer = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buffer);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let config_bytes = serde_json::to_vec_pretty(config)?;
    zip.start_file("config.json", opts)?;
    zip.write_all(&config_bytes)?;

    let deployment_bytes = serde_json::to_vec_pretty(deployment)?;
    zip.start_file("deployment.json", opts)?;
    zip.write_all(&deployment_bytes)?;

    zip.start_file("OrionII-Setup.msi", opts)?;
    zip.write_all(installer)?;

    zip.start_file("Install-OrionII.cmd", opts)?;
    zip.write_all(INSTALL_CMD.as_bytes())?;

    zip.start_file("Install-OrionII.ps1", opts)?;
    zip.write_all(INSTALL_PS1.as_bytes())?;

    zip.start_file("README-FIRST-RUN.txt", opts)?;
    zip.write_all(README_TEXT.as_bytes())?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

fn public_base_url(headers: &HeaderMap) -> String {
    request_base_url(headers)
        .or_else(|| {
            std::env::var("SAO_PUBLIC_BASE_URL")
                .ok()
                .map(|value| value.trim().trim_end_matches('/').to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "http://localhost:3100".to_string())
}

fn request_base_url(headers: &HeaderMap) -> Option<String> {
    let host =
        header_value(headers, "x-forwarded-host").or_else(|| header_value(headers, "host"))?;
    let host = host.split(',').next()?.trim().trim_end_matches('/');
    if host.is_empty() {
        return None;
    }

    let proto = header_value(headers, "x-forwarded-proto")
        .and_then(|value| value.split(',').next().map(str::trim).map(str::to_string))
        .filter(|value| matches!(value.as_str(), "http" | "https"))
        .unwrap_or_else(|| "http".to_string());

    Some(format!("{proto}://{host}"))
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
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
