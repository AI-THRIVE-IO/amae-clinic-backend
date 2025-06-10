// libs/video-conferencing-cell/src/router.rs
use std::sync::Arc;

use axum::{
    middleware,
    routing::{get, post, put, delete},
    Router,
};

use shared_config::AppConfig;
use shared_utils::extractor::auth_middleware;

use crate::handlers::*;

/// Creates the video conferencing routes
/// Follows the RESTful API design pattern used by other cells
pub fn video_conferencing_routes(state: Arc<AppConfig>) -> Router {
    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/health", get(video_health_check));

    // Protected routes (authentication required)
    let protected_routes = Router::new()
        // Video session management
        .route("/sessions", post(create_video_session))
        .route("/sessions/{session_id}", get(get_video_session))
        .route("/sessions/{session_id}/join", post(join_video_session))
        .route("/sessions/{session_id}/tracks", post(add_session_tracks))
        .route("/sessions/{session_id}/renegotiate", put(renegotiate_session))
        .route("/sessions/{session_id}/end", delete(end_video_session))
        
        // Appointment integration
        .route("/appointments/{appointment_id}/session", post(create_session_for_appointment))
        .route("/appointments/{appointment_id}/availability", get(check_video_availability))
        .route("/appointments/{appointment_id}/stats", get(get_appointment_video_stats))
        
        // User session management
        .route("/upcoming", get(get_upcoming_sessions))
        
        // Admin endpoints
        .route("/admin/cleanup", post(cleanup_expired_sessions))
        
        // Apply authentication middleware to all protected routes
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // Combine public and protected routes
    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
}

