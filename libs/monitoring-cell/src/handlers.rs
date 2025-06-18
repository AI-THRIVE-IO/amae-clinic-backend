// =====================================================================================
// MONITORING CELL HANDLERS
// =====================================================================================

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use tracing::{info, instrument};

use crate::models::{
    HealthCheckRequest, HealthCheckResponse, MetricsRequest, MetricsResponse,
    HealthStatus, MonitoringError, Alert,
};
use crate::services::{HealthMonitorService, MetricsCollectorService, AlertManagerService};
use shared_config::AppConfig;
use shared_utils::jwt::validate_token;

pub struct MonitoringHandlers {
    health_service: Arc<HealthMonitorService>,
    metrics_service: Arc<MetricsCollectorService>,
    alert_service: Arc<AlertManagerService>,
    config: Arc<AppConfig>,
}

impl MonitoringHandlers {
    pub fn new(config: Arc<AppConfig>) -> Self {
        let metrics_service = Arc::new(MetricsCollectorService::new());
        let alert_service = Arc::new(AlertManagerService::new());
        let health_service = Arc::new(HealthMonitorService::new(
            &config, 
            metrics_service.clone(), 
            alert_service.clone()
        ));

        Self {
            health_service,
            metrics_service,
            alert_service,
            config,
        }
    }

    pub fn get_metrics_service(&self) -> Arc<MetricsCollectorService> {
        self.metrics_service.clone()
    }
}

// =====================================================================================
// PUBLIC HEALTH CHECK ENDPOINTS
// =====================================================================================

#[instrument(skip(handlers))]
pub async fn get_health_status(
    State(handlers): State<Arc<MonitoringHandlers>>,
    Query(request): Query<HealthCheckRequest>,
) -> Result<Json<HealthCheckResponse>, MonitoringError> {
    let health = handlers.health_service.perform_comprehensive_health_check().await?;
    
    let healthy_count = health.components.iter()
        .filter(|c| matches!(c.status, HealthStatus::Healthy))
        .count() as u32;
    
    let degraded_count = health.components.iter()
        .filter(|c| matches!(c.status, HealthStatus::Degraded))
        .count() as u32;
    
    let unhealthy_count = health.components.iter()
        .filter(|c| matches!(c.status, HealthStatus::Unhealthy | HealthStatus::Critical))
        .count() as u32;

    let response = HealthCheckResponse {
        status: health.overall_status.clone(),
        uptime_seconds: health.system_uptime_seconds,
        healthy_components: healthy_count,
        degraded_components: degraded_count,
        unhealthy_components: unhealthy_count,
        last_check: health.timestamp,
        details: if request.include_details.unwrap_or(false) {
            Some(health)
        } else {
            None
        },
    };

    Ok(Json(response))
}

#[instrument(skip(handlers))]
pub async fn get_component_health(
    State(handlers): State<Arc<MonitoringHandlers>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<crate::models::HealthCheck>, MonitoringError> {
    let component_name = params.get("component")
        .ok_or_else(|| MonitoringError::HealthCheckFailed("Component name required".to_string()))?;

    let health_check = handlers.health_service.get_component_health(component_name).await
        .ok_or_else(|| MonitoringError::HealthCheckFailed(format!("Unknown component: {}", component_name)))?;

    Ok(Json(health_check))
}

// =====================================================================================
// METRICS ENDPOINTS
// =====================================================================================

#[instrument(skip(handlers))]
pub async fn get_current_metrics(
    State(handlers): State<Arc<MonitoringHandlers>>,
    Query(_request): Query<MetricsRequest>,
) -> Result<Json<MetricsResponse>, MonitoringError> {
    // Get current performance metrics
    let current_metrics = handlers.health_service.perform_comprehensive_health_check().await?;
    
    // Get active alerts
    let alerts = handlers.alert_service.get_active_alerts().await;

    let response = MetricsResponse {
        current_metrics: current_metrics.performance_metrics,
        business_metrics: current_metrics.business_metrics,
        alerts,
        timestamp: chrono::Utc::now(),
    };

    Ok(Json(response))
}

// =====================================================================================
// ALERT ENDPOINTS (Protected)
// =====================================================================================

#[instrument(skip(handlers))]
pub async fn get_active_alerts(
    State(handlers): State<Arc<MonitoringHandlers>>,
    headers: HeaderMap,
) -> Result<Json<Vec<Alert>>, MonitoringError> {
    // Require authentication for alerts
    let token = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
        .ok_or(MonitoringError::ServiceUnavailable)?;

    let _user = validate_token(token, &handlers.config.supabase_jwt_secret)
        .map_err(|_| MonitoringError::ServiceUnavailable)?;

    let alerts = handlers.alert_service.get_active_alerts().await;
    Ok(Json(alerts))
}

#[instrument(skip(handlers))]
pub async fn acknowledge_alert(
    State(handlers): State<Arc<MonitoringHandlers>>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<StatusCode, MonitoringError> {
    // Require authentication
    let token = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
        .ok_or(MonitoringError::ServiceUnavailable)?;

    let user = validate_token(token, &handlers.config.supabase_jwt_secret)
        .map_err(|_| MonitoringError::ServiceUnavailable)?;

    let alert_id = params.get("alert_id")
        .ok_or_else(|| MonitoringError::AlertError("Alert ID required".to_string()))?;

    let acknowledged = handlers.alert_service.acknowledge_alert(alert_id).await;
    
    if acknowledged {
        info!("User {} acknowledged alert {}", user.id, alert_id);
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

// =====================================================================================
// ADMIN ENDPOINTS
// =====================================================================================

#[instrument(skip(handlers))]
pub async fn clear_all_alerts(
    State(handlers): State<Arc<MonitoringHandlers>>,
    headers: HeaderMap,
) -> Result<StatusCode, MonitoringError> {
    // Require admin authentication
    let token = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
        .ok_or(MonitoringError::ServiceUnavailable)?;

    let user = validate_token(token, &handlers.config.supabase_jwt_secret)
        .map_err(|_| MonitoringError::ServiceUnavailable)?;

    if user.role.as_deref() != Some("admin") {
        return Err(MonitoringError::ServiceUnavailable);
    }

    handlers.alert_service.clear_all_alerts().await;
    info!("Admin {} cleared all alerts", user.id);
    
    Ok(StatusCode::OK)
}

#[instrument(skip(handlers))]
pub async fn get_alert_summary(
    State(handlers): State<Arc<MonitoringHandlers>>,
) -> Result<Json<std::collections::HashMap<String, u32>>, MonitoringError> {
    let summary = handlers.alert_service.get_alert_summary().await;
    Ok(Json(summary))
}

// =====================================================================================
// ERROR RESPONSE IMPLEMENTATION
// =====================================================================================

impl IntoResponse for MonitoringError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            MonitoringError::HealthCheckFailed(_) => (StatusCode::SERVICE_UNAVAILABLE, "Health check failed"),
            MonitoringError::MetricsError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Metrics collection error"),
            MonitoringError::AlertError(_) => (StatusCode::BAD_REQUEST, "Alert system error"),
            MonitoringError::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "Service unavailable"),
        };

        (status, Json(serde_json::json!({
            "error": message,
            "timestamp": chrono::Utc::now()
        }))).into_response()
    }
}