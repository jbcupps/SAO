use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/setup/status", get(setup_status))
}

async fn setup_status(State(state): State<AppState>) -> Json<Value> {
    let vault_state = state.inner.vault_state.read().await;
    let initialized = !matches!(*vault_state, crate::vault_state::VaultState::Uninitialized);
    let has_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&state.inner.db)
        .await
        .unwrap_or(0)
        > 0;
    let needs_setup = !initialized || !has_users;

    Json(json!({
        "initialized": initialized,
        "has_users": has_users,
        "needs_setup": needs_setup,
        "bootstrap_mode": if needs_setup { "installer_required" } else { "operational" },
        "recommended_installer": {
            "command": "docker build -f installer/Dockerfile -t sao-installer installer && docker run --rm -it -e ANTHROPIC_API_KEY=<your-key> sao-installer",
            "image_role": "standalone_conversational_bootstrapper",
        },
    }))
}
