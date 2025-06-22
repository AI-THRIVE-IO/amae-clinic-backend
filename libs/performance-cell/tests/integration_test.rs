// =====================================================================================
// PERFORMANCE CELL INTEGRATION TESTS - BASIC VALIDATION
// =====================================================================================

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tower::ServiceExt;

use performance_cell::{
    create_performance_router,
    models::{CacheStats, PerformanceStats},
    services::CacheService,
    handlers::PerformanceHandlers,
};
use shared_config::AppConfig;

async fn setup_test_config() -> Arc<AppConfig> {
    Arc::new(AppConfig {
        supabase_url: "https://test.supabase.co".to_string(),
        supabase_anon_key: "test_anon_key".to_string(),
        supabase_jwt_secret: "test_jwt_secret_for_performance_tests".to_string(),
        cloudflare_realtime_app_id: "test_app_id".to_string(),
        cloudflare_realtime_api_token: "test_api_token".to_string(),
        cloudflare_realtime_base_url: "https://test.cloudflare.com".to_string(),
        redis_url: Some("redis://localhost:6379".to_string()),
    })
}

#[tokio::test]
async fn test_performance_stats_endpoint() {
    let config = setup_test_config().await;
    let app = create_performance_router(config);

    let request = Request::builder()
        .method("GET")
        .uri("/stats")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json.get("cache_stats").is_some());
    assert!(json.get("request_count").is_some());
    assert!(json.get("average_response_time_ms").is_some());
}

#[tokio::test]
async fn test_cache_service_functionality() {
    let cache_service = CacheService::new();

    // Test cache stats
    let stats = cache_service.get_cache_stats().await;
    assert!(stats.hit_rate >= 0.0);
    assert!(stats.hit_rate <= 1.0);
    assert!(stats.total_entries > 0);
    assert!(stats.memory_usage_mb > 0.0);

    // Test performance stats
    let perf_stats = cache_service.get_performance_stats().await;
    assert!(perf_stats.request_count > 0);
    assert!(perf_stats.average_response_time_ms > 0.0);
    assert_eq!(perf_stats.cache_stats.hit_rate, stats.hit_rate);
}

#[tokio::test]
async fn test_performance_handlers_creation() {
    let handlers = PerformanceHandlers::new();
    // Basic test to ensure handlers can be created without errors
    // In a more complete implementation, we would test handler methods
}

#[tokio::test]
async fn test_performance_router_creation() {
    let config = setup_test_config().await;
    let router = create_performance_router(config);
    
    // Test that router can be created without panicking
    // In the actual implementation, this router would have endpoints
    
    // Test a basic request to verify router is functional
    let request = Request::builder()
        .method("GET")
        .uri("/nonexistent")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    // Should return 404 for non-existent endpoint, which means router is working
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}