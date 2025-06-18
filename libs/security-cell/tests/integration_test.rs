// =====================================================================================
// SECURITY CELL INTEGRATION TESTS - PRODUCTION GRADE VALIDATION
// =====================================================================================

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;
use wiremock::{MockServer, Mock, ResponseTemplate};

use security_cell::{
    create_security_router,
    models::{AuditEventType, AuditOutcome, ValidationResult},
    services::{AuditService, ValidationService, SecurityMonitoringService, PasswordSecurityService},
};
use shared_config::AppConfig;

async fn setup_test_config() -> Arc<AppConfig> {
    Arc::new(AppConfig {
        supabase_url: "https://test.supabase.co".to_string(),
        supabase_anon_key: "test_anon_key".to_string(),
        supabase_jwt_secret: "test_jwt_secret_for_validation_testing_purposes_only".to_string(),
        cloudflare_realtime_app_id: "test_app_id".to_string(),
        cloudflare_realtime_api_token: "test_api_token".to_string(),
        cloudflare_realtime_base_url: "https://test.cloudflare.com".to_string(),
    })
}

#[tokio::test]
async fn test_security_health_endpoint() {
    let config = setup_test_config().await;
    let app = create_security_router(config);

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
    assert!(json.get("blocked_ips").is_some());
    assert!(json.get("failed_login_attempts").is_some());
}

