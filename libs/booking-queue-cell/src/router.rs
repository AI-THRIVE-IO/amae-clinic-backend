use std::sync::Arc;
use axum::{
    Router,
    routing::{get, post},
    middleware,
};

use shared_config::AppConfig;
use shared_utils::extractor::auth_middleware;
use crate::handlers::{
    enqueue_smart_booking,
    get_job_status,
    cancel_job,
    retry_job,
    get_queue_stats,
    get_websocket_endpoint,
};

pub fn create_booking_queue_router(state: Arc<AppConfig>) -> Router {
    let protected_routes = Router::new()
        .route("/smart-book", post(enqueue_smart_booking))
        .route("/jobs/{job_id}/status", get(get_job_status))
        .route("/jobs/{job_id}/cancel", post(cancel_job))
        .route("/jobs/{job_id}/retry", post(retry_job))
        .route("/stats", get(get_queue_stats))
        .route("/websocket", get(get_websocket_endpoint))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .merge(protected_routes)
        .with_state(state)
}