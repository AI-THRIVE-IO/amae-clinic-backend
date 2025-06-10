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
        .route("/search", get(handlers::search_doctors_public))
        .route("/{doctor_id}", get(handlers::get_doctor_public))
        .route("/{doctor_id}/specialties", get(handlers::get_doctor_specialties_public))
        .route("/{doctor_id}/availability", get(handlers::get_doctor_availability_public))
        .route("/{doctor_id}/available-slots", get(handlers::get_available_slots_public));

    // Protected routes (authentication required)
    let protected_routes = Router::new()
        // Doctor profile management - NO PRICING ENDPOINTS
        .route("/", post(handlers::create_doctor))
        .route("/{doctor_id}", put(handlers::update_doctor))
        .route("/{doctor_id}/verify", patch(handlers::verify_doctor))
        .route("/{doctor_id}/stats", get(handlers::get_doctor_stats))
        .route("/{doctor_id}/profile-image", post(handlers::upload_doctor_profile_image))
        
        // Doctor specialties management
        .route("/{doctor_id}/specialties", post(handlers::add_doctor_specialty))
        
        // Availability management - NO PRICING IN SCHEDULES
        .route("/{doctor_id}/availability", post(handlers::create_availability))
        .route("/{doctor_id}/availability/{availability_id}", put(handlers::update_availability))
        .route("/{doctor_id}/availability/{availability_id}", delete(handlers::delete_availability))
        .route("/{doctor_id}/availability-overrides", post(handlers::create_availability_override))
        
        // Doctor matching and recommendations - NO COST FILTERING
        .route("/matching/find", get(handlers::find_matching_doctors))
        .route("/matching/best", post(handlers::find_best_doctor))
        .route("/recommendations", get(handlers::get_recommended_doctors))

        // Authenticated versions of search/get for full access
        .route("/auth/search", get(handlers::search_doctors))  // Full authenticated search
        .route("/auth/{doctor_id}", get(handlers::get_doctor))  // Full authenticated get
        .route("/auth/{doctor_id}/available-slots", get(handlers::get_available_slots))  // Authenticated slots
        
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
}