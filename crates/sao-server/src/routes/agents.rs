use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
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
        .route("/api/agents/{id}", get(get_agent_status))
        .route("/api/agents/{id}", delete(delete_agent_handler))
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

    let (identity_agent_id, agent_dir) = match state.inner.identity_manager.create_agent(name)
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

    match crate::db::agents::create_agent(&state.inner.db, user.user_id, name).await {
        Ok(_agent) => {
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
                    "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
                    "soul_immutable": true,
                    "personality_preview": "default ego traits loaded - Superego will evolve this later",
                    "triangleethic_preview": ethic_preview,
                })),
            )
        }
        Err(e) => {
            let _ = state
                .inner
                .identity_manager
                .remove_agent(&identity_agent_id);
            let _ = std::fs::remove_dir_all(&agent_dir);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        }
    }
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

    (
        StatusCode::OK,
        Json(json!({
            "agent_id": id,
            "status": "READY",
            "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
            "soul_immutable": true,
            "personality_preview": "ego traits (editable by Superego only)",
            "last_heartbeat": "just now"
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

    match crate::db::agents::delete_agent(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.user_id),
                None,
                "agents.delete",
                Some("agent"),
                Some(json!({ "agent_id": id, "request_id": context.request_id })),
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
