// libs/appointment-cell/src/router.rs
use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post, put, patch},
    middleware,
};

use shared_config::AppConfig;
use shared_utils::extractor::auth_middleware;

use crate::handlers;

pub fn appointment_routes(state: Arc<AppConfig>) -> Router {
    // All appointment operations require authentication
    let protected_routes = Router::new()
        // ENHANCED: Core appointment management with smart booking
        .route("/smart-book", post(handlers::smart_book_appointment)) // NEW: Smart booking with history prioritization
        .route("/", post(handlers::book_appointment))
        .route("/search", get(handlers::search_appointments))
        .route("/{appointment_id}", get(handlers::get_appointment))
        .route("/{appointment_id}", put(handlers::update_appointment))
        .route("/{appointment_id}/reschedule", patch(handlers::reschedule_appointment))
        .route("/{appointment_id}/cancel", post(handlers::cancel_appointment))
        
        // Appointment listings
        .route("/upcoming", get(handlers::get_upcoming_appointments))
        .route("/patients/{patient_id}", get(handlers::get_patient_appointments))
        .route("/doctors/{doctor_id}", get(handlers::get_doctor_appointments))
        
        // Utility endpoints
        .route("/conflicts/check", get(handlers::check_appointment_conflicts))
        .route("/stats", get(handlers::get_appointment_stats)) // ENHANCED: Now includes continuity metrics
        
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .merge(protected_routes)
        .with_state(state)
}