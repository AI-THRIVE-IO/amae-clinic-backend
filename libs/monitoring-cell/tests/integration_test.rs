// =====================================================================================
// MONITORING CELL INTEGRATION TESTS - PRODUCTION GRADE VALIDATION
// =====================================================================================

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

use monitoring_cell::{
    create_monitoring_router,
    models::{HealthStatus, MetricsSnapshot, Alert, AlertSeverity},
    services::{HealthMonitorService, MetricsCollectorService, AlertManagerService},
    MonitoringHandlers,
};
use shared_config::AppConfig;

async fn setup_test_config() -> Arc<AppConfig> {
    Arc::new(AppConfig {
        supabase_url: "https://test.supabase.co".to_string(),
        supabase_anon_key: "test_anon_key".to_string(),
        supabase_jwt_secret: "test_jwt_secret_for_monitoring_tests".to_string(),
        cloudflare_realtime_app_id: "test_app_id".to_string(),
        cloudflare_realtime_api_token: "test_api_token".to_string(),
        cloudflare_realtime_base_url: "https://test.cloudflare.com".to_string(),
    })
}

#[tokio::test]
async fn test_health_status_endpoint() {
    let config = setup_test_config().await;
    let app = create_monitoring_router(config);

    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json.get("status").is_some());
    assert!(json.get("uptime_seconds").is_some());
    assert!(json.get("healthy_components").is_some());
    assert!(json.get("last_check").is_some());
}

#[tokio::test]
async fn test_health_status_with_details() {
    let config = setup_test_config().await;
    let app = create_monitoring_router(config);

    let request = Request::builder()
        .method("GET")
        .uri("/health?include_details=true")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json.get("details").is_some());
    let details = json["details"].as_object().unwrap();
    assert!(details.get("components").is_some());
    assert!(details.get("performance_metrics").is_some());
    assert!(details.get("business_metrics").is_some());
}

#[tokio::test]
async fn test_component_health_endpoint() {
    let config = setup_test_config().await;
    let app = create_monitoring_router(config);

    // Test database component health
    let request = Request::builder()
        .method("GET")
        .uri("/health/component?component=database")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["component"], "database");
    assert!(json.get("status").is_some());
    assert!(json.get("response_time_ms").is_some());
    assert!(json.get("last_checked").is_some());
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let config = setup_test_config().await;
    let app = create_monitoring_router(config);

    let request = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json.get("current_metrics").is_some());
    assert!(json.get("business_metrics").is_some());
    assert!(json.get("alerts").is_some());
    assert!(json.get("timestamp").is_some());

    let current_metrics = &json["current_metrics"];
    assert!(current_metrics.get("requests_per_second").is_some());
    assert!(current_metrics.get("average_response_time_ms").is_some());
    assert!(current_metrics.get("error_rate_percentage").is_some());
}

#[tokio::test]
async fn test_alert_summary_endpoint() {
    let config = setup_test_config().await;
    let app = create_monitoring_router(config);

    let request = Request::builder()
        .method("GET")
        .uri("/alerts/summary")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should return alert counts by severity
    assert!(json.is_object());
}

#[tokio::test]
async fn test_health_monitor_service() {
    let config = setup_test_config().await;
    let metrics_service = Arc::new(MetricsCollectorService::new());
    let alert_service = Arc::new(AlertManagerService::new());
    let health_service = HealthMonitorService::new(&config, metrics_service, alert_service);

    // Test comprehensive health check
    let system_health = health_service.perform_comprehensive_health_check().await.unwrap();
    
    assert!(system_health.components.len() > 0);
    assert!(system_health.system_uptime_seconds >= 0);
    assert!(matches!(
        system_health.overall_status,
        HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy | HealthStatus::Critical
    ));

    // Verify all expected components are checked
    let component_names: Vec<&str> = system_health.components
        .iter()
        .map(|c| c.component.as_str())
        .collect();
    
    assert!(component_names.contains(&"database"));
    assert!(component_names.contains(&"supabase"));
    assert!(component_names.contains(&"memory"));
    assert!(component_names.contains(&"auth_service"));
    assert!(component_names.contains(&"doctor_service"));
    assert!(component_names.contains(&"appointment_service"));

    // Test individual component health checks
    let db_health = health_service.get_component_health("database").await;
    assert!(db_health.is_some());
    
    let db_check = db_health.unwrap();
    assert_eq!(db_check.component, "database");
    assert!(db_check.response_time_ms >= 0);

    // Test invalid component
    let invalid_health = health_service.get_component_health("nonexistent").await;
    assert!(invalid_health.is_none());
}

