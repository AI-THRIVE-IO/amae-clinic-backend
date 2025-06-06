use std::sync::Arc;
use axum::{extract::State, http::{HeaderMap, HeaderValue}};
use axum_extra::TypedHeader;
use headers::Authorization;
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

use auth_cell::handlers::{validate_token, verify_token, get_profile};
use shared_config::AppConfig;
use shared_models::error::AppError;
use shared_utils::test_utils::{TestConfig, TestUser, JwtTestUtils, MockSupabaseResponses};

fn create_test_config() -> AppConfig {
    TestConfig::default().to_app_config()
}

fn create_auth_header(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers
}

#[tokio::test]
async fn test_extract_bearer_token_success() {
    let config = Arc::new(create_test_config());
    let user = TestUser::default();
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let headers = create_auth_header(&token);

    let result = validate_token(State(config), headers).await;
    
    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response.valid, true);
    assert_eq!(response.user_id, user.id);
    assert_eq!(response.email, Some(user.email));
    assert_eq!(response.role, Some(user.role));
}

#[tokio::test]
async fn test_extract_bearer_token_missing_header() {
    let config = Arc::new(create_test_config());
    let headers = HeaderMap::new();

    let result = validate_token(State(config), headers).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(msg) => assert_eq!(msg, "Missing authorization header"),
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_extract_bearer_token_invalid_format() {
    let config = Arc::new(create_test_config());
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Invalid Token"));

    let result = validate_token(State(config), headers).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(msg) => assert_eq!(msg, "Invalid authorization header format"),
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_extract_bearer_token_no_bearer_prefix() {
    let config = Arc::new(create_test_config());
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("sometoken"));

    let result = validate_token(State(config), headers).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(msg) => assert_eq!(msg, "Invalid authorization header format"),
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_validate_token_success() {
    let config = Arc::new(create_test_config());
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let headers = create_auth_header(&token);

    let result = validate_token(State(config), headers).await;
    
    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response.valid, true);
    assert_eq!(response.user_id, user.id);
    assert_eq!(response.email, Some(user.email));
    assert_eq!(response.role, Some(user.role));
}

#[tokio::test]
async fn test_validate_token_expired() {
    let config = Arc::new(create_test_config());
    let user = TestUser::default();
    let token = JwtTestUtils::create_expired_token(&user, &config.supabase_jwt_secret);
    let headers = create_auth_header(&token);

    let result = validate_token(State(config), headers).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(_) => {}, // Expected
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_validate_token_invalid_signature() {
    let config = Arc::new(create_test_config());
    let user = TestUser::default();
    let token = JwtTestUtils::create_invalid_signature_token(&user);
    let headers = create_auth_header(&token);

    let result = validate_token(State(config), headers).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(_) => {}, // Expected
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_validate_token_malformed() {
    let config = Arc::new(create_test_config());
    let token = JwtTestUtils::create_malformed_token();
    let headers = create_auth_header(&token);

    let result = validate_token(State(config), headers).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(_) => {}, // Expected
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_verify_token_valid() {
    let config = Arc::new(create_test_config());
    let user = TestUser::doctor("doctor@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let headers = create_auth_header(&token);

    let result = verify_token(State(config), headers).await;
    
    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response["valid"], true);
}

#[tokio::test]
async fn test_verify_token_invalid() {
    let config = Arc::new(create_test_config());
    let user = TestUser::default();
    let token = JwtTestUtils::create_expired_token(&user, &config.supabase_jwt_secret);
    let headers = create_auth_header(&token);

    let result = verify_token(State(config), headers).await;
    
    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response["valid"], false);
}

#[tokio::test]
async fn test_get_profile_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock auth profile response
    Mock::given(method("GET"))
        .and(path(format!("/rest/v1/auth_profiles")))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::user_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;
    
    // Mock health profile response
    Mock::given(method("GET"))
        .and(path(format!("/rest/v1/health_profiles")))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::health_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;

    let auth_header = Authorization::bearer(&token).unwrap();
    let typed_header = TypedHeader(auth_header);

    let result = get_profile(State(Arc::new(config)), typed_header).await;
    
    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response["user_id"], user.id);
    assert!(response["auth_profile"].is_array());
    assert!(response["health_profile"].is_array());
}

#[tokio::test]
async fn test_get_profile_invalid_token() {
    let config = Arc::new(create_test_config());
    let user = TestUser::default();
    let token = JwtTestUtils::create_expired_token(&user, &config.supabase_jwt_secret);
    
    let auth_header = Authorization::bearer(&token).unwrap();
    let typed_header = TypedHeader(auth_header);

    let result = get_profile(State(config), typed_header).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(_) => {}, // Expected
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_get_profile_supabase_error() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::default();
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock error response from Supabase
    Mock::given(method("GET"))
        .and(path(format!("/rest/v1/auth_profiles")))
        .respond_with(ResponseTemplate::new(500).set_body_json(
            MockSupabaseResponses::error_response("Internal server error", "INTERNAL_ERROR")
        ))
        .mount(&mock_server)
        .await;

    let auth_header = Authorization::bearer(&token).unwrap();
    let typed_header = TypedHeader(auth_header);

    let result = get_profile(State(Arc::new(config)), typed_header).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::ExternalService(_) => {}, // Expected
        _ => panic!("Expected ExternalService error"),
    }
}

