use std::sync::Arc;

use axum::{
    Router,
    routing::get,
};

use auth_cell::router::auth_routes;
use shared_config::AppConfig;

pub fn create_router(state: Arc<AppConfig>) -> Router {
    Router::new()
        .route("/", get(|| async { "Amae Clinic API is running!" }))
        .route("/health", get(health_check))
        .nest("/auth", auth_routes(state.clone()))
        // Other cells added later
}

async fn health_check() -> &'static str {
    "Amae Clinic Backend API is running"
}