use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post, put, patch, delete},
    middleware,
};

use shared_config::AppConfig;
use shared_utils::extractor::auth_middleware;

use crate::handlers;

pub fn doctor_routes(state: Arc<AppConfig>) -> Router {
    // Public routes (no authentication required)
    let public_routes = Router::new()
        // Public doctor search and viewing
        .route("/search", get(handlers::search_doctors))
        .route("/{doctor_id}", get(handlers::get_doctor))
        .route("/{doctor_id}/specialties", get(handlers::get_doctor_specialties))
        .route("/{doctor_id}/availability", get(handlers::get_doctor_availability))
        .route("/{doctor_id}/available-slots", get(handlers::get_available_slots));

    // Protected routes (authentication required)
    let protected_routes = Router::new()
        // Doctor profile management
        .route("/", post(handlers::create_doctor))
        .route("/{doctor_id}", put(handlers::update_doctor))
        .route("/{doctor_id}/verify", patch(handlers::verify_doctor))
        .route("/{doctor_id}/stats", get(handlers::get_doctor_stats))
        .route("/{doctor_id}/profile-image", post(handlers::upload_doctor_profile_image))
        
        // Doctor specialties
        .route("/{doctor_id}/specialties", post(handlers::add_doctor_specialty))
        
        // Availability management
        .route("/{doctor_id}/availability", post(handlers::create_availability))
        .route("/{doctor_id}/availability/{availability_id}", put(handlers::update_availability))
        .route("/{doctor_id}/availability/{availability_id}", delete(handlers::delete_availability))
        .route("/{doctor_id}/availability-overrides", post(handlers::create_availability_override))
        
        // Doctor matching and recommendations
        .route("/matching/find", get(handlers::find_matching_doctors))
        .route("/matching/best", post(handlers::find_best_doctor))
        .route("/recommendations", get(handlers::get_recommended_doctors))
        
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
}