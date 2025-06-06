use std::sync::Arc;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use tower::ServiceExt;
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};
use chrono::{Utc, Duration, NaiveDate, NaiveTime};
use uuid::Uuid;

use appointment_cell::router::appointment_routes;
use appointment_cell::models::{
    BookAppointmentRequest, SmartBookingRequest, UpdateAppointmentRequest, 
    RescheduleAppointmentRequest, CancelAppointmentRequest,
    AppointmentType, AppointmentStatus, CancelledBy
};
use shared_config::AppConfig;
use shared_utils::test_utils::{TestConfig, TestUser, JwtTestUtils, MockSupabaseResponses};

async fn create_test_app(config: AppConfig) -> Router {
    appointment_routes(Arc::new(config))
}

#[tokio::test]
async fn test_book_appointment_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    let doctor_id = Uuid::new_v4();
    let appointment_time = Utc::now() + Duration::hours(24);
    
    // Mock Supabase appointment insert response
    Mock::given(method("POST"))
        .and(path("/rest/v1/appointments"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            MockSupabaseResponses::appointment_response(&user.id, &doctor_id.to_string())
        ])))
        .mount(&mock_server)
        .await;

    let request_body = BookAppointmentRequest {
        patient_id: Uuid::parse_str(&user.id).unwrap(),
        doctor_id: Some(doctor_id),
        appointment_date: appointment_time,
        appointment_type: AppointmentType::GeneralConsultation,
        duration_minutes: 30,
        timezone: "UTC".to_string(),
        patient_notes: Some("First consultation".to_string()),
        preferred_language: Some("English".to_string()),
        specialty_required: Some("General Practice".to_string()),
    };

    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_smart_book_appointment_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock doctors search response
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::doctor_profile_response("doctor-1"),
            MockSupabaseResponses::doctor_profile_response("doctor-2")
        ])))
        .mount(&mock_server)
        .await;

    // Mock patient history response
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::appointment_response(&user.id, "doctor-1")
        ])))
        .mount(&mock_server)
        .await;

    // Mock appointment creation
    Mock::given(method("POST"))
        .and(path("/rest/v1/appointments"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            MockSupabaseResponses::appointment_response(&user.id, "doctor-1")
        ])))
        .mount(&mock_server)
        .await;

    let request_body = SmartBookingRequest {
        patient_id: Uuid::parse_str(&user.id).unwrap(),
        preferred_date: Some(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap()),
        preferred_time_start: Some(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
        preferred_time_end: Some(NaiveTime::from_hms_opt(16, 0, 0).unwrap()),
        appointment_type: AppointmentType::GeneralConsultation,
        duration_minutes: 30,
        timezone: "UTC".to_string(),
        specialty_required: Some("General Practice".to_string()),
        patient_notes: Some("Smart booking test".to_string()),
        allow_history_prioritization: Some(true),
    };

    let request = Request::builder()
        .method("POST")
        .uri("/smart-book")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_get_appointment_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let appointment_id = Uuid::new_v4();
    
    // Mock Supabase get appointment response
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::appointment_response(&user.id, "doctor-1")
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri(&format!("/{}", appointment_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_update_appointment_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::doctor("doctor@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let appointment_id = Uuid::new_v4();
    
    // Mock Supabase update response
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/appointments"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::appointment_response("patient-1", &user.id)
        ])))
        .mount(&mock_server)
        .await;

    let update_body = UpdateAppointmentRequest {
        status: Some(AppointmentStatus::Completed),
        doctor_notes: Some("Patient responded well to treatment".to_string()),
        patient_notes: None,
        reschedule_to: None,
        reschedule_duration: None,
    };

    let request = Request::builder()
        .method("PUT")
        .uri(&format!("/{}", appointment_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&update_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_reschedule_appointment_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let appointment_id = Uuid::new_v4();
    let new_time = Utc::now() + Duration::hours(48);
    
    // Mock conflict check response (no conflicts)
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock_server)
        .await;

    // Mock appointment update response
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/appointments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::appointment_response(&user.id, "doctor-1")
        ])))
        .mount(&mock_server)
        .await;

    let reschedule_body = RescheduleAppointmentRequest {
        new_start_time: new_time,
        new_duration_minutes: Some(45),
        reason: Some("Schedule conflict".to_string()),
    };

    let request = Request::builder()
        .method("PATCH")
        .uri(&format!("/{}/reschedule", appointment_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&reschedule_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_cancel_appointment_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let appointment_id = Uuid::new_v4();
    
    // Mock appointment cancellation response
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/appointments"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::appointment_response(&user.id, "doctor-1")
        ])))
        .mount(&mock_server)
        .await;

    let cancel_body = CancelAppointmentRequest {
        reason: "Family emergency".to_string(),
        cancelled_by: CancelledBy::Patient,
    };

    let request = Request::builder()
        .method("POST")
        .uri(&format!("/{}/cancel", appointment_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&cancel_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_upcoming_appointments_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock upcoming appointments response
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::appointment_response(&user.id, "doctor-1"),
            MockSupabaseResponses::appointment_response(&user.id, "doctor-2")
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri("/upcoming")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json_response.is_array());
    assert_eq!(json_response.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_check_appointment_conflicts_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::doctor("doctor@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    let start_time = Utc::now() + Duration::hours(24);
    let end_time = start_time + Duration::minutes(30);
    
    // Mock conflict check response (no conflicts)
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri(&format!("/conflicts/check?doctor_id={}&start_time={}&end_time={}", 
            user.id, start_time.to_rfc3339(), end_time.to_rfc3339()))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_appointment_stats_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::admin("admin@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock stats response
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::appointment_response("patient-1", "doctor-1"),
            MockSupabaseResponses::appointment_response("patient-2", "doctor-2")
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri("/stats")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_unauthorized_requests() {
    let config = TestConfig::default().to_app_config();
    
    let protected_endpoints = vec![
        ("POST", "/smart-book"),
        ("POST", "/"),
        ("GET", "/search"),
        ("GET", "/upcoming"),
        ("POST", "/appointment-123/cancel"),
        ("PATCH", "/appointment-123/reschedule"),
        ("GET", "/conflicts/check"),
        ("GET", "/stats"),
    ];

    for (method, uri) in protected_endpoints {
        let app = create_test_app(config.clone()).await;
        
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, 
                  "Failed for {} {}", method, uri);
    }
}

#[tokio::test]
async fn test_invalid_token_requests() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config).await;
    
    let invalid_token = "invalid.token.here";

    let request = Request::builder()
        .method("GET")
        .uri("/upcoming")
        .header("authorization", format!("Bearer {}", invalid_token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}