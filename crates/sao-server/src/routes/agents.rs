use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::security::RequestAuditContext;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/agents", get(list_agents))
        .route("/api/agents", post(create_agent))
        .route(
            "/api/agents/:id/delete",
            post(delete_agent_handler).delete(delete_agent_handler),
        )
        .route(
            "/api/agents/:id",
            get(get_agent_status)
                .put(update_agent_handler)
                .delete(delete_agent_handler)
                .post(delete_agent_handler),
        )
        .route("/api/agents/:id/events", get(list_agent_events))
}

#[derive(Debug)]
struct ValidatedAgentLlmDefaults {
    provider: Option<String>,
    id_model: Option<String>,
    ego_model: Option<String>,
}

fn trimmed_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn validate_agent_llm_defaults(
    state: &AppState,
    provider: Option<&str>,
    id_model: Option<&str>,
    ego_model: Option<&str>,
) -> Result<ValidatedAgentLlmDefaults, String> {
    let provider = trimmed_optional(provider);
    let id_model = trimmed_optional(id_model);
    let ego_model = trimmed_optional(ego_model);

    if provider.is_none() && id_model.is_none() && ego_model.is_none() {
        return Ok(ValidatedAgentLlmDefaults {
            provider: None,
            id_model: None,
            ego_model: None,
        });
    }

    let provider_name = provider
        .clone()
        .ok_or_else(|| "Choose an LLM provider".to_string())?;
    let id_model_name = id_model
        .clone()
        .ok_or_else(|| "Choose an Id model".to_string())?;
    let ego_model_name = ego_model
        .clone()
        .ok_or_else(|| "Choose an Ego model".to_string())?;

    let provider_entry =
        crate::db::llm_providers::get_enabled_catalog_entry(&state.inner.db, &provider_name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Provider {provider_name} is not enabled"))?;

    if provider_entry.approved_models.is_empty() {
        return Err(format!(
            "Provider {provider_name} has no approved models configured"
        ));
    }

    if provider_entry
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err(format!(
            "Provider {provider_name} has no default model configured"
        ));
    }

    if !provider_entry
        .approved_models
        .iter()
        .any(|model| model == &id_model_name)
    {
        return Err(format!(
            "Id model {id_model_name} is not approved for provider {provider_name}"
        ));
    }

    if !provider_entry
        .approved_models
        .iter()
        .any(|model| model == &ego_model_name)
    {
        return Err(format!(
            "Ego model {ego_model_name} is not approved for provider {provider_name}"
        ));
    }

    Ok(ValidatedAgentLlmDefaults {
        provider: Some(provider_name),
        id_model: Some(id_model_name),
        ego_model: Some(ego_model_name),
    })
}