#[tokio::test]
async fn test_metrics_collector_service() {
    let metrics_service = MetricsCollectorService::new();

    // Test initial state
    let initial_stats = metrics_service.get_current_stats().await;
    assert_eq!(initial_stats.total_requests, 0);
    assert_eq!(initial_stats.total_errors, 0);
    assert_eq!(initial_stats.requests_per_second, 0.0);

    // Record some requests
    metrics_service.record_request(100, false).await;
    metrics_service.record_request(200, false).await;
    metrics_service.record_request(150, true).await; // Error request

    let stats_after = metrics_service.get_current_stats().await;
    assert_eq!(stats_after.total_requests, 3);
    assert_eq!(stats_after.total_errors, 1);
    assert_eq!(stats_after.error_rate_percentage, (1.0 / 3.0) * 100.0);
    assert_eq!(stats_after.average_response_time_ms, (100.0 + 200.0 + 150.0) / 3.0);

    // Test recent response times
    let recent_times = metrics_service.get_recent_response_times(2).await;
    assert_eq!(recent_times.len(), 2);
    assert_eq!(recent_times[0], 150); // Most recent first
    assert_eq!(recent_times[1], 200);

    // Test P95 calculation with more data
    for i in 1..=100 {
        metrics_service.record_request(i, false).await;
    }

    let stats_with_p95 = metrics_service.get_current_stats().await;
    assert!(stats_with_p95.p95_response_time_ms > 0.0);
    assert!(stats_with_p95.total_requests >= 100);

    // Test metrics reset
    metrics_service.reset_metrics();
    let reset_stats = metrics_service.get_current_stats().await;
    assert_eq!(reset_stats.total_requests, 0);
    assert_eq!(reset_stats.total_errors, 0);
}

#[tokio::test]
async fn test_alert_manager_service() {
    let alert_service = AlertManagerService::new();

    // Test initial state
    let initial_alerts = alert_service.get_active_alerts().await;
    assert_eq!(initial_alerts.len(), 0);

    let initial_summary = alert_service.get_alert_summary().await;
    assert_eq!(initial_summary.len(), 0);

    // Create high error rate scenario
    let high_error_metrics = MetricsSnapshot {
        requests_per_second: 100.0,
        average_response_time_ms: 50.0,
        p95_response_time_ms: 200.0,
        error_rate_percentage: 15.0, // Above critical threshold
        total_requests: 1000,
        total_errors: 150,
        uptime_seconds: 3600,
    };

    // Evaluate alerts
    alert_service.evaluate_alerts(&high_error_metrics).await;

    // Check that alerts were triggered
    let alerts_after = alert_service.get_active_alerts().await;
    assert!(alerts_after.len() > 0);

    // Verify alert content
    let critical_alerts: Vec<&Alert> = alerts_after
        .iter()
        .filter(|a| matches!(a.severity, AlertSeverity::Critical))
        .collect();
    assert!(critical_alerts.len() > 0);

    let critical_alert = critical_alerts[0];
    assert!(critical_alert.metric_value >= 10.0); // Critical threshold
    assert_eq!(critical_alert.component, "api");

    // Test alert acknowledgment
    let alert_id = &critical_alert.alert_id;
    let acknowledged = alert_service.acknowledge_alert(alert_id).await;
    assert!(acknowledged);

    // Verify alert was removed
    let alerts_after_ack = alert_service.get_active_alerts().await;
    let remaining_critical = alerts_after_ack
        .iter()
        .filter(|a| a.alert_id == *alert_id)
        .count();
    assert_eq!(remaining_critical, 0);

    // Test high response time scenario
    let slow_response_metrics = MetricsSnapshot {
        requests_per_second: 50.0,
        average_response_time_ms: 1500.0,
        p95_response_time_ms: 2500.0, // Above warning threshold
        error_rate_percentage: 2.0,
        total_requests: 500,
        total_errors: 10,
        uptime_seconds: 3600,
    };

    alert_service.evaluate_alerts(&slow_response_metrics).await;

    let alerts_after_slow = alert_service.get_active_alerts().await;
    let warning_alerts: Vec<&Alert> = alerts_after_slow
        .iter()
        .filter(|a| matches!(a.severity, AlertSeverity::Warning))
        .collect();
    assert!(warning_alerts.len() > 0);

    // Test clear all alerts
    alert_service.clear_all_alerts().await;
    let alerts_after_clear = alert_service.get_active_alerts().await;
    assert_eq!(alerts_after_clear.len(), 0);
}

