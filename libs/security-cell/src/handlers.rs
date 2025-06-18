// =====================================================================================
// SECURITY CELL HANDLERS - HTTP ENDPOINTS
// =====================================================================================

use axum::{
    extract::{Query, State, ConnectInfo},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn, instrument};
use uuid::Uuid;

use crate::models::{
    SecurityHealthRequest, SecurityHealthResponse, SecurityHealthDetails, SecurityMetrics,
    ValidateInputRequest, ValidateInputResponse, PasswordValidationRequest, PasswordValidationResponse,
    SecurityError, AuditEntry, AuditEventType, AuditOutcome, SuspiciousActivity,
};
use crate::services::{AuditService, ValidationService, SecurityMonitoringService, PasswordSecurityService};
use shared_config::AppConfig;
use shared_utils::jwt::validate_token;

pub struct SecurityHandlers {
    pub audit_service: Arc<AuditService>,
    pub validation_service: Arc<ValidationService>,
    pub monitoring_service: Arc<SecurityMonitoringService>,
    pub password_service: Arc<PasswordSecurityService>,
    pub config: Arc<AppConfig>,
}

impl SecurityHandlers {
    pub fn new(config: Arc<AppConfig>) -> Self {
        let audit_service = Arc::new(AuditService::new(&config));
        let validation_service = Arc::new(ValidationService::with_default_config());
        let monitoring_service = Arc::new(SecurityMonitoringService::new(audit_service.clone()));
        let password_service = Arc::new(PasswordSecurityService::new());

        Self {
            audit_service,
            validation_service,
            monitoring_service,
            password_service,
            config,
        }
    }
}

// =====================================================================================
// HEALTH CHECK ENDPOINTS
// =====================================================================================

#[instrument(skip(handlers))]
pub async fn get_security_health(
    State(handlers): State<Arc<SecurityHandlers>>,
    Query(request): Query<SecurityHealthRequest>,
) -> Result<Json<SecurityHealthResponse>, SecurityError> {
    let blocked_ips = handlers.monitoring_service.get_blocked_ips().await;
    let failed_attempts = handlers.monitoring_service.get_failed_login_stats().await;
    let suspicious_activities = handlers.monitoring_service.get_recent_suspicious_activities(24).await;

    let details = if request.include_details.unwrap_or(false) {
        let recent_blocked_ips: Vec<String> = blocked_ips.iter()
            .take(10)
            .map(|(ip, _)| ip.clone())
            .collect();

        let security_metrics = SecurityMetrics {
            login_success_rate: 95.5, // Would calculate from audit logs
            average_risk_score: suspicious_activities.iter()
                .map(|a| a.risk_score as f64)
                .sum::<f64>() / suspicious_activities.len().max(1) as f64,
            top_threat_types: vec![
                "Failed Login Attempts".to_string(),
                "SQL Injection Attempts".to_string(),
                "XSS Attempts".to_string(),
            ],
        };

        Some(SecurityHealthDetails {
            recent_blocked_ips,
            recent_suspicious_activities: suspicious_activities.clone(),
            security_metrics,
        })
    } else {
        None
    };

    let response = SecurityHealthResponse {
        status: "healthy".to_string(),
        blocked_ips: blocked_ips.len() as u32,
        failed_login_attempts: failed_attempts.len() as u32,
        suspicious_activities: suspicious_activities.len() as u32,
        audit_entries_today: 0, // Would query audit database
        details,
    };

    Ok(Json(response))
}

// =====================================================================================
// VALIDATION ENDPOINTS
// =====================================================================================

#[instrument(skip(handlers, request))]
pub async fn validate_input(
    State(handlers): State<Arc<SecurityHandlers>>,
    Json(request): Json<ValidateInputRequest>,
) -> Result<Json<ValidateInputResponse>, SecurityError> {
    let validation_result = handlers.validation_service.validate_input(&request.value, &request.field_name);

    // Log high-risk validation attempts
    if validation_result.risk_score >= 50 {
        let audit_entry = AuditEntry::new(
            AuditEventType::InvalidDataSubmission,
            format!("High-risk input validation for field: {}", request.field_name),
            AuditOutcome::Denied,
        )
        .with_risk_score(validation_result.risk_score)
        .add_context("field_name", &request.field_name)
        .add_context("validation_type", request.validation_type.as_deref().unwrap_or("general"));

        let _ = handlers.audit_service.log_audit_entry(audit_entry).await;
    }

    let response = ValidateInputResponse {
        is_valid: validation_result.is_valid,
        sanitized_value: validation_result.sanitized_input,
        risk_score: validation_result.risk_score,
        issues: validation_result.issues.iter()
            .map(|issue| format!("{:?}", issue))
            .collect(),
    };

    Ok(Json(response))
}

