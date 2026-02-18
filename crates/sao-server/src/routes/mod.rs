pub mod admin;
pub mod agents;
pub mod auth;
pub mod ethical;
pub mod health;
pub mod oidc;
pub mod setup;
pub mod vault;

use crate::state::AppState;
use axum::Router;

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
