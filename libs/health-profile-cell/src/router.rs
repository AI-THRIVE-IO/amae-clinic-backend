use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post, put, delete},
    middleware,
};

use shared_config::AppConfig;
use shared_utils::extractor::auth_middleware;

use crate::handlers;

pub fn health_profile_routes(state: Arc<AppConfig>) -> Router {
    // Protected routes
    let protected_routes = Router::new()
        // Health profile endpoints
        .route("/health-profiles/{id}", get(handlers::get_health_profile))
        .route("/health-profiles/{id}", put(handlers::update_health_profile))
        .route("/health-profiles", post(handlers::create_health_profile))
        .route("/health-profiles/{id}", delete(handlers::delete_health_profile))
        
        // Avatar endpoints
        .route("/health-profiles/{id}/avatar", post(handlers::upload_avatar))
        .route("/health-profiles/{id}/avatar", delete(handlers::remove_avatar))
        
        // Document endpoints
        .route("/health-profiles/{id}/documents", get(handlers::get_documents))
        .route("/health-profiles/{id}/documents", post(handlers::upload_document))
        .route("/health-profiles/{id}/documents/{doc_id}", get(handlers::get_document))
        .route("/health-profiles/{id}/documents/{doc_id}", delete(handlers::delete_document))
        
        // AI features
        .route("/health-profiles/{id}/ai/nutrition-plan", post(handlers::generate_nutrition_plan))
        .route("/health-profiles/{id}/ai/care-plan", post(handlers::generate_care_plan))
        
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));
        
    Router::new()
        .merge(protected_routes)
        .with_state(state)
}