use std::sync::Arc;
use axum::{
    extract::{Path, State},
    response::Json,
    Extension,
};
use serde_json::{json, Value};
use tracing::{error, info};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_models::{auth::User, error::AppError};

use crate::{
    SmartBookingRequest, BookingQueueError,
    services::{
        consumer::BookingConsumerService,
        queue::RedisQueueService,
    },
    WorkerConfig,
};

/// Enqueue a smart booking request
pub async fn enqueue_smart_booking(
    State(config): State<Arc<AppConfig>>,
    Extension(user): Extension<User>,
    Json(request): Json<SmartBookingRequest>,
) -> Result<Json<Value>, AppError> {
    info!("Smart booking request from user: {}", user.id);

    // Create booking consumer service
    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(
        worker_config,
        config.clone(),
    ).await.map_err(|e| {
        error!("Failed to create booking consumer: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    // Enqueue the booking request
    let auth_token = "authenticated_user"; // In production, extract from JWT
    let response = consumer.enqueue_booking(request, auth_token).await.map_err(|e| {
        error!("Failed to enqueue booking: {}", e);
        match e {
            BookingQueueError::ValidationError(_) => AppError::BadRequest(e.to_string()),
            _ => AppError::Internal("Operation failed".to_string()),
        }
    })?;

    Ok(Json(json!({
        "success": true,
        "job_id": response.job_id,
        "status": response.status,
        "estimated_completion_time": response.estimated_completion_time,
        "websocket_channel": response.websocket_channel,
        "tracking_url": response.tracking_url,
        "retry_count": response.retry_count,
        "max_retries": response.max_retries
    })))
}

/// Get job status
pub async fn get_job_status(
    State(config): State<Arc<AppConfig>>,
    Extension(user): Extension<User>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    info!("Job status request for job: {} from user: {}", job_id, user.id);

    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(
        worker_config,
        config.clone(),
    ).await.map_err(|e| {
        error!("Failed to create booking consumer: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    let job = consumer.get_job_status(job_id).await.map_err(|e| {
        error!("Failed to get job status: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    match job {
        Some(job) => {
            // Verify user has access to this job
            let user_uuid = Uuid::parse_str(&user.id).map_err(|_| {
                AppError::BadRequest("Invalid user ID format".to_string())
            })?;
            if job.patient_id != user_uuid {
                return Err(AppError::Auth("Access denied".to_string()));
            }

            Ok(Json(json!({
                "job_id": job.job_id,
                "patient_id": job.patient_id,
                "status": job.status,
                "created_at": job.created_at,
                "updated_at": job.updated_at,
                "completed_at": job.completed_at,
                "retry_count": job.retry_count,
                "max_retries": job.max_retries,
                "error_message": job.error_message,
                "result": null
            })))
        }
        None => Err(AppError::NotFound("Job not found".to_string())),
    }
}

/// Cancel a job
pub async fn cancel_job(
    State(config): State<Arc<AppConfig>>,
    Extension(user): Extension<User>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    info!("Cancel job request for job: {} from user: {}", job_id, user.id);

    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(
        worker_config,
        config.clone(),
    ).await.map_err(|e| {
        error!("Failed to create booking consumer: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    // Verify user has access to this job
    let job = consumer.get_job_status(job_id).await.map_err(|e| {
        error!("Failed to get job status: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    match job {
        Some(job) => {
            let user_uuid = Uuid::parse_str(&user.id).map_err(|_| {
                AppError::BadRequest("Invalid user ID format".to_string())
            })?;
            if job.patient_id != user_uuid {
                return Err(AppError::Auth("Access denied".to_string()));
            }
        }
        None => return Err(AppError::NotFound("Job not found".to_string())),
    }

    consumer.cancel_job(job_id).await.map_err(|e| {
        error!("Failed to cancel job: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    Ok(Json(json!({
        "success": true,
        "message": "Job cancelled successfully"
    })))
}

/// Retry a failed job
pub async fn retry_job(
    State(config): State<Arc<AppConfig>>,
    Extension(user): Extension<User>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    info!("Retry job request for job: {} from user: {}", job_id, user.id);

    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(
        worker_config,
        config.clone(),
    ).await.map_err(|e| {
        error!("Failed to create booking consumer: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    // Verify user has access to this job
    let job = consumer.get_job_status(job_id).await.map_err(|e| {
        error!("Failed to get job status: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    match job {
        Some(job) => {
            let user_uuid = Uuid::parse_str(&user.id).map_err(|_| {
                AppError::BadRequest("Invalid user ID format".to_string())
            })?;
            if job.patient_id != user_uuid {
                return Err(AppError::Auth("Access denied".to_string()));
            }
        }
        None => return Err(AppError::NotFound("Job not found".to_string())),
    }

    consumer.retry_job(job_id).await.map_err(|e| {
        error!("Failed to retry job: {}", e);
        match e {
            BookingQueueError::MaxRetriesExceeded { .. } => AppError::BadRequest(e.to_string()),
            _ => AppError::Internal("Operation failed".to_string()),
        }
    })?;

    Ok(Json(json!({
        "success": true,
        "message": "Job retry initiated successfully"
    })))
}

/// Get queue statistics (admin only)
pub async fn get_queue_stats(
    State(config): State<Arc<AppConfig>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    // TODO: Add admin role check
    info!("Queue stats request from user: {}", user.id);

    let queue_service = RedisQueueService::new(&config).await.map_err(|e| {
        error!("Failed to create queue service: {}", e);
        AppError::Internal("Service initialization failed".to_string())
    })?;

    let stats = queue_service.get_queue_stats().await;

    Ok(Json(json!({
        "queued_jobs": stats.queued_jobs,
        "processing_jobs": stats.processing_jobs,
        "completed_today": stats.completed_today,
        "failed_today": stats.failed_today,
        "active_workers": stats.active_workers,
        "queue_health": stats.queue_health,
        "average_processing_time_ms": stats.average_processing_time_ms,
        "total_workers": stats.active_workers
    })))
}

/// Get WebSocket endpoint information
pub async fn get_websocket_endpoint(
    State(_config): State<Arc<AppConfig>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    info!("WebSocket endpoint request from user: {}", user.id);

    Ok(Json(json!({
        "websocket_base_url": "ws://localhost:3000/ws",
        "instructions": {
            "connect": "Connect to ws://localhost:3000/ws/{job_id} to receive real-time updates",
            "authentication": "Include Authorization header with your JWT token",
            "message_format": "JSON messages with job status updates"
        }
    })))
}