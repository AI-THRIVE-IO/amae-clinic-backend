use axum::{routing::get, Router};
use std::sync::Arc;

use crate::handlers::{get_performance_stats, PerformanceHandlers};
use shared_config::AppConfig;

pub fn create_performance_router(_config: Arc<AppConfig>) -> Router {
    let handlers = Arc::new(PerformanceHandlers::new());

    Router::new()
        .route("/stats", get(get_performance_stats))
        .with_state(handlers)
}