// libs/doctor-cell/tests/integration_test.rs - CRITICAL FIXES

use uuid::Uuid;
use std::sync::Arc;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use tower::ServiceExt;
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, query_param};

use doctor_cell::router::doctor_routes;
use shared_config::AppConfig;
use shared_utils::test_utils::TestUser;

async fn create_test_app(config: AppConfig) -> Router {
    doctor_routes(Arc::new(config))
}

// FIXED: Create proper JWT token for tests
fn create_valid_jwt_token(user: &TestUser, secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    
    let header = json!({"alg": "HS256", "typ": "JWT"});
    let payload = json!({
        "sub": user.id,
        "email": user.email,
        "role": user.role,
        "iat": chrono::Utc::now().timestamp() as u64,
        "exp": (chrono::Utc::now() + chrono::Duration::hours(24)).timestamp() as u64
    });
    
    // CRITICAL: Encode binary data, not JSON strings
    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
    let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
    let signature_input = format!("{}.{}", header_b64, payload_b64);
    
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signature_input.as_bytes());
    let signature = mac.finalize().into_bytes();
    let signature_b64 = URL_SAFE_NO_PAD.encode(&signature);
    
    format!("{}.{}.{}", header_b64, payload_b64, signature_b64)
}

// FIXED: Comprehensive mock setup
async fn setup_all_mocks(mock_server: &MockServer, doctor_id: &str) {
    // Mock all required Supabase endpoints
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": doctor_id,
                "full_name": "Dr. Test Doctor",
                "email": "doctor@example.com",
                "specialty": "General Medicine",
                "bio": "Experienced physician",
                "years_experience": 10,
                "rating": 4.5,
                "total_consultations": 150,
                "is_available": true,
                "is_verified": true,
                "created_at": "2024-01-01T00:00:00Z"
            }
        ])))
        .mount(mock_server)
        .await;

    // Mock doctor creation
    Mock::given(method("POST"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            {
                "id": Uuid::new_v4().to_string(),
                "full_name": "Dr. New Doctor",
                "email": "newdoc@example.com",
                "specialty": "General Medicine",
                "bio": "New physician",
                "years_experience": 5,
                "is_verified": false,
                "is_available": true,
                "created_at": "2024-01-01T00:00:00Z"
            }
        ])))
        .mount(mock_server)
        .await;

    // Mock doctor updates  
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": doctor_id,
                "full_name": "Dr. Updated",
                "email": "updated@example.com",
                "specialty": "General Medicine",
                "is_verified": true,
                "updated_at": "2024-01-01T00:00:00Z"
            }
        ])))
        .mount(mock_server)
        .await;

    // Mock availability operations
    Mock::given(method("POST"))
        .and(path("/rest/v1/appointment_availabilities"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            {
                "id": Uuid::new_v4().to_string(),
                "doctor_id": doctor_id,
                "day_of_week": 1,
                "start_time": "09:00:00",
                "end_time": "17:00:00",
                "duration_minutes": 30,
                "is_available": true,
                "appointment_type": "consultation",
                "created_at": "2024-01-01T00:00:00Z"
            }
        ])))
        .mount(mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/rest/v1/appointment_availabilities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": Uuid::new_v4().to_string(),
                "doctor_id": doctor_id,
                "day_of_week": 1,
                "start_time": "09:00:00",
                "end_time": "17:00:00",
                "duration_minutes": 30,
                "is_available": true,
                "appointment_type": "consultation"
            }
        ])))
        .mount(mock_server)
        .await;

    // Mock patient lookup
    Mock::given(method("GET"))
        .and(path("/rest/v1/patients"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": Uuid::new_v4().to_string(),
                "full_name": "Test Patient",
                "email": "patient@example.com",
                "created_at": "2024-01-01T00:00:00Z"
            }
        ])))
        .mount(mock_server)
        .await;

    // Mock appointments
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(mock_server)
        .await;
}

// ================================================================================================
// FIXED TESTS
// ================================================================================================

#[tokio::test]
async fn test_search_doctors_public() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    let app = create_test_app(config).await;

    let request = Request::builder()
        .method("GET")
        .uri("/search?specialty=cardiology")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_doctor_public() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    let app = create_test_app(config).await;

    let request = Request::builder()
        .method("GET")
        .uri("/doctor-123")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_doctor_availability_public() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    let app = create_test_app(config).await;

    let request = Request::builder()
        .method("GET")
        .uri("/doctor-123/availability")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_doctor_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    
    // Mock email check
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .and(query_param("email", "eq.newdoc@example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock_server)
        .await;
    
    let app = create_test_app(config.clone()).await;
    let admin_user = TestUser::admin("admin@example.com");
    let token = create_valid_jwt_token(&admin_user, &config.supabase_jwt_secret);

    let request_body = json!({
        "full_name": "Dr. New Doctor",
        "email": "newdoc@example.com",
        "specialty": "General Medicine",
        "timezone": "UTC"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_update_doctor_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    let app = create_test_app(config.clone()).await;
    
    let doctor_user = TestUser {
        id: "doctor-123".to_string(),
        email: "doctor@example.com".to_string(),
        role: "doctor".to_string(),
    };
    let token = create_valid_jwt_token(&doctor_user, &config.supabase_jwt_secret);

    let request_body = json!({
        "full_name": "Dr. Updated Name"
    });

    let request = Request::builder()
        .method("PUT")
        .uri("/doctor-123")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_availability_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    
    // Mock conflict check
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointment_availabilities"))
        .and(query_param("doctor_id", "eq.doctor-123"))
        .and(query_param("day_of_week", "eq.1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock_server)
        .await;
    
    let app = create_test_app(config.clone()).await;
    
    let doctor_user = TestUser {
        id: "doctor-123".to_string(),
        email: "doctor@example.com".to_string(),
        role: "doctor".to_string(),
    };
    let token = create_valid_jwt_token(&doctor_user, &config.supabase_jwt_secret);

    let request_body = json!({
        "day_of_week": 1,
        "start_time": "09:00:00",
        "end_time": "17:00:00",
        "duration_minutes": 30,
        "appointment_type": "consultation"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/doctor-123/availability")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_verify_doctor_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    let app = create_test_app(config.clone()).await;
    
    let admin_user = TestUser::admin("admin@example.com");
    let token = create_valid_jwt_token(&admin_user, &config.supabase_jwt_secret);

    let request = Request::builder()
        .method("PATCH")
        .uri("/doctor-123/verify")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_find_matching_doctors_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    let app = create_test_app(config.clone()).await;
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = create_valid_jwt_token(&patient_user, &config.supabase_jwt_secret);

    let request = Request::builder()
        .method("GET")
        .uri("/matching/find?specialty=general")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_doctor_unauthorized() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = create_valid_jwt_token(&patient_user, &config.supabase_jwt_secret);

    let request_body = json!({
        "full_name": "Dr. New Doctor",
        "email": "newdoc@example.com",
        "specialty": "General Medicine",
        "timezone": "UTC"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_protected_endpoints_unauthorized() {
    let config = AppConfig {
        supabase_url: "http://localhost:54321".to_string(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let protected_endpoints = vec![
        ("POST", "/"),
        ("PUT", "/doctor-123"),
        ("PATCH", "/doctor-123/verify"),
        ("POST", "/doctor-123/availability"),
        ("GET", "/matching/find"),
    ];

    for (method, uri) in protected_endpoints {
        let app = create_test_app(config.clone()).await;
        
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
    
    setup_all_mocks(&mock_server, "doctor-123").await;
    
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
        assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    }
}