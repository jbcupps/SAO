use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
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
                .delete(delete_agent_handler)
                .post(delete_agent_handler),
        )
        .route("/api/agents/:id/events", get(list_agent_events))
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

    let (identity_agent_id, agent_dir) = match state.inner.identity_manager.create_agent(name) {
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
        user.user_id,
        name,
        req.default_provider.as_deref(),
        req.default_id_model.as_deref(),
        req.default_ego_model.as_deref(),
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
    let ethic_preview =
        sao_core::ethical_bridge::get_triangleethic_preview(&identity_agent_id);
    (
        StatusCode::CREATED,
        Json(json!({
            "status": "READY",
            "agent_id": created.id,
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

    (
        StatusCode::OK,
        Json(json!({
            "agent_id": id,
            "status": "READY",
            "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
            "soul_immutable": true,
            "personality_preview": "ego traits (editable by Superego only)",
            "default_provider": agent.default_provider,
            "default_id_model": agent.default_id_model,
            "default_ego_model": agent.default_ego_model,
            "last_heartbeat": last_heartbeat,
        })),
    )
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
                })),
                context.client_ip.as_deref(),
                context.user_agent.as_deref(),
            )
            .await;
            (StatusCode::OK, Json(json!({ "deleted": true })))
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
