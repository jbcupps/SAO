use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/health", get(health_check))
}

async fn health_check() -> Json<Value> {
    Json(json\!({
        "status": "ok",
        "service": "sao",
        "version": env\!("CARGO_PKG_VERSION"),
    }))
}
