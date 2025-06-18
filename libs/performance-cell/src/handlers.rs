use axum::{extract::State, Json};
use std::sync::Arc;

use crate::models::{PerformanceStats, PerformanceError};
use crate::services::CacheService;

pub struct PerformanceHandlers {
    cache_service: CacheService,
}

impl PerformanceHandlers {
    pub fn new() -> Self {
        Self {
            cache_service: CacheService::new(),
        }
    }
}

pub async fn get_performance_stats(
    State(handlers): State<Arc<PerformanceHandlers>>,
) -> Result<Json<PerformanceStats>, PerformanceError> {
    let stats = handlers.cache_service.get_performance_stats().await;
    Ok(Json(stats))
}

// Error response implementation
use axum::{response::IntoResponse, http::StatusCode};

impl IntoResponse for PerformanceError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            PerformanceError::CacheError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Cache error"),
            PerformanceError::MetricsUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "Metrics unavailable"),
        };

        (status, Json(serde_json::json!({
            "error": message,
            "timestamp": chrono::Utc::now()
        }))).into_response()
    }
}