async fn list_agents(user: AuthUser, State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let owner_filter = if user.is_admin() {
        None
    } else {
        Some(user.user_id)
    };
    match crate::db::agents::list_agents(&state.inner.db, owner_filter).await {
        Ok(agents) => (StatusCode::OK, Json(json!({ "agents": agents }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct CreateAgentRequest {
    name: String,
    #[serde(rename = "type", default)]
    agent_type: Option<String>,
    #[serde(default)]
    pubkey: Option<String>,
    #[serde(default)]
    default_provider: Option<String>,
    #[serde(default)]
    default_id_model: Option<String>,
    #[serde(default)]
    default_ego_model: Option<String>,
}

async fn create_agent(
    user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Extension(context): axum::extract::Extension<RequestAuditContext>,
    Json(req): Json<CreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let name = req.name.trim();
    if name.is_empty() || name.len() > 120 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Agent name must be between 1 and 120 characters" })),
        );
    }

    let defaults = match validate_agent_llm_defaults(
        &state,
        req.default_provider.as_deref(),
        req.default_id_model.as_deref(),
        req.default_ego_model.as_deref(),
    )
    .await
    {
        Ok(defaults) => defaults,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error })));
        }
    };

    let agent_id = Uuid::new_v4();
    let identity_agent_id = agent_id.to_string();
    let agent_dir = match state
        .inner
        .identity_manager
        .create_agent_with_id(&identity_agent_id, name)
    {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            );
        }
    };

    if let Err(e) = state.inner.identity_manager.create_birth_documents(
        &identity_agent_id,
        &agent_dir,
        name,
        req.agent_type.as_deref(),
        req.pubkey.as_deref(),
    ) {
        let _ = state
            .inner
            .identity_manager
            .remove_agent(&identity_agent_id);
        let _ = std::fs::remove_dir_all(&agent_dir);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        );
    }

    let created = match crate::db::agents::create_agent(
        &state.inner.db,
        agent_id,
        user.user_id,
        name,
        defaults.provider.as_deref(),
        defaults.id_model.as_deref(),
        defaults.ego_model.as_deref(),
    )
    .await
    {
        Ok(agent) => agent,
        Err(e) => {
            let _ = state
                .inner
                .identity_manager
                .remove_agent(&identity_agent_id);
            let _ = std::fs::remove_dir_all(&agent_dir);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    // Pull-on-create: if a default OrionII installer source is configured, fetch it
    // (cached by sha) and pin those coordinates to this agent. Bundle download serves the
    // pinned copy, so re-rolling the default later doesn't break old agents.
    let mut installer_status = serde_json::Value::Null;
    if let Ok(Some(source)) =
        crate::db::installer_sources::get_default(&state.inner.db, "orion-msi").await
    {
        match crate::installers::fetch_or_cache(&source).await {
            Ok(_path) => {
                let _ = crate::db::agents::set_installer_pin(
                    &state.inner.db,
                    created.id,
                    &source.expected_sha256,
                    &source.filename,
                    &source.version,
                )
                .await;
                installer_status = json!({
                    "pinned": true,
                    "version": source.version,
                    "sha256": source.expected_sha256,
                    "filename": source.filename,
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, agent_id = %created.id, "Failed to pin installer for new agent");
                installer_status = json!({ "pinned": false, "error": e.to_string() });
            }
        }
    }

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(user.user_id),
        None,
        "agents.create",
        Some("agent"),
        Some(json!({
            "name": name,
            "identity_agent_id": identity_agent_id,
            "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
            "installer": installer_status,
            "request_id": context.request_id,
        })),
        context.client_ip.as_deref(),
        context.user_agent.as_deref(),
    )
    .await;
    crate::db::audit::log_birth_event(&identity_agent_id);
    let ethic_preview = sao_core::ethical_bridge::get_triangleethic_preview(&identity_agent_id);
    (
        StatusCode::CREATED,
        Json(json!({
            "status": "READY",
            "agent_id": created.id,
            "identity_agent_id": identity_agent_id,
            "birth_status": created.birth_status,
            "birthed_at": created.birthed_at,
            "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
            "soul_immutable": true,
            "personality_preview": "default ego traits loaded - Superego will evolve this later",
            "triangleethic_preview": ethic_preview,
            "installer": installer_status,
        })),
    )
}

async fn get_agent_status(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    let agent = match crate::db::agents::get_agent(&state.inner.db, id).await {
        Ok(Some(agent)) => agent,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Agent not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };
    if !user.is_admin() && agent.owner_user_id != Some(user.user_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        );
    }

    let last_heartbeat = crate::db::agents::last_egress_at(&state.inner.db, id)
        .await
        .ok()
        .flatten();
    let available_llm_providers = crate::db::llm_providers::list_enabled_catalog(&state.inner.db)
        .await
        .unwrap_or_default();

    (
        StatusCode::OK,
        Json(json!({
            "agent_id": id,
            "status": "READY",
            "birth_status": agent.birth_status,
            "birthed_at": agent.birthed_at,
            "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
            "soul_immutable": true,
            "personality_preview": "ego traits (editable by Superego only)",
            "default_provider": agent.default_provider,
            "default_id_model": agent.default_id_model,
            "default_ego_model": agent.default_ego_model,
            "available_llm_providers": available_llm_providers,
            "last_heartbeat": last_heartbeat,
        })),
    )
}

