// =====================================================================================
// SECURITY CELL ROUTER
// =====================================================================================

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::handlers::{
    get_security_health, validate_input, validate_password,
    get_blocked_ips, unblock_ip, get_suspicious_activities, get_user_audit_log,
    security_check, SecurityHandlers,
};
use shared_config::AppConfig;
use shared_utils::jwt::validate_token;
// use shared_utils::auth::auth_middleware;

pub fn create_security_router(config: Arc<AppConfig>) -> Router {
    let handlers = Arc::new(SecurityHandlers::new(config.clone()));

    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/health", get(get_security_health))
        .route("/validate", post(validate_input))
        .route("/password/validate", post(validate_password))
        .layer(CorsLayer::permissive())
        .with_state(handlers.clone());

    // Protected routes (authentication required)
    let protected_routes = Router::new()
        .route("/audit/user", get(get_user_audit_log))
        .route("/monitoring/blocked-ips", get(get_blocked_ips))
        .route("/monitoring/unblock-ip", post(unblock_ip))
        .route("/monitoring/suspicious-activities", get(get_suspicious_activities))
        // .layer(middleware::from_fn_with_state(
        //     config.clone(),
        //     auth_middleware,
        // ))
        .with_state(handlers.clone());

    // Admin only routes (require admin role)
    let admin_routes = Router::new()
        .route("/admin/cleanup", post(cleanup_security_data))
        .route("/admin/block-ip", post(manually_block_ip))
        // .layer(middleware::from_fn_with_state(
        //     config.clone(),
        //     auth_middleware,
        // ))
        .with_state(handlers.clone());

    // Security middleware route (for use by other services)
    let middleware_routes = Router::new()
        .route("/middleware/check", post(security_check))
        .with_state(handlers);

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(admin_routes)
        .merge(middleware_routes)
}

// =====================================================================================
// ADDITIONAL ADMIN HANDLERS
// =====================================================================================

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::collections::HashMap;
use tracing::{info, instrument};

use crate::models::SecurityError;
// use shared_utils::auth::extract_user_from_token;

#[instrument(skip(handlers))]
async fn cleanup_security_data(
    State(handlers): State<Arc<SecurityHandlers>>,
    headers: HeaderMap,
) -> Result<StatusCode, SecurityError> {
    // Require admin authentication
    let token = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
        .ok_or(SecurityError::AuthenticationRequired)?;

    let user = validate_token(token, &handlers.config.supabase_jwt_secret)
        .map_err(|_| SecurityError::AuthenticationRequired)?;

    if user.role.as_deref() != Some("admin") {
        return Err(SecurityError::InsufficientPermissions);
    }

    handlers.monitoring_service.cleanup_old_data().await
        .map_err(|e| SecurityError::ServiceError(e.to_string()))?;

    handlers.audit_service.flush_audit_buffer().await
        .map_err(|e| SecurityError::ServiceError(e.to_string()))?;

    info!("Admin {} triggered security data cleanup", user.id);
    Ok(StatusCode::OK)
}

#[instrument(skip(handlers))]
async fn manually_block_ip(
    State(handlers): State<Arc<SecurityHandlers>>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<StatusCode, SecurityError> {
    // Require admin authentication
    let token = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
        .ok_or(SecurityError::AuthenticationRequired)?;

    let user = validate_token(token, &handlers.config.supabase_jwt_secret)
        .map_err(|_| SecurityError::AuthenticationRequired)?;

    if user.role.as_deref() != Some("admin") {
        return Err(SecurityError::InsufficientPermissions);
    }

    let ip_address = params.get("ip")
        .ok_or_else(|| SecurityError::ServiceError("IP address parameter required".to_string()))?;

    let reason = params.get("reason")
        .map(|s| s.as_str())
        .unwrap_or("Manual admin block");

    let duration_hours = params.get("duration")
        .and_then(|d| d.parse::<u32>().ok())
        .unwrap_or(24);

    handlers.monitoring_service.manually_block_ip(ip_address, reason, duration_hours).await
        .map_err(|e| SecurityError::ServiceError(e.to_string()))?;

    info!("Admin {} manually blocked IP {} for {} hours: {}", 
          user.id, ip_address, duration_hours, reason);
    
    Ok(StatusCode::OK)
}