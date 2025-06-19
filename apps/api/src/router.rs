use std::sync::Arc;

use axum::{
    Router,
    routing::get,
};

use auth_cell::router::auth_routes;
use health_profile_cell::router::health_profile_routes;
use doctor_cell::router::doctor_routes;
use appointment_cell::router::appointment_routes;
use video_conferencing_cell::router::video_conferencing_routes;
use patient_cell::router::create_patient_router;
use security_cell::create_security_router;
use monitoring_cell::create_monitoring_router;
use performance_cell::create_performance_router;
use shared_config::AppConfig;

pub fn create_router(state: Arc<AppConfig>) -> Router {
    Router::new()
        .route("/", get(|| async { "Amae Clinic API is running!" }))
        .nest("/auth", auth_routes(state.clone()))
        .nest("/health", health_profile_routes(state.clone()))
        .nest("/doctors", doctor_routes(state.clone()))
        .nest("/appointments", appointment_routes(state.clone()))
        .nest("/video", video_conferencing_routes(state.clone()))
        .nest("/patients", create_patient_router(state.clone()))
        .nest("/security", create_security_router(state.clone()))
        .nest("/monitoring", create_monitoring_router(state.clone()))
        .nest("/performance", create_performance_router(state.clone()))
}