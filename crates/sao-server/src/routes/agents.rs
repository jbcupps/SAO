use axum::{extract::State, routing::{get, post}, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/agents", get(list_agents))
        .route("/api/agents", post(create_agent))
}

async fn list_agents(State(state): State<AppState>) -> Json<Value> {
    match state.inner.identity_manager.list_agents() {
        Ok(agents) => Json(json\!({ "agents": agents })),
        Err(e) => Json(json\!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct CreateAgentRequest { name: String }

async fn create_agent(State(state): State<AppState>, Json(req): Json<CreateAgentRequest>) -> Json<Value> {
    match state.inner.identity_manager.create_agent(&req.name) {
        Ok((uuid, dir)) => Json(json\!({ "id": uuid, "directory": dir.to_string_lossy() })),
        Err(e) => Json(json\!({ "error": e })),
    }
}
