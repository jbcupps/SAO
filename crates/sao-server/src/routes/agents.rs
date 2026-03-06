use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/agents", get(list_agents))
        .route("/api/agents", post(create_agent))
        .route("/api/agents/{id}", get(get_agent))
        .route("/api/agents/{id}", delete(delete_agent_handler))
}

async fn list_agents(_user: AuthUser, State(state): State<AppState>) -> Json<Value> {
    match crate::db::agents::list_agents(&state.inner.db).await {
        Ok(agents) => Json(json!({ "agents": agents })),
        Err(e) => Json(json!({ "error": e.to_string() })),
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
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Json<Value> {
    let (identity_agent_id, agent_dir) = match state.inner.identity_manager.create_agent(&req.name) {
        Ok(result) => result,
        Err(e) => return Json(json!({ "error": e })),
    };

    if let Err(e) = state.inner.identity_manager.create_birth_documents(
        &identity_agent_id,
        &agent_dir,
        &req.name,
        req.agent_type.as_deref(),
        req.pubkey.as_deref(),
    ) {
        let _ = state.inner.identity_manager.remove_agent(&identity_agent_id);
        let _ = std::fs::remove_dir_all(&agent_dir);
        return Json(json!({ "error": e }));
    }

    match crate::db::agents::create_agent(&state.inner.db, &req.name).await {
        Ok(_agent) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                None,
                None,
                "agents.create",
                Some("agent"),
                Some(json!({
                    "name": req.name,
                    "identity_agent_id": identity_agent_id,
                    "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
                })),
                None,
                None,
            )
            .await;
            Json(json!({
                "status": "READY",
                "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
                "soul_immutable": true,
            }))
        }
        Err(e) => {
            let _ = state.inner.identity_manager.remove_agent(&identity_agent_id);
            let _ = std::fs::remove_dir_all(&agent_dir);
            Json(json!({ "error": e.to_string() }))
        }
    }
}

async fn get_agent(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Value> {
    match crate::db::agents::get_agent(&state.inner.db, id).await {
        Ok(Some(agent)) => Json(json!(agent)),
        Ok(None) => Json(json!({ "error": "Agent not found" })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

async fn delete_agent_handler(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Value> {
    match crate::db::agents::delete_agent(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.user_id),
                None,
                "agents.delete",
                Some("agent"),
                Some(json!({ "agent_id": id })),
                None,
                None,
            )
            .await;
            Json(json!({ "deleted": true }))
        }
        Ok(false) => Json(json!({ "error": "Agent not found" })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}
