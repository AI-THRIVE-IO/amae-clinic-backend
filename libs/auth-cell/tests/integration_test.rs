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
    
    // Mock auth profile response
    Mock::given(method("GET"))
        .and(path("/rest/v1/auth_profiles"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::user_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;
    
    // Mock health profile response
    Mock::given(method("GET"))
        .and(path("/rest/v1/health_profiles"))
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
    assert!(json_response["auth_profile"].is_array());
    assert!(json_response["health_profile"].is_array());
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
async fn test_get_profile_endpoint_invalid_token() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config.clone()).await;
    
    let user = TestUser::default();
    let token = JwtTestUtils::create_expired_token(&user, &config.supabase_jwt_secret);
    
    let request = Request::builder()
        .method("POST")
        .uri("/profile")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_cors_and_middleware() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config.clone()).await;
    
    let user = TestUser::default();
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    let request = Request::builder()
        .method("POST")
        .uri("/validate")
        .header("authorization", format!("Bearer {}", token))
        .header("origin", "https://example.com")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_multiple_sequential_requests() {
    let config = TestConfig::default().to_app_config();
    
    let user = TestUser::admin("admin@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Test multiple sequential requests to validate token
    for i in 0..5 {
        let app = create_test_app(config.clone()).await;
        
        let request = Request::builder()
            .method("POST")
            .uri("/validate")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        
        assert_eq!(response.status(), StatusCode::OK, "Request {} failed", i);
        
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        
        assert_eq!(json_response["valid"], true);
        assert_eq!(json_response["user_id"], user.id);
    }
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

#[tokio::test]
async fn test_nonexistent_routes() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(config).await;
    
    // Test nonexistent route
    let request = Request::builder()
        .method("POST")
        .uri("/nonexistent")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}