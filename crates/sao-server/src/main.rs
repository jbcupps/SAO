mod db;
mod routes;
mod state;
mod ws;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting SAO - Secure Agent Orchestrator");

    // Initialize database pool
    let db = db::DbPool::from_env().await;

    // Initialize application state
    let app_state = state::init_app_state(db);

    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the router
    let app = Router::new()
        .merge(routes::routes())
        .merge(ws::ws_routes())
        .layer(cors)
        .with_state(app_state);

    // Bind and serve
    let bind_addr = std::env::var("SAO_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3100".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    tracing::info!("SAO server listening on {}", bind_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
