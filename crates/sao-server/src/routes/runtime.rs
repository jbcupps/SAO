use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde_json::json;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/runtime/capabilities", get(capabilities))
}

async fn capabilities(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match state.inner.runtime.capabilities() {
        Ok(report) => (StatusCode::OK, Json(json!(report))),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, Json(error.to_value())),
    }
}