#[instrument(skip(handlers, request))]
pub async fn validate_password(
    State(handlers): State<Arc<SecurityHandlers>>,
    Json(request): Json<PasswordValidationRequest>,
) -> Result<Json<PasswordValidationResponse>, SecurityError> {
    let strength_result = PasswordSecurityService::validate_password_strength(&request.password);
    
    // Check for breached passwords
    let is_breached = PasswordSecurityService::check_password_breaches(&request.password);
    let mut suggestions = strength_result.issues.clone();
    
    if is_breached {
        suggestions.insert(0, "This password has been found in data breaches. Please choose a different password.".to_string());
    }

    let requirements_met = strength_result.score >= 60 && !is_breached;

    let response = PasswordValidationResponse {
        strength: strength_result.strength,
        score: strength_result.score,
        requirements_met,
        suggestions,
    };

    Ok(Json(response))
}

// =====================================================================================
// MONITORING ENDPOINTS
// =====================================================================================

#[instrument(skip(handlers))]
pub async fn get_blocked_ips(
    State(handlers): State<Arc<SecurityHandlers>>,
    headers: HeaderMap,
) -> Result<Json<Vec<String>>, SecurityError> {
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

    let blocked_ips = handlers.monitoring_service.get_blocked_ips().await;
    let ip_list: Vec<String> = blocked_ips.into_iter().map(|(ip, _)| ip).collect();

    Ok(Json(ip_list))
}

#[instrument(skip(handlers))]
pub async fn unblock_ip(
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

    handlers.monitoring_service.unblock_ip(ip_address).await
        .map_err(|e| SecurityError::ServiceError(e.to_string()))?;

    info!("Admin {} unblocked IP: {}", user.id, ip_address);
    Ok(StatusCode::OK)
}

#[instrument(skip(handlers))]
pub async fn get_suspicious_activities(
    State(handlers): State<Arc<SecurityHandlers>>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<SuspiciousActivity>>, SecurityError> {
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

    let hours = params.get("hours")
        .and_then(|h| h.parse::<u32>().ok())
        .unwrap_or(24);

    let activities = handlers.monitoring_service.get_recent_suspicious_activities(hours).await;
    Ok(Json(activities))
}

// =====================================================================================
// AUDIT ENDPOINTS
// =====================================================================================

#[instrument(skip(handlers))]
pub async fn get_user_audit_log(
    State(handlers): State<Arc<SecurityHandlers>>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AuditEntry>>, SecurityError> {
    // Extract user from token
    let token = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
        .ok_or(SecurityError::AuthenticationRequired)?;

    let user = validate_token(token, &handlers.config.supabase_jwt_secret)
        .map_err(|_| SecurityError::AuthenticationRequired)?;

    // Users can only see their own audit logs unless they're admin
    let target_user_id = params.get("user_id")
        .unwrap_or(&user.id);

    if target_user_id != &user.id && user.role.as_deref() != Some("admin") {
        return Err(SecurityError::InsufficientPermissions);
    }

    let limit = params.get("limit")
        .and_then(|l| l.parse::<u32>().ok())
        .unwrap_or(50);

    let audit_entries = handlers.audit_service.get_audit_entries_for_user(target_user_id, Some(limit)).await
        .map_err(|e| SecurityError::ServiceError(e.to_string()))?;

    Ok(Json(audit_entries))
}

// =====================================================================================
// SECURITY MIDDLEWARE HANDLER
// =====================================================================================

#[instrument(skip(handlers, headers))]
pub async fn security_check(
    State(handlers): State<Arc<SecurityHandlers>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<StatusCode, Response> {
    let ip_address = addr.ip().to_string();
    
    // Check if IP is blocked
    if handlers.monitoring_service.is_ip_blocked(&ip_address).await {
        let audit_entry = AuditEntry::new(
            AuditEventType::SuspiciousActivity,
            "Blocked IP attempted access".to_string(),
            AuditOutcome::Denied,
        )
        .with_risk_score(80)
        .add_context("ip_address", &ip_address);

        let _ = handlers.audit_service.log_audit_entry(audit_entry).await;

        return Err((StatusCode::TOO_MANY_REQUESTS, "IP temporarily blocked").into_response());
    }

    Ok(StatusCode::OK)
}

// =====================================================================================
// ERROR RESPONSE IMPLEMENTATION
// =====================================================================================

impl IntoResponse for SecurityError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            SecurityError::ValidationFailed(_) => (StatusCode::BAD_REQUEST, "Invalid input detected"),
            SecurityError::IpBlocked => (StatusCode::TOO_MANY_REQUESTS, "Access temporarily restricted"),
            SecurityError::AuthenticationRequired => (StatusCode::UNAUTHORIZED, "Authentication required"),
            SecurityError::InsufficientPermissions => (StatusCode::FORBIDDEN, "Insufficient permissions"),
            SecurityError::ServiceError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Security service error"),
        };

        (status, Json(serde_json::json!({
            "error": message,
            "timestamp": chrono::Utc::now()
        }))).into_response()
    }
}