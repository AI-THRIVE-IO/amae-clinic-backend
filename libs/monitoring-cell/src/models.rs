// =====================================================================================
// MONITORING CELL MODELS
// =====================================================================================

use std::collections::HashMap;
use std::time::Instant;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub component: String,
    pub status: HealthStatus,
    pub response_time_ms: u64,
    pub last_checked: chrono::DateTime<chrono::Utc>,
    pub error_message: Option<String>,
    pub details: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub overall_status: HealthStatus,
    pub system_uptime_seconds: u64,
    pub components: Vec<HealthCheck>,
    pub performance_metrics: PerformanceMetrics,
    pub business_metrics: BusinessMetrics,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub requests_per_second: f64,
    pub average_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub error_rate_percentage: f64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percentage: f64,
    pub active_connections: u64,
    pub cache_hit_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessMetrics {
    pub total_patients: u64,
    pub active_doctors: u64,
    pub appointments_today: u64,
    pub appointments_this_week: u64,
    pub video_sessions_active: u64,
    pub prescription_requests_pending: u64,
    pub average_appointment_duration_minutes: f64,
    pub patient_satisfaction_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub requests_per_second: f64,
    pub average_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub error_rate_percentage: f64,
    pub total_requests: u64,
    pub total_errors: u64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub alert_id: String,
    pub severity: AlertSeverity,
    pub title: String,
    pub description: String,
    pub component: String,
    pub metric_value: f64,
    pub threshold: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

#[derive(Debug, Clone)]
pub struct AlertRule {
    pub name: String,
    pub metric_name: String,
    pub threshold: f64,
    pub comparison: AlertComparison,
    pub severity: AlertSeverity,
    pub duration_minutes: u64,
}

#[derive(Debug, Clone)]
pub enum AlertComparison {
    GreaterThan,
    LessThan,
    Equals,
}

// Request/Response models
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthCheckRequest {
    pub include_details: Option<bool>,
    pub components: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct HealthCheckResponse {
    pub status: HealthStatus,
    pub uptime_seconds: u64,
    pub healthy_components: u32,
    pub degraded_components: u32,
    pub unhealthy_components: u32,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub details: Option<SystemHealth>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsRequest {
    pub time_range_hours: Option<u32>,
    pub metric_types: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub current_metrics: PerformanceMetrics,
    pub business_metrics: BusinessMetrics,
    pub alerts: Vec<Alert>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum MonitoringError {
    #[error("Component health check failed: {0}")]
    HealthCheckFailed(String),
    #[error("Metrics collection error: {0}")]
    MetricsError(String),
    #[error("Alert system error: {0}")]
    AlertError(String),
    #[error("Service unavailable")]
    ServiceUnavailable,
}