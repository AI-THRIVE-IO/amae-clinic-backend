use std::net::SocketAddr;
use std::sync::Arc;
use dotenv::dotenv;
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::{self, TraceLayer};
use tracing::{Level, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod router;

use shared_config::AppConfig;

#[tokio::main]
async fn main() {
    // Loading Env Vars
    dotenv().ok();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    info!("Starting Amae Clinic API server");
    
    // Load configuration
    let config = AppConfig::from_env();
    
    // Set up CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    
    // Create shared state
    let state = Arc::new(config);
    
    // Start booking queue consumer service
    let consumer_result = start_booking_consumer(Arc::clone(&state)).await;
    if let Err(e) = &consumer_result {
        tracing::warn!("Failed to start booking queue consumer: {}. Async booking will be unavailable.", e);
    }
    
    // Build the application router
    let app = router::create_router(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new()
                    .level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new()
                    .level(Level::INFO)),
        )
        .layer(cors);
    
    // Run the server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Listening on {}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    
    // Graceful shutdown handling
    let server = axum::serve(listener, app);
    
    tokio::select! {
        result = server => {
            if let Err(e) = result {
                tracing::error!("Server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal, shutting down gracefully...");
        }
    }
    
    info!("Amae Clinic API server shutdown complete");
}

async fn start_booking_consumer(config: Arc<AppConfig>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use booking_queue_cell::{BookingConsumerService, WorkerConfig};
    
    // Check if Redis URL is configured
    if config.redis_url.is_none() {
        return Err("Redis URL not configured, async booking disabled".into());
    }
    
    let worker_config = WorkerConfig {
        worker_id: "main-api-worker".to_string(),
        max_concurrent_jobs: 3,
        job_timeout_seconds: 120,
        retry_delay_seconds: 30,
        health_check_interval_seconds: 60,
        graceful_shutdown_timeout_seconds: 30,
    };
    
    let consumer = BookingConsumerService::new(worker_config, config).await?;
    
    // Start consumer in background
    tokio::spawn(async move {
        if let Err(e) = consumer.start().await {
            tracing::error!("Booking consumer service failed: {}", e);
        }
    });
    
    info!("Booking queue consumer service started successfully");
    Ok(())
}
