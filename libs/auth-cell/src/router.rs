use std::sync::Arc;

use axum::{
    Router,
    routing::post,
    middleware,
};

use shared_config::AppConfig;
use shared_utils::extractor::auth_middleware;

use crate::handlers;

pub fn auth_routes(state: Arc<AppConfig>) -> Router {
    let public_routes = Router::new()
        .route("/validate", post(handlers::validate_token))
        .route("/verify", post(handlers::verify_token));
        
    let protected_routes = Router::new()
        .route("/profile", post(handlers::get_profile))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));
        
    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
}