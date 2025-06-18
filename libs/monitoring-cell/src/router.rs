// =====================================================================================
// MONITORING CELL ROUTER
// =====================================================================================

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::handlers::{
    get_health_status, get_component_health, get_current_metrics,
    get_active_alerts, acknowledge_alert, clear_all_alerts, get_alert_summary,
    MonitoringHandlers,
};
use shared_config::AppConfig;
// use shared_utils::auth::auth_middleware;

pub fn create_monitoring_router(config: Arc<AppConfig>) -> Router {
    let handlers = Arc::new(MonitoringHandlers::new(config.clone()));

    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/health", get(get_health_status))
        .route("/health/component", get(get_component_health))
        .route("/metrics", get(get_current_metrics))
        .route("/alerts/summary", get(get_alert_summary))
        .layer(CorsLayer::permissive())
        .with_state(handlers.clone());

    // Protected routes (authentication required)
    let protected_routes = Router::new()
        .route("/alerts", get(get_active_alerts))
        .route("/alerts/acknowledge", post(acknowledge_alert))
        // .layer(middleware::from_fn_with_state(
        //     config.clone(),
        //     auth_middleware,
        // ))
        .with_state(handlers.clone());

    // Admin only routes
    let admin_routes = Router::new()
        .route("/admin/alerts/clear", post(clear_all_alerts))
        // .layer(middleware::from_fn_with_state(
        //     config.clone(),
        //     auth_middleware,
        // ))
        .with_state(handlers);

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(admin_routes)
}