#[tokio::test]
async fn test_comprehensive_monitoring_workflow() {
    let config = setup_test_config().await;
    let metrics_service = Arc::new(MetricsCollectorService::new());
    let alert_service = Arc::new(AlertManagerService::new());
    let health_service = HealthMonitorService::new(
        &config, 
        metrics_service.clone(), 
        alert_service.clone()
    );

    // Simulate application traffic and monitoring
    
    // 1. Record normal traffic
    for i in 1..=50 {
        let response_time = 50 + (i % 10) * 10; // 50-140ms
        metrics_service.record_request(response_time, false).await;
    }

    let normal_stats = metrics_service.get_current_stats().await;
    assert_eq!(normal_stats.total_requests, 50);
    assert_eq!(normal_stats.error_rate_percentage, 0.0);

    // 2. Evaluate alerts for normal conditions
    alert_service.evaluate_alerts(&normal_stats).await;
    let normal_alerts = alert_service.get_active_alerts().await;
    // Normal conditions might still trigger low RPS alert in fast tests, so we check it's not error-related
    let has_error_alerts = normal_alerts.iter().any(|a| a.title.contains("Error Rate"));
    assert!(!has_error_alerts); // No error rate alerts for normal conditions

    // 3. Perform health check during normal operation
    let system_health = health_service.perform_comprehensive_health_check().await.unwrap();
    assert!(matches!(
        system_health.overall_status,
        HealthStatus::Healthy | HealthStatus::Degraded
    ));

    // 4. Simulate performance degradation
    for i in 1..=20 {
        let response_time = 500 + i * 50; // 550-1500ms (slow)
        let is_error = i % 5 == 0; // 20% error rate
        metrics_service.record_request(response_time, is_error).await;
    }

    let degraded_stats = metrics_service.get_current_stats().await;
    assert!(degraded_stats.error_rate_percentage > 5.0);
    assert!(degraded_stats.average_response_time_ms > 200.0);

    // 5. Check that alerts are triggered
    alert_service.evaluate_alerts(&degraded_stats).await;
    let degraded_alerts = alert_service.get_active_alerts().await;
    assert!(degraded_alerts.len() > 0);

    // 6. Verify alert types
    let has_error_alert = degraded_alerts
        .iter()
        .any(|a| a.title.contains("Error Rate"));
    assert!(has_error_alert);

    // 7. Simulate recovery
    for i in 1..=30 {
        let response_time = 60 + (i % 5) * 5; // Back to 60-80ms
        metrics_service.record_request(response_time, false).await;
    }

    let recovered_stats = metrics_service.get_current_stats().await;
    assert!(recovered_stats.error_rate_percentage < 5.0);
    // Note: Average includes all requests, so recovery may not immediately show < 200ms
    assert!(recovered_stats.average_response_time_ms < degraded_stats.average_response_time_ms);

    // 8. Final health check should show improvement
    let final_health = health_service.perform_comprehensive_health_check().await.unwrap();
    assert!(final_health.performance_metrics.requests_per_second >= 0.0);
    assert!(final_health.business_metrics.total_patients > 0);
}

#[tokio::test]
async fn test_monitoring_handlers_integration() {
    let config = setup_test_config().await;
    let handlers = MonitoringHandlers::new(config);

    // Test that handlers can access metrics service
    let metrics_service = handlers.get_metrics_service();
    
    // Record some test data
    metrics_service.record_request(100, false).await;
    metrics_service.record_request(150, true).await;

    let stats = metrics_service.get_current_stats().await;
    assert_eq!(stats.total_requests, 2);
    assert_eq!(stats.total_errors, 1);
    assert_eq!(stats.error_rate_percentage, 50.0);
}

#[tokio::test]
async fn test_business_metrics_tracking() {
    let config = setup_test_config().await;
    let metrics_service = Arc::new(MetricsCollectorService::new());
    let alert_service = Arc::new(AlertManagerService::new());
    let health_service = HealthMonitorService::new(&config, metrics_service, alert_service);

    let system_health = health_service.perform_comprehensive_health_check().await.unwrap();
    let business_metrics = &system_health.business_metrics;

    // Verify business metrics are populated
    assert!(business_metrics.total_patients > 0);
    assert!(business_metrics.active_doctors > 0);
    assert!(business_metrics.appointments_today >= 0);
    assert!(business_metrics.appointments_this_week >= 0);
    assert!(business_metrics.video_sessions_active >= 0);
    assert!(business_metrics.prescription_requests_pending >= 0);
    assert!(business_metrics.average_appointment_duration_minutes > 0.0);
    assert!(business_metrics.patient_satisfaction_score >= 0.0);
    assert!(business_metrics.patient_satisfaction_score <= 5.0);
}