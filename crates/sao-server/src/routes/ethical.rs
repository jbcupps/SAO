use axum::{routing::post, Json, Router};
use serde_json::{json, Value};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ethical/evaluate", post(evaluate))
}

async fn evaluate(Json(_body): Json<Value>) -> Json<Value> {
    Json(json\!({
        "status": "not_configured",
        "message": "Ethical evaluation service not yet connected",
    }))
}