#[derive(Deserialize)]
struct UpdateAgentRequest {
    #[serde(default)]
    default_provider: Option<String>,
    #[serde(default)]
    default_id_model: Option<String>,
    #[serde(default)]
    default_ego_model: Option<String>,
}

async fn update_agent_handler(
    user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Extension(context): axum::extract::Extension<RequestAuditContext>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match crate::db::agents::get_agent(&state.inner.db, id).await {
        Ok(Some(agent)) => agent,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Agent not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    if !user.is_admin() && existing.owner_user_id != Some(user.user_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        );
    }

    let defaults = match validate_agent_llm_defaults(
        &state,
        req.default_provider.as_deref(),
        req.default_id_model.as_deref(),
        req.default_ego_model.as_deref(),
    )
    .await
    {
        Ok(defaults) => defaults,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error })));
        }
    };

    let updated = match crate::db::agents::update_agent_llm_defaults(
        &state.inner.db,
        id,
        defaults.provider.as_deref(),
        defaults.id_model.as_deref(),
        defaults.ego_model.as_deref(),
    )
    .await
    {
        Ok(Some(agent)) => agent,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Agent not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    let last_heartbeat = crate::db::agents::last_egress_at(&state.inner.db, id)
        .await
        .ok()
        .flatten();
    let available_llm_providers = crate::db::llm_providers::list_enabled_catalog(&state.inner.db)
        .await
        .unwrap_or_default();

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(user.user_id),
        Some(id),
        "agents.update_llm_defaults",
        Some("agent"),
        Some(json!({
            "agent_id": id,
            "default_provider": updated.default_provider,
            "default_id_model": updated.default_id_model,
            "default_ego_model": updated.default_ego_model,
            "request_id": context.request_id,
        })),
        context.client_ip.as_deref(),
        context.user_agent.as_deref(),
    )
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "agent_id": id,
            "status": "READY",
            "birth_status": updated.birth_status,
            "birthed_at": updated.birthed_at,
            "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
            "soul_immutable": true,
            "personality_preview": "ego traits (editable by Superego only)",
            "default_provider": updated.default_provider,
            "default_id_model": updated.default_id_model,
            "default_ego_model": updated.default_ego_model,
            "available_llm_providers": available_llm_providers,
            "last_heartbeat": last_heartbeat,
        })),
    )
}

