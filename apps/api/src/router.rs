use std::sync::Arc;

use axum::{
    Router,
    routing::get,
};

use auth_cell::router::auth_routes;
use health_profile_cell::router::health_profile_routes;
use shared_config::AppConfig;

pub fn create_router(state: Arc<AppConfig>) -> Router {
    Router::new()
        .route("/", get(|| async { "Amae Clinic API is running!" }))
        .nest("/auth", auth_routes(state.clone()))
        .nest("/health", health_profile_routes(state.clone()))
        // Other cells added later
}