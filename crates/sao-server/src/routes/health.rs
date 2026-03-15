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

    Json(build_health_payload(db_healthy))
}

fn build_health_payload(db_healthy: bool) -> Value {
    let status = if db_healthy { "ok" } else { "degraded" };

    json!({
        "status": status,
        "service": "sao",
        "version": env!("CARGO_PKG_VERSION"),
        "database": {
            "connected": db_healthy,
            "healthy": db_healthy,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::build_health_payload;

    #[test]
    fn health_payload_marks_database_disconnected_when_probe_fails() {
        let payload = build_health_payload(false);

        assert_eq!(payload["status"], "degraded");
        assert_eq!(payload["database"]["connected"], false);
        assert_eq!(payload["database"]["healthy"], false);
    }
}