async fn archive_agent_snapshot(
    state: &AppState,
    agent: &crate::db::agents::AgentRow,
    created_by: Uuid,
    reason: &str,
) -> Result<crate::db::entity_archives::EntityArchiveRow, String> {
    let archive_id = Uuid::new_v4();
    let archive_root = state.inner.identity_manager.archive_root()?;
    let archive_dir = archive_root
        .join("entities")
        .join(agent.id.to_string())
        .join(archive_id.to_string());
    fs::create_dir_all(&archive_dir).map_err(|e| e.to_string())?;

    let identity_agent_id = match state
        .inner
        .identity_manager
        .copy_agent_identity_for_archive(
            &agent.id.to_string(),
            &agent.name,
            &archive_dir.join("identity"),
        ) {
        Ok(identity_agent_id) => identity_agent_id,
        Err(error) => {
            tracing::warn!(
                agent_id = %agent.id,
                error = %error,
                "Agent identity documents were not available during archive"
            );
            None
        }
    };

    let egress_events = crate::db::orion::list_all_for_agent(&state.inner.db, agent.id)
        .await
        .map_err(|e| e.to_string())?;
    let memory_events = egress_events
        .iter()
        .filter(|event| event.event_type == "memoryEvent")
        .collect::<Vec<_>>();

    write_json_file(
        &archive_dir.join("orion_egress_events.json"),
        &egress_events,
    )?;
    write_json_file(&archive_dir.join("memories.json"), &memory_events)?;

    let archived_at = Utc::now();
    let manifest = json!({
        "archive_id": archive_id,
        "agent_id": agent.id,
        "agent_name": agent.name.clone(),
        "owner_user_id": agent.owner_user_id,
        "created_by": created_by,
        "reason": reason,
        "archived_at": archived_at,
        "birth_status": agent.birth_status.clone(),
        "birthed_at": agent.birthed_at,
        "identity_agent_id": identity_agent_id,
        "identity_documents_copied": identity_agent_id.is_some(),
        "egress_event_count": egress_events.len(),
        "memory_event_count": memory_events.len(),
        "files": {
            "identity": "identity/",
            "egress_events": "orion_egress_events.json",
            "memories": "memories.json"
        }
    });
    write_json_file(&archive_dir.join("manifest.json"), &manifest)?;

    crate::db::entity_archives::insert_archive(
        &state.inner.db,
        archive_id,
        agent.id,
        &agent.name,
        agent.owner_user_id,
        Some(created_by),
        Some(reason),
        &archive_dir.to_string_lossy(),
        manifest,
        egress_events.len() as i32,
        memory_events.len() as i32,
    )
    .await
    .map_err(|e| e.to_string())
}

fn write_json_file<T: serde::Serialize>(path: &std::path::Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())
}

async fn delete_agent_handler(
    user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Extension(context): axum::extract::Extension<RequestAuditContext>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    let agent = match crate::db::agents::get_agent(&state.inner.db, id).await {
        Ok(Some(agent)) => agent,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Agent not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };
    if !user.is_admin() && agent.owner_user_id != Some(user.user_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        );
    }

    let archive = match archive_agent_snapshot(&state, &agent, user.user_id, "agent.delete").await {
        Ok(archive) => archive,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Agent archive failed; the active entity was not deleted",
                    "details": error,
                })),
            );
        }
    };

    let revoked_tokens = crate::auth::agent_tokens::revoke_for_agent(&state.inner.db, id)
        .await
        .unwrap_or(0);

    match crate::db::agents::delete_agent(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.user_id),
                None,
                "agents.delete",
                Some("agent"),
                Some(json!({
                    "agent_id": id,
                    "request_id": context.request_id,
                    "revoked_tokens": revoked_tokens,
                    "archive_id": archive.id,
                    "archive_path": archive.archive_path.clone(),
                    "memory_event_count": archive.memory_event_count,
                    "egress_event_count": archive.egress_event_count,
                })),
                context.client_ip.as_deref(),
                context.user_agent.as_deref(),
            )
            .await;
            let cleanup_identity_id = archive
                .manifest
                .get("identity_agent_id")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| id.to_string());
            let identity_dir = state
                .inner
                .identity_manager
                .agent_dir(&cleanup_identity_id)
                .ok();
            let _ = state
                .inner
                .identity_manager
                .remove_agent(&cleanup_identity_id);
            if let Some(identity_dir) = identity_dir {
                let _ = fs::remove_dir_all(identity_dir);
            }
            (
                StatusCode::OK,
                Json(json!({
                    "deleted": true,
                    "archived": true,
                    "archive_id": archive.id,
                    "archive_path": archive.archive_path,
                })),
            )
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Agent not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct EventsQuery {
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
}

async fn list_agent_events(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<EventsQuery>,
) -> (StatusCode, Json<Value>) {
    let agent = match crate::db::agents::get_agent(&state.inner.db, id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Agent not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };
    if !user.is_admin() && agent.owner_user_id != Some(user.user_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        );
    }

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);

    match crate::db::orion::list_for_agent(&state.inner.db, id, limit, offset).await {
        Ok(events) => (
            StatusCode::OK,
            Json(json!({
                "events": events,
                "limit": limit,
                "offset": offset,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}
