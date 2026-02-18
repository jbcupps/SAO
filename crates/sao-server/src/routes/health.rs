use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/health", get(health_check))
}

async fn health_check(State(state): State<AppState>) -> Json<Value> {
    let db_healthy = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.inner.db)
        .await
        .is_ok();

    let status = if db_healthy { "ok" } else { "degraded" };

    Json(json!({
        "status": status,
        "service": "sao",
        "version": env!("CARGO_PKG_VERSION"),
        "database": {
            "connected": true,
            "healthy": db_healthy,
        },
    }))
}
