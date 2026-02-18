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

async fn list_agents(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Json<Value> {
    match crate::db::agents::list_agents(&state.inner.db).await {
        Ok(agents) => Json(json!({ "agents": agents })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct CreateAgentRequest {
    name: String,
}

async fn create_agent(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Json<Value> {
    match crate::db::agents::create_agent(&state.inner.db, &req.name).await {
        Ok(agent) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.user_id),
                None,
                "agents.create",
                Some("agent"),
                Some(json!({ "agent_id": agent.id, "name": agent.name })),
                None,
                None,
            )
            .await;
            Json(json!({
                "id": agent.id,
                "name": agent.name,
                "state": agent.state,
            }))
        }
        Err(e) => Json(json!({ "error": e.to_string() })),
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