#[tokio::test]
async fn test_input_validation_endpoint() {
    let config = setup_test_config().await;
    let app = create_security_router(config);

    // Test SQL injection attempt
    let payload = json!({
        "field_name": "username",
        "value": "admin'; DROP TABLE users; --"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/validate")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["is_valid"], false);
    assert!(json["risk_score"].as_u64().unwrap() > 30);
    assert!(json["issues"].as_array().unwrap().len() > 0);
}

#[tokio::test]
async fn test_password_validation_endpoint() {
    let config = setup_test_config().await;
    
    // Test weak password
    let app1 = create_security_router(config.clone());
    let weak_payload = json!({
        "password": "123"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/password/validate")
        .header("content-type", "application/json")
        .body(Body::from(weak_payload.to_string()))
        .unwrap();

    let response = app1.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["requirements_met"], false);
    assert!(json["suggestions"].as_array().unwrap().len() > 0);

    // Test strong password
    let app2 = create_security_router(config);
    let strong_payload = json!({
        "password": "MyStr0ng!P@ssw0rd2024#"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/password/validate")
        .header("content-type", "application/json")
        .body(Body::from(strong_payload.to_string()))
        .unwrap();

    let response = app2.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["requirements_met"], true);
    assert!(json["score"].as_u64().unwrap() >= 60);
}

#[tokio::test]
async fn test_audit_service_functionality() {
    let config = setup_test_config().await;
    let audit_service = AuditService::new(&config);

    // Test audit entry creation and logging
    let entry = security_cell::models::AuditEntry::new(
        AuditEventType::LoginSuccess,
        "Test login event".to_string(),
        AuditOutcome::Success,
    )
    .with_user("test_user_123".to_string())
    .with_risk_score(10);

    let result = audit_service.log_audit_entry(entry).await;
    assert!(result.is_ok());

    // Test failed authentication logging
    let result = audit_service.log_failed_authentication(
        Some("test@example.com"),
        "192.168.1.100",
        "Invalid password"
    ).await;
    assert!(result.is_ok());

    // Test successful authentication logging  
    let result = audit_service.log_successful_authentication(
        "user_456",
        "192.168.1.101", 
        "session_789"
    ).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validation_service_comprehensive() {
    let validation_service = ValidationService::with_default_config();

    // Test SQL injection detection
    let sql_result = validation_service.validate_input(
        "SELECT * FROM users WHERE id = 1 OR 1=1",
        "search_query"
    );
    assert!(!sql_result.is_valid);
    assert!(sql_result.risk_score >= 50);

    // Test XSS detection
    let xss_result = validation_service.validate_input(
        "<script>alert('xss')</script>",
        "comment"
    );
    assert!(!xss_result.is_valid);
    assert!(xss_result.risk_score >= 40);

    // Test email validation
    assert!(validation_service.validate_email("test@example.com"));
    assert!(!validation_service.validate_email("invalid-email"));

    // Test phone validation
    assert!(validation_service.validate_phone("+1234567890"));
    assert!(!validation_service.validate_phone("abc123"));

    // Test UUID validation
    assert!(validation_service.validate_uuid("550e8400-e29b-41d4-a716-446655440000"));
    assert!(!validation_service.validate_uuid("not-a-uuid"));

    // Test medical ID validation
    let medical_result = validation_service.validate_medical_id("MED-12345-ABC");
    assert!(medical_result.is_valid);

    let invalid_medical = validation_service.validate_medical_id("invalid!@#");
    assert!(!invalid_medical.is_valid);
}

#[tokio::test]
async fn test_security_monitoring_service() {
    let config = setup_test_config().await;
    let audit_service = Arc::new(AuditService::new(&config));
    let monitoring_service = SecurityMonitoringService::new(audit_service);

    // Test failed login tracking
    let should_block = monitoring_service.record_failed_login(
        "192.168.1.200",
        Some("attacker@example.com")
    ).await.unwrap();
    assert!(!should_block); // First attempt shouldn't block

    // Simulate multiple failed attempts
    for _ in 0..3 {
        let _ = monitoring_service.record_failed_login(
            "192.168.1.200",
            Some("attacker@example.com")
        ).await;
    }

    // Check if IP is blocked after multiple attempts
    let is_blocked = monitoring_service.is_ip_blocked("192.168.1.200").await;
    assert!(is_blocked);

    // Test suspicious activity detection
    let risk_score = monitoring_service.detect_suspicious_patterns(
        "user_123",
        "192.168.1.201"
    ).await;
    assert!(risk_score >= 0);

    // Test manual IP blocking
    let result = monitoring_service.manually_block_ip(
        "192.168.1.202",
        "Manual security test",
        24
    ).await;
    assert!(result.is_ok());

    let is_manually_blocked = monitoring_service.is_ip_blocked("192.168.1.202").await;
    assert!(is_manually_blocked);
}

#[tokio::test]
async fn test_password_security_service() {
    // Test password hashing and verification
    let password = "TestPassword123!";
    let hash = PasswordSecurityService::hash_password(password).unwrap();
    
    // Verify correct password
    let is_valid = PasswordSecurityService::verify_password(password, &hash).unwrap();
    assert!(is_valid);

    // Verify incorrect password
    let is_invalid = PasswordSecurityService::verify_password("WrongPassword", &hash).unwrap();
    assert!(!is_invalid);

    // Test password strength validation
    let weak_result = PasswordSecurityService::validate_password_strength("123");
    assert!(matches!(weak_result.strength, security_cell::models::PasswordStrength::Weak));

    let strong_result = PasswordSecurityService::validate_password_strength("MyVeryStr0ng!P@ssw0rd2024#");
    assert!(matches!(strong_result.strength, security_cell::models::PasswordStrength::Strong));

    // Test medical professional password requirements
    let doctor_result = PasswordSecurityService::validate_medical_professional_password(
        "DoctorSecure!2024#Pass",
        "doctor"
    );
    assert!(doctor_result.score >= 60);

    // Test password generation
    let generated = PasswordSecurityService::generate_secure_password(16);
    assert_eq!(generated.len(), 16);
    
    let generated_result = PasswordSecurityService::validate_password_strength(&generated);
    assert!(generated_result.score >= 75);

    // Test breach checking
    let is_breached = PasswordSecurityService::check_password_breaches("123456");
    assert!(is_breached);

    let not_breached = PasswordSecurityService::check_password_breaches("UniqueSecurePassword!2024#");
    assert!(!not_breached);
}

#[tokio::test]
async fn test_comprehensive_security_workflow() {
    let config = setup_test_config().await;
    let audit_service = Arc::new(AuditService::new(&config));
    let monitoring_service = Arc::new(SecurityMonitoringService::new(audit_service.clone()));
    let validation_service = ValidationService::with_default_config();

    // Simulate a complete security workflow
    
    // 1. Validate user input
    let user_input = "alice";
    let validation_result = validation_service.validate_input(user_input, "username");
    if !validation_result.is_valid {
        println!("Validation failed for '{}': {:?}", user_input, validation_result);
    }
    assert!(validation_result.is_valid);

    // 2. Attempt login with valid credentials
    let login_audit = security_cell::models::AuditEntry::new(
        AuditEventType::LoginSuccess,
        "User login attempt".to_string(),
        AuditOutcome::Success,
    )
    .with_user("test_user".to_string())
    .with_risk_score(5);

    audit_service.log_audit_entry(login_audit).await.unwrap();

    // 3. Detect no suspicious activity for normal user
    let risk_score = monitoring_service.detect_suspicious_patterns(
        "test_user",
        "192.168.1.100"
    ).await;
    assert!(risk_score < 30); // Low risk for normal activity

    // 4. Simulate malicious activity
    let malicious_input = "'; DROP TABLE users; --";
    let malicious_validation = validation_service.validate_input(malicious_input, "search");
    assert!(!malicious_validation.is_valid);
    assert!(malicious_validation.risk_score >= 50);

    // 5. Log security violation
    let security_audit = security_cell::models::AuditEntry::new(
        AuditEventType::SqlInjectionAttempt,
        "SQL injection detected".to_string(),
        AuditOutcome::Denied,
    )
    .with_risk_score(malicious_validation.risk_score)
    .add_context("ip_address", "192.168.1.999")
    .add_context("user_agent", "AttackBot/1.0");

    audit_service.log_audit_entry(security_audit).await.unwrap();

    // 6. Block malicious IP after multiple attempts
    for _ in 0..5 {
        monitoring_service.record_failed_login("192.168.1.999", None).await.unwrap();
    }

    let is_blocked = monitoring_service.is_ip_blocked("192.168.1.999").await;
    assert!(is_blocked);
}