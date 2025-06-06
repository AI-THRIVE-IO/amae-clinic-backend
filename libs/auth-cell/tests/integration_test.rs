use std::sync::Arc;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use tower::ServiceExt;
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header, query_param};

use auth_cell::router::auth_routes;
use shared_config::AppConfig;
use shared_utils::test_utils::{TestConfig, TestUser, JwtTestUtils, MockSupabaseResponses};

async fn create_test_app(config: AppConfig) -> Router {
    auth_routes(Arc::new(config))
}

#[tokio::test]
async fn test_validate_token_endpoint() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config.clone()).await;
    
    let user = TestUser::patient("test@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    let request = Request::builder()
        .method("POST")
        .uri("/validate")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json_response["valid"], true);
    assert_eq!(json_response["user_id"], user.id);
    assert_eq!(json_response["email"], user.email);
    assert_eq!(json_response["role"], user.role);
}

#[tokio::test]
async fn test_validate_token_endpoint_unauthorized() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config).await;
    
    let request = Request::builder()
        .method("POST")
        .uri("/validate")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_verify_token_endpoint_valid() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config.clone()).await;
    
    let user = TestUser::doctor("doctor@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    let request = Request::builder()
        .method("POST")
        .uri("/verify")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json_response["valid"], true);
}

#[tokio::test]
async fn test_verify_token_endpoint_invalid() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config.clone()).await;
    
    let user = TestUser::default();
    let token = JwtTestUtils::create_expired_token(&user, &config.supabase_jwt_secret);
    
    let request = Request::builder()
        .method("POST")
        .uri("/verify")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json_response["valid"], false);
}

#[tokio::test]
async fn test_get_profile_endpoint_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(config.clone()).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock auth profile response (Supabase auth endpoint)
    Mock::given(method("GET"))
        .and(path("/auth/v1/user"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            MockSupabaseResponses::user_profile_response(&user.id)
        ))
        .mount(&mock_server)
        .await;
    
    // Mock health profile response (with query parameter)
    Mock::given(method("GET"))
        .and(path("/rest/v1/health_profiles"))
        .and(query_param("patient_id", format!("eq.{}", user.id)))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::health_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("POST")
        .uri("/profile")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json_response["user_id"], user.id);
    assert!(json_response["auth_profile"].is_object());
    assert!(!json_response["health_profile"].is_null());
}

#[tokio::test]
async fn test_get_profile_endpoint_unauthorized() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config).await;
    
    let request = Request::builder()
        .method("POST")
        .uri("/profile")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_different_user_roles() {
    let config = TestConfig::default().to_app_config();
    
    // Test patient role
    let patient = TestUser::patient("patient@test.com");
    let patient_token = JwtTestUtils::create_test_token(&patient, &config.supabase_jwt_secret, Some(24));
    let patient_app = create_test_app(config.clone()).await;
    
    let request = Request::builder()
        .method("POST")
        .uri("/validate")
        .header("authorization", format!("Bearer {}", patient_token))
        .body(Body::empty())
        .unwrap();

    let response = patient_app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json_response["role"], "patient");
    
    // Test doctor role
    let doctor = TestUser::doctor("doctor@test.com");
    let doctor_token = JwtTestUtils::create_test_token(&doctor, &config.supabase_jwt_secret, Some(24));
    let doctor_app = create_test_app(config.clone()).await;
    
    let request = Request::builder()
        .method("POST")
        .uri("/validate")
        .header("authorization", format!("Bearer {}", doctor_token))
        .body(Body::empty())
        .unwrap();

    let response = doctor_app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json_response["role"], "doctor");
}

#[tokio::test]
async fn test_malformed_requests() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config).await;
    
    // Test with malformed authorization header
    let request = Request::builder()
        .method("POST")
        .uri("/validate")
        .header("authorization", "InvalidToken")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_unsupported_methods() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config).await;
    
    // Test GET method on POST endpoint
    let request = Request::builder()
        .method("GET")
        .uri("/validate")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}