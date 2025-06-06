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
use chrono::NaiveTime;

use doctor_cell::router::doctor_routes;
use doctor_cell::models::{CreateDoctorRequest, UpdateDoctorRequest, CreateAvailabilityRequest};
use shared_config::AppConfig;
use shared_utils::test_utils::{TestConfig, TestUser, JwtTestUtils, MockSupabaseResponses};

async fn create_test_app(config: AppConfig) -> Router {
    doctor_routes(Arc::new(config))
}

#[tokio::test]
async fn test_search_doctors_public() {
    let mock_server = MockServer::start().await;
    
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    
    // Mock Supabase search response
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::doctor_profile_response("doctor-1"),
            MockSupabaseResponses::doctor_profile_response("doctor-2")
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri("/search?specialty=cardiology&min_rating=4.0")
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
async fn test_get_doctor_public() {
    let mock_server = MockServer::start().await;
    
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let doctor_id = "doctor-123";
    
    // Mock Supabase get doctor response
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::doctor_profile_response(doctor_id)
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri(&format!("/{}", doctor_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_doctor_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::doctor("doctor@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock Supabase insert response
    Mock::given(method("POST"))
        .and(path("/rest/v1/doctors"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            MockSupabaseResponses::doctor_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;

    let request_body = CreateDoctorRequest {
        full_name: "Dr. John Doe".to_string(),
        email: user.email.clone(),
        specialty: "Cardiology".to_string(),
        bio: Some("Experienced cardiologist".to_string()),
        license_number: Some("MD123456".to_string()),
        years_experience: Some(10),
        timezone: "UTC".to_string(),
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
async fn test_create_doctor_unauthorized() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config).await;
    
    let request_body = CreateDoctorRequest {
        full_name: "Dr. John Doe".to_string(),
        email: "doctor@example.com".to_string(),
        specialty: "Cardiology".to_string(),
        bio: None,
        license_number: None,
        years_experience: None,
        timezone: "UTC".to_string(),
    };

    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_update_doctor_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::doctor("doctor@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock Supabase update response
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/doctors"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::doctor_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;

    let update_body = UpdateDoctorRequest {
        full_name: Some("Dr. John Updated".to_string()),
        bio: Some("Updated bio".to_string()),
        specialty: Some("Internal Medicine".to_string()),
        years_experience: Some(12),
        timezone: Some("EST".to_string()),
        is_available: Some(true),
    };

    let request = Request::builder()
        .method("PUT")
        .uri(&format!("/{}", user.id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&update_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_availability_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::doctor("doctor@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock Supabase insert response
    Mock::given(method("POST"))
        .and(path("/rest/v1/doctor_availability"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([{
            "id": "availability-123",
            "doctor_id": user.id,
            "day_of_week": 1,
            "start_time": "09:00:00",
            "end_time": "17:00:00",
            "duration_minutes": 30,
            "timezone": "UTC",
            "appointment_type": "consultation",
            "buffer_minutes": 10,
            "max_concurrent_appointments": 1,
            "is_recurring": true,
            "specific_date": null,
            "is_available": true,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }])))
        .mount(&mock_server)
        .await;

    let availability_request = CreateAvailabilityRequest {
        day_of_week: 1, // Monday
        start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        end_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        duration_minutes: 30,
        timezone: "UTC".to_string(),
        appointment_type: "consultation".to_string(),
        buffer_minutes: Some(10),
        max_concurrent_appointments: Some(1),
        is_recurring: Some(true),
        specific_date: None,
    };

    let request = Request::builder()
        .method("POST")
        .uri(&format!("/{}/availability", user.id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&availability_request).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_get_doctor_availability_public() {
    let mock_server = MockServer::start().await;
    
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let doctor_id = "doctor-123";
    
    // Mock Supabase availability response
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctor_availability"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": "availability-123",
            "doctor_id": doctor_id,
            "day_of_week": 1,
            "start_time": "09:00:00",
            "end_time": "17:00:00",
            "duration_minutes": 30,
            "timezone": "UTC",
            "appointment_type": "consultation",
            "is_available": true,
            "created_at": "2024-01-01T00:00:00Z"
        }])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri(&format!("/{}/availability", doctor_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_find_matching_doctors_success() {
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

    // Mock appointments history for patient
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::appointment_response(&user.id, "doctor-1")
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri(&format!("/matching/find?patient_id={}&specialty_required=cardiology&appointment_type=consultation&duration_minutes=30&timezone=UTC", user.id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_verify_doctor_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::admin("admin@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = "doctor-123";
    
    // Mock Supabase update response
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/doctors"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::doctor_profile_response(doctor_id)
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("PATCH")
        .uri(&format!("/{}/verify", doctor_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from("{\"is_verified\": true}"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_protected_endpoints_unauthorized() {
    let config = TestConfig::default().to_app_config();
    
    let protected_endpoints = vec![
        ("POST", "/"),
        ("PUT", "/doctor-123"),
        ("PATCH", "/doctor-123/verify"),
        ("GET", "/doctor-123/stats"),
        ("POST", "/doctor-123/availability"),
        ("POST", "/doctor-123/specialties"),
        ("GET", "/matching/find"),
        ("POST", "/matching/best"),
        ("GET", "/recommendations"),
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
async fn test_public_endpoints_accessible() {
    let mock_server = MockServer::start().await;
    
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    // Mock responses for all public endpoints
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock_server)
        .await;

    let public_endpoints = vec![
        "/search",
        "/doctor-123",
        "/doctor-123/specialties",
        "/doctor-123/availability",
        "/doctor-123/available-slots",
    ];

    for uri in public_endpoints {
        let app = create_test_app(config.clone()).await;
        
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // Should not be unauthorized (could be 200, 404, or other valid status)
        assert_ne!(response.status(), StatusCode::UNAUTHORIZED, 
                  "Public endpoint {} should be accessible", uri);
    }
}