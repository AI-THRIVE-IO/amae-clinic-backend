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

use health_profile_cell::router::health_profile_routes;
use health_profile_cell::models::CreateHealthProfileRequest;
use shared_config::AppConfig;
use shared_utils::test_utils::{TestConfig, TestUser, JwtTestUtils, MockSupabaseResponses};

async fn create_test_app(config: &AppConfig) -> Router {
    health_profile_routes(Arc::new(config.clone()))
}

#[tokio::test]
async fn test_create_health_profile_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(&config).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock Supabase insert response
    Mock::given(method("POST"))
        .and(path("/rest/v1/health_profiles"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            MockSupabaseResponses::health_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;

    let request_body = CreateHealthProfileRequest {
        patient_id: user.id.clone(),
        is_pregnant: Some(false),
        is_breastfeeding: Some(false),
        reproductive_stage: Some("premenopause".to_string()),
    };

    let request = Request::builder()
        .method("POST")
        .uri("/health-profiles")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_create_health_profile_unauthorized() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(&config).await;
    
    let request_body = CreateHealthProfileRequest {
        patient_id: "test-id".to_string(),
        is_pregnant: None,
        is_breastfeeding: None,
        reproductive_stage: None,
    };

    let request = Request::builder()
        .method("POST")
        .uri("/health-profiles")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_health_profile_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(&config).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock Supabase get response
    Mock::given(method("GET"))
        .and(path("/rest/v1/health_profiles"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::health_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .method("GET")
        .uri(&format!("/health-profiles/{}", user.id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_update_health_profile_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(&config).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock Supabase update response
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/health_profiles"))
        .and(header("Authorization", format!("Bearer {}", token)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::health_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;

    let update_body = json!({
        "blood_type": "O+",
        "height_cm": 175,
        "weight_kg": 70
    });

    let request = Request::builder()
        .method("PUT")
        .uri(&format!("/health-profiles/{}", user.id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(update_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_avatar_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(&config).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock file upload response
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/avatars/test-file"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Key": "avatars/test-file",
            "Id": "test-file-id"
        })))
        .mount(&mock_server)
        .await;

    let avatar_data = json!({
        "file_data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="
    });

    let request = Request::builder()
        .method("POST")
        .uri(&format!("/health-profiles/{}/avatar", user.id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(avatar_data.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_generate_nutrition_plan_success() {
    let mock_server = MockServer::start().await;
    
    let user = TestUser::patient("patient@example.com");
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let app = create_test_app(&config).await;
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    
    // Mock health profile fetch
    Mock::given(method("GET"))
        .and(path("/rest/v1/health_profiles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            MockSupabaseResponses::health_profile_response(&user.id)
        ])))
        .mount(&mock_server)
        .await;

    let nutrition_request = json!({
        "patient_id": user.id
    });

    let request = Request::builder()
        .method("POST")
        .uri(&format!("/health-profiles/{}/ai/nutrition-plan", user.id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(nutrition_request.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_unauthorized_requests() {
    let config = TestConfig::default().to_app_config();
    let app = create_test_app(&config).await;
    
    let test_cases = vec![
        ("GET", "/health-profiles/test-id"),
        ("PUT", "/health-profiles/test-id"),
        ("POST", "/health-profiles"),
        ("DELETE", "/health-profiles/test-id"),
        ("POST", "/health-profiles/test-id/avatar"),
        ("GET", "/health-profiles/test-id/documents"),
        ("POST", "/health-profiles/test-id/ai/nutrition-plan"),
    ];

    for (method, uri) in test_cases {
        let app = create_test_app(&config.clone()).await;
        
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
    let app = create_test_app(&config).await;
    
    let invalid_token = "invalid.token.here";

    let request = Request::builder()
        .method("GET")
        .uri("/health-profiles/test-id")
        .header("authorization", format!("Bearer {}", invalid_token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}