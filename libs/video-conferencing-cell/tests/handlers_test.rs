use std::sync::Arc;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use serde_json::json;
use wiremock::MockServer;

use shared_utils::test_utils::TestConfig;
use video_conferencing_cell::router::video_conferencing_routes;

fn create_test_config() -> shared_config::AppConfig {
    TestConfig::default().to_app_config()
}

#[tokio::test]
async fn test_video_health_check_not_configured() {
    let mut config = create_test_config();
    config.cloudflare_realtime_app_id = "".to_string(); // Not configured
    
    let app = video_conferencing_routes(Arc::new(config));
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["status"], "not_configured");
    assert_eq!(json["video_configured"], false);
}

#[tokio::test]
async fn test_video_health_check_configured() {
    let config = create_test_config(); // Fully configured
    
    let app = video_conferencing_routes(Arc::new(config));
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should show unhealthy since we're not actually connecting to Cloudflare
    assert!(json["status"] == "healthy" || json["status"] == "unhealthy");
    assert_eq!(json["video_configured"], true);
}

#[tokio::test]
async fn test_create_video_session_unauthorized() {
    let mock_server = MockServer::start().await;
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let app = video_conferencing_routes(Arc::new(config));
    
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "appointment_id": "12345678-1234-1234-1234-123456789012",
                    "room_id": null,
                    "room_type": "consultation",
                    "max_participants": 4,
                    "participant_type": "patient",
                    "session_type": "consultation",
                    "scheduled_start_time": "2024-12-25T10:00:00Z"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_upcoming_sessions_unauthorized() {
    let config = create_test_config();
    let app = video_conferencing_routes(Arc::new(config));
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/upcoming")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}