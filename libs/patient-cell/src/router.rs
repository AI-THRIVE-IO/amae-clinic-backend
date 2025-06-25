use std::sync::Arc;
use axum::{middleware, routing::{get, post, put}, Router};
use shared_config::AppConfig;
use shared_utils::extractor::auth_middleware;

use crate::handlers::*;

pub fn create_patient_router(config: Arc<AppConfig>) -> Router {
    Router::new()
        .route("/", post(create_patient))
        .route("/{id}", get(get_patient))
        .route("/{id}", put(update_patient))
        .route("/profile", get(get_patient_profile))
        .route("/search", get(search_patients))
        .layer(middleware::from_fn_with_state(config.clone(), auth_middleware))
        .with_state(config)
}