#[tokio::test]
async fn test_different_user_roles() {
    let config = Arc::new(create_test_config());
    
    // Test patient role
    let patient = TestUser::patient("patient@test.com");
    let patient_token = JwtTestUtils::create_test_token(&patient, &config.supabase_jwt_secret, Some(24));
    let patient_headers = create_auth_header(&patient_token);
    
    let patient_result = validate_token(State(config.clone()), patient_headers).await;
    assert!(patient_result.is_ok());
    let patient_response = patient_result.unwrap().0;
    assert_eq!(patient_response.role, Some("patient".to_string()));
    
    // Test doctor role
    let doctor = TestUser::doctor("doctor@test.com");
    let doctor_token = JwtTestUtils::create_test_token(&doctor, &config.supabase_jwt_secret, Some(24));
    let doctor_headers = create_auth_header(&doctor_token);
    
    let doctor_result = validate_token(State(config.clone()), doctor_headers).await;
    assert!(doctor_result.is_ok());
    let doctor_response = doctor_result.unwrap().0;
    assert_eq!(doctor_response.role, Some("doctor".to_string()));
    
    // Test admin role
    let admin = TestUser::admin("admin@test.com");
    let admin_token = JwtTestUtils::create_test_token(&admin, &config.supabase_jwt_secret, Some(24));
    let admin_headers = create_auth_header(&admin_token);
    
    let admin_result = validate_token(State(config), admin_headers).await;
    assert!(admin_result.is_ok());
    let admin_response = admin_result.unwrap().0;
    assert_eq!(admin_response.role, Some("admin".to_string()));
}

#[tokio::test]
async fn test_edge_cases() {
    let config = Arc::new(create_test_config());
    
    // Test with very long token (should still work if valid)
    let user = TestUser::new("user@example.com", "test_role_with_very_long_name");
    let long_token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let headers = create_auth_header(&long_token);
    
    let result = validate_token(State(config.clone()), headers).await;
    assert!(result.is_ok());
    
    // Test with empty user email in token
    let empty_email_user = TestUser::new("", "patient");
    let empty_email_token = JwtTestUtils::create_test_token(&empty_email_user, &config.supabase_jwt_secret, Some(24));
    let empty_email_headers = create_auth_header(&empty_email_token);
    
    let empty_email_result = validate_token(State(config), empty_email_headers).await;
    assert!(empty_email_result.is_ok());
    let empty_email_response = empty_email_result.unwrap().0;
    assert_eq!(empty_email_response.email, Some("".to_string()));
}