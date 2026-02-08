mod health;
mod agents;
mod ethical;

use axum::Router;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(agents::routes())
        .merge(ethical::routes())
}
