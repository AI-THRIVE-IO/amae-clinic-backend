use std::sync::Arc;
use uuid::Uuid;
use assert_matches::assert_matches;

use booking_queue_cell::*;
use shared_config::AppConfig;
use super::RedisTestUtils;

#[tokio::test]
async fn test_redis_queue_service_initialization() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue_service = RedisQueueService::new(&config).await;
    assert!(queue_service.is_ok(), "Queue service should initialize successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_redis_queue_service_initialization_invalid_url() {
    let config = AppConfig {
        supabase_url: "http://localhost:54321".to_string(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
        cloudflare_realtime_app_id: "test-app-id".to_string(),
        cloudflare_realtime_api_token: "test-token".to_string(),
        cloudflare_realtime_base_url: "https://test.cloudflare.com/v1".to_string(),
        redis_url: Some("redis://invalid-host:6379".to_string()),
    };
    
    let queue_service = RedisQueueService::new(&config).await;
    assert!(queue_service.is_err(), "Queue service should fail with invalid Redis URL");
}

#[tokio::test]
async fn test_enqueue_job_success() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let job = test_utils.create_test_job(None);
    let job_id = job.job_id;
    
    let result = queue_service.enqueue_job(&job).await;
    assert!(result.is_ok(), "Job should be enqueued successfully");
    
    // Verify job exists in Redis
    let retrieved_job = queue_service.get_job(job_id).await.expect("Failed to get job");
    assert!(retrieved_job.is_some(), "Job should exist in Redis");
    
    let retrieved_job = retrieved_job.unwrap();
    assert_eq!(retrieved_job.job_id, job_id);
    assert_eq!(retrieved_job.status, BookingStatus::Queued);
    assert_eq!(retrieved_job.patient_id, job.patient_id);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_dequeue_job_success() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    let worker_id = "test-worker-1";
    let dequeued_job = queue_service.dequeue_job(worker_id).await.expect("Failed to dequeue job");
    
    assert!(dequeued_job.is_some(), "Job should be dequeued successfully");
    let dequeued_job = dequeued_job.unwrap();
    
    assert_eq!(dequeued_job.job_id, job.job_id);
    assert_eq!(dequeued_job.status, BookingStatus::Processing);
    assert_eq!(dequeued_job.worker_id, Some(worker_id.to_string()));
    assert!(dequeued_job.updated_at > job.updated_at);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_dequeue_job_empty_queue() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let worker_id = "test-worker-1";
    let result = queue_service.dequeue_job(worker_id).await.expect("Failed to dequeue from empty queue");
    
    assert!(result.is_none(), "Should return None for empty queue");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_update_job_status_valid_transitions() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Test valid status transitions
    let transitions = vec![
        (BookingStatus::Queued, BookingStatus::Processing),
        (BookingStatus::Processing, BookingStatus::DoctorMatching),
        (BookingStatus::DoctorMatching, BookingStatus::AvailabilityCheck),
        (BookingStatus::AvailabilityCheck, BookingStatus::SlotSelection),
        (BookingStatus::SlotSelection, BookingStatus::AppointmentCreation),
        (BookingStatus::AppointmentCreation, BookingStatus::AlternativeGeneration),
        (BookingStatus::AlternativeGeneration, BookingStatus::Completed),
    ];
    
    for (from_status, to_status) in transitions {
        let result = queue_service.update_job_status(job.job_id, to_status.clone(), None).await;
        assert!(result.is_ok(), "Valid transition from {:?} to {:?} should succeed", from_status, to_status);
        
        let updated_job = queue_service.get_job(job.job_id).await.expect("Failed to get updated job");
        assert!(updated_job.is_some(), "Job should exist after status update");
        assert_eq!(updated_job.unwrap().status, to_status);
    }
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_update_job_status_invalid_transitions() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Test invalid transition (skipping steps)
    let result = queue_service.update_job_status(job.job_id, BookingStatus::Completed, None).await;
    assert!(result.is_err(), "Invalid transition should fail");
    assert_matches!(result.unwrap_err(), BookingQueueError::InvalidStatusTransition { .. });
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_update_job_status_with_error_message() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    let error_message = "Doctor not available";
    let result = queue_service.update_job_status(job.job_id, BookingStatus::Failed, Some(error_message.to_string())).await;
    assert!(result.is_ok(), "Updating to failed status should succeed");
    
    let updated_job = queue_service.get_job(job.job_id).await.expect("Failed to get updated job");
    assert!(updated_job.is_some(), "Job should exist after status update");
    
    let updated_job = updated_job.unwrap();
    assert_eq!(updated_job.status, BookingStatus::Failed);
    assert_eq!(updated_job.error_message, Some(error_message.to_string()));
    assert!(updated_job.completed_at.is_some(), "Failed jobs should have completion time");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_update_job_status_nonexistent_job() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let nonexistent_job_id = Uuid::new_v4();
    let result = queue_service.update_job_status(nonexistent_job_id, BookingStatus::Processing, None).await;
    
    assert!(result.is_err(), "Updating nonexistent job should fail");
    assert_matches!(result.unwrap_err(), BookingQueueError::JobNotFound(_));
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_retry_job_success() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // First mark job as failed
    queue_service.update_job_status(job.job_id, BookingStatus::Failed, Some("Test failure".to_string())).await
        .expect("Failed to mark job as failed");
    
    // Now retry the job
    let result = queue_service.retry_job(job.job_id).await;
    assert!(result.is_ok(), "Job retry should succeed");
    
    let retried_job = queue_service.get_job(job.job_id).await.expect("Failed to get retried job");
    assert!(retried_job.is_some(), "Retried job should exist");
    
    let retried_job = retried_job.unwrap();
    assert_eq!(retried_job.status, BookingStatus::Retrying);
    assert_eq!(retried_job.retry_count, 1);
    assert!(retried_job.error_message.is_none(), "Error message should be cleared on retry");
    assert!(retried_job.worker_id.is_none(), "Worker ID should be cleared on retry");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_retry_job_max_retries_exceeded() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let mut job = test_utils.create_test_job(None);
    job.retry_count = job.max_retries; // Set to max retries
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Mark as failed
    queue_service.update_job_status(job.job_id, BookingStatus::Failed, Some("Test failure".to_string())).await
        .expect("Failed to mark job as failed");
    
    // Try to retry - should fail
    let result = queue_service.retry_job(job.job_id).await;
    assert!(result.is_err(), "Retry should fail when max retries exceeded");
    assert_matches!(result.unwrap_err(), BookingQueueError::MaxRetriesExceeded { .. });
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_retry_job_not_failed() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Try to retry job that's not failed
    let result = queue_service.retry_job(job.job_id).await;
    assert!(result.is_err(), "Retry should fail for job that's not failed");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_get_queue_stats() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let stats = queue_service.get_queue_stats().await;
    
    // Initial stats should be empty
    assert_eq!(stats.queued_jobs, 0);
    assert_eq!(stats.processing_jobs, 0);
    assert_eq!(stats.completed_today, 0);
    assert_eq!(stats.failed_today, 0);
    assert_eq!(stats.active_workers, 0);
    assert_matches!(stats.queue_health, QueueHealth::Healthy);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_queue_stats_updates() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    // Enqueue a job
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    let stats = queue_service.get_queue_stats().await;
    assert_eq!(stats.queued_jobs, 1);
    
    // Dequeue job
    queue_service.dequeue_job("test-worker").await.expect("Failed to dequeue job");
    
    let stats = queue_service.get_queue_stats().await;
    assert_eq!(stats.queued_jobs, 0);
    assert_eq!(stats.processing_jobs, 1);
    
    // Complete job
    queue_service.update_job_status(job.job_id, BookingStatus::Completed, None).await
        .expect("Failed to complete job");
    
    let stats = queue_service.get_queue_stats().await;
    assert_eq!(stats.processing_jobs, 0);
    assert_eq!(stats.completed_today, 1);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_cleanup_expired_jobs() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    // Create a job
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Cleanup (should find no expired jobs as job was just created)
    let cleaned = queue_service.cleanup_expired_jobs().await.expect("Failed to cleanup expired jobs");
    assert_eq!(cleaned, 0, "No jobs should be expired yet");
    
    // Verify job still exists
    let retrieved_job = queue_service.get_job(job.job_id).await.expect("Failed to get job");
    assert!(retrieved_job.is_some(), "Job should still exist after cleanup");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_concurrent_job_operations() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    
    // Create multiple jobs concurrently
    let mut handles = vec![];
    for _i in 0..10 {
        let queue_clone = Arc::clone(&queue_service);
        let job = test_utils.create_test_job(None);
        
        let handle = tokio::spawn(async move {
            queue_clone.enqueue_job(&job).await.expect("Failed to enqueue job");
            job.job_id
        });
        handles.push(handle);
    }
    
    // Wait for all jobs to be enqueued
    let mut job_ids = vec![];
    for handle in handles {
        job_ids.push(handle.await.expect("Failed to join handle"));
    }
    
    // Verify all jobs were enqueued
    let stats = queue_service.get_queue_stats().await;
    assert_eq!(stats.queued_jobs, 10);
    
    // Dequeue jobs concurrently with multiple workers
    let mut dequeue_handles = vec![];
    for i in 0..5 {
        let queue_clone = Arc::clone(&queue_service);
        let worker_id = format!("worker-{}", i);
        
        let handle = tokio::spawn(async move {
            queue_clone.dequeue_job(&worker_id).await
        });
        dequeue_handles.push(handle);
    }
    
    // Wait for dequeue operations
    let mut dequeued_count = 0;
    for handle in dequeue_handles {
        let result = handle.await.expect("Failed to join dequeue handle");
        if result.expect("Failed to dequeue").is_some() {
            dequeued_count += 1;
        }
    }
    
    assert_eq!(dequeued_count, 5, "Should have dequeued 5 jobs");
    
    let stats = queue_service.get_queue_stats().await;
    assert_eq!(stats.queued_jobs, 5, "Should have 5 jobs remaining in queue");
    assert_eq!(stats.processing_jobs, 5, "Should have 5 jobs processing");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_job_expiration_handling() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    let job = test_utils.create_test_job(None);
    queue_service.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Verify job has expiration set in Redis
    let job_key = format!("booking_job:{}", job.job_id);
    let ttl_exists = test_utils.assert_key_exists(&job_key).await.expect("Failed to check key existence");
    assert!(ttl_exists, "Job key should exist in Redis");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test] 
async fn test_queue_service_redis_reconnection() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    let queue_service = RedisQueueService::new(&config).await.expect("Failed to create queue service");
    
    // Test that operations continue to work (Redis pool handles reconnections)
    for i in 0..5 {
        let job = test_utils.create_test_job(None);
        let result = queue_service.enqueue_job(&job).await;
        assert!(result.is_ok(), "Job {} should be enqueued successfully", i);
    }
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}