pub mod health;
pub mod agents;
pub mod ethical;
pub mod vault;
pub mod setup;
pub mod auth;
pub mod oidc;
pub mod admin;

use axum::Router;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Public routes (no auth required)
        .merge(health::routes())
        .merge(setup::routes())
        .merge(auth::routes())
        .merge(oidc::routes())
        // Protected routes (auth enforced at handler level via extractors)
        .merge(agents::routes())
        .merge(vault::routes())
        .merge(ethical::routes())
        .merge(admin::routes())
}
