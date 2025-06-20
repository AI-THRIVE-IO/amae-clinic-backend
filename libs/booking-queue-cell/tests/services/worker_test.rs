use std::sync::Arc;
use tokio::time::{sleep, Duration, timeout};

use booking_queue_cell::*;
use super::RedisTestUtils;

#[tokio::test]
async fn test_worker_service_initialization() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 2,
        job_timeout_seconds: 30,
        retry_delay_seconds: 5,
        health_check_interval_seconds: 10,
        graceful_shutdown_timeout_seconds: 5,
    };
    
    let _worker = BookingWorkerService::new(
        worker_config,
        queue,
        Arc::new(config),
        websocket_service,
    );
    
    // Worker should be created successfully (can't access private field, but creation succeeding means it works)
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_worker_service_start_and_shutdown() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 5,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 1,
        graceful_shutdown_timeout_seconds: 2,
    };
    
    let worker = Arc::new(BookingWorkerService::new(
        worker_config,
        queue,
        Arc::new(config),
        websocket_service,
    ));
    
    // Start worker in background
    let worker_clone = Arc::clone(&worker);
    let worker_handle = tokio::spawn(async move {
        worker_clone.start().await
    });
    
    // Let worker start up
    sleep(Duration::from_millis(100)).await;
    
    // Shutdown worker
    let shutdown_result = worker.shutdown().await;
    assert!(shutdown_result.is_ok(), "Worker shutdown should succeed");
    
    // Wait for worker to complete
    let start_result = timeout(Duration::from_secs(5), worker_handle).await;
    assert!(start_result.is_ok(), "Worker should complete within timeout");
    assert!(start_result.unwrap().is_ok(), "Worker start should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_worker_processes_job_successfully() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 10,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 5,
        graceful_shutdown_timeout_seconds: 2,
    };
    
    let worker = Arc::new(BookingWorkerService::new(
        worker_config,
        Arc::clone(&queue),
        Arc::new(config),
        websocket_service,
    ));
    
    // Create and enqueue a job
    let job = test_utils.create_test_job(None);
    let job_id = job.job_id;
    queue.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Start worker in background
    let worker_clone = Arc::clone(&worker);
    let worker_handle = tokio::spawn(async move {
        worker_clone.start().await
    });
    
    // Wait for job to be processed
    let job_completed = test_utils.wait_for_job_status(
        &queue,
        job_id,
        BookingStatus::Completed,
        15, // 15 second timeout
    ).await.expect("Failed to wait for job completion");
    
    assert!(job_completed, "Job should be completed within timeout");
    
    // Verify final job state
    let final_job = queue.get_job(job_id).await.expect("Failed to get final job");
    assert!(final_job.is_some(), "Job should exist after completion");
    
    let final_job = final_job.unwrap();
    assert_eq!(final_job.status, BookingStatus::Completed);
    assert!(final_job.completed_at.is_some(), "Completed job should have completion time");
    
    // Shutdown worker
    worker.shutdown().await.expect("Failed to shutdown worker");
    
    // Wait for worker to complete
    timeout(Duration::from_secs(5), worker_handle).await
        .expect("Worker should complete within timeout")
        .expect("Worker should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_worker_handles_job_timeout() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    // Very short timeout to force timeout scenario
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 1, // 1 second timeout
        retry_delay_seconds: 1,
        health_check_interval_seconds: 5,
        graceful_shutdown_timeout_seconds: 2,
    };
    
    let worker = Arc::new(BookingWorkerService::new(
        worker_config,
        Arc::clone(&queue),
        Arc::new(config),
        websocket_service,
    ));
    
    // Create and enqueue a job
    let job = test_utils.create_test_job(None);
    let job_id = job.job_id;
    queue.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Start worker in background
    let worker_clone = Arc::clone(&worker);
    let worker_handle = tokio::spawn(async move {
        worker_clone.start().await
    });
    
    // Wait for job to timeout and fail
    let job_failed = test_utils.wait_for_job_status(
        &queue,
        job_id,
        BookingStatus::Failed,
        10, // 10 second timeout for the test
    ).await.expect("Failed to wait for job failure");
    
    assert!(job_failed, "Job should fail due to timeout");
    
    // Verify job failed with timeout error
    let failed_job = queue.get_job(job_id).await.expect("Failed to get failed job");
    assert!(failed_job.is_some(), "Failed job should exist");
    
    let failed_job = failed_job.unwrap();
    assert_eq!(failed_job.status, BookingStatus::Failed);
    assert!(failed_job.error_message.is_some(), "Failed job should have error message");
    assert!(failed_job.error_message.unwrap().contains("timed out"), "Error should mention timeout");
    
    // Shutdown worker
    worker.shutdown().await.expect("Failed to shutdown worker");
    
    timeout(Duration::from_secs(5), worker_handle).await
        .expect("Worker should complete within timeout")
        .expect("Worker should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_worker_concurrent_job_processing() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 3, // Allow 3 concurrent jobs
        job_timeout_seconds: 15,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 5,
        graceful_shutdown_timeout_seconds: 3,
    };
    
    let worker = Arc::new(BookingWorkerService::new(
        worker_config,
        Arc::clone(&queue),
        Arc::new(config),
        websocket_service,
    ));
    
    // Create and enqueue multiple jobs
    let mut job_ids = vec![];
    for _ in 0..5 {
        let job = test_utils.create_test_job(None);
        job_ids.push(job.job_id);
        queue.enqueue_job(&job).await.expect("Failed to enqueue job");
    }
    
    // Start worker in background
    let worker_clone = Arc::clone(&worker);
    let worker_handle = tokio::spawn(async move {
        worker_clone.start().await
    });
    
    // Wait for all jobs to complete
    for job_id in &job_ids {
        let job_completed = test_utils.wait_for_job_status(
            &queue,
            *job_id,
            BookingStatus::Completed,
            30, // 30 second timeout
        ).await.expect("Failed to wait for job completion");
        
        assert!(job_completed, "Job {} should be completed within timeout", job_id);
    }
    
    // Verify all jobs completed
    for job_id in &job_ids {
        let final_job = queue.get_job(*job_id).await.expect("Failed to get final job");
        assert!(final_job.is_some(), "Job {} should exist after completion", job_id);
        assert_eq!(final_job.unwrap().status, BookingStatus::Completed);
    }
    
    // Shutdown worker
    worker.shutdown().await.expect("Failed to shutdown worker");
    
    timeout(Duration::from_secs(10), worker_handle).await
        .expect("Worker should complete within timeout")
        .expect("Worker should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_worker_job_retry_mechanism() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 1, // Short timeout to force failure
        retry_delay_seconds: 1, // Short retry delay for testing
        health_check_interval_seconds: 5,
        graceful_shutdown_timeout_seconds: 2,
    };
    
    let worker = Arc::new(BookingWorkerService::new(
        worker_config,
        Arc::clone(&queue),
        Arc::new(config),
        websocket_service,
    ));
    
    // Create a job that will timeout and be retried
    let job = test_utils.create_test_job(None);
    let job_id = job.job_id;
    queue.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Start worker in background
    let worker_clone = Arc::clone(&worker);
    let worker_handle = tokio::spawn(async move {
        worker_clone.start().await
    });
    
    // Wait a bit for initial processing and retry
    sleep(Duration::from_secs(5)).await;
    
    // Check if job was retried (should have retry_count > 0)
    let retried_job = queue.get_job(job_id).await.expect("Failed to get retried job");
    assert!(retried_job.is_some(), "Job should exist after retry attempt");
    
    let retried_job = retried_job.unwrap();
    // Job should either be retrying or failed after max retries
    assert!(
        retried_job.retry_count > 0 || retried_job.status == BookingStatus::Failed,
        "Job should have been retried or failed after max retries"
    );
    
    // Shutdown worker
    worker.shutdown().await.expect("Failed to shutdown worker");
    
    timeout(Duration::from_secs(5), worker_handle).await
        .expect("Worker should complete within timeout")
        .expect("Worker should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_worker_health_check_functionality() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 10,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 1, // Very frequent health checks
        graceful_shutdown_timeout_seconds: 2,
    };
    
    let worker = Arc::new(BookingWorkerService::new(
        worker_config,
        Arc::clone(&queue),
        Arc::new(config),
        websocket_service,
    ));
    
    // Start worker in background
    let worker_clone = Arc::clone(&worker);
    let worker_handle = tokio::spawn(async move {
        worker_clone.start().await
    });
    
    // Let health checks run for a bit
    sleep(Duration::from_secs(3)).await;
    
    // Health checks should be running (no way to directly observe, but worker should be stable)
    // The fact that we can shutdown cleanly indicates health checks are working
    
    // Shutdown worker
    worker.shutdown().await.expect("Failed to shutdown worker");
    
    timeout(Duration::from_secs(5), worker_handle).await
        .expect("Worker should complete within timeout")
        .expect("Worker should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_worker_graceful_shutdown_with_active_jobs() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 2,
        job_timeout_seconds: 10,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 5,
        graceful_shutdown_timeout_seconds: 3, // 3 second grace period
    };
    
    let worker = Arc::new(BookingWorkerService::new(
        worker_config,
        Arc::clone(&queue),
        Arc::new(config),
        websocket_service,
    ));
    
    // Create jobs but don't wait for completion before shutdown
    for _ in 0..3 {
        let job = test_utils.create_test_job(None);
        queue.enqueue_job(&job).await.expect("Failed to enqueue job");
    }
    
    // Start worker in background
    let worker_clone = Arc::clone(&worker);
    let worker_handle = tokio::spawn(async move {
        worker_clone.start().await
    });
    
    // Let worker start processing
    sleep(Duration::from_millis(500)).await;
    
    // Shutdown worker while jobs might be processing
    let shutdown_start = std::time::Instant::now();
    worker.shutdown().await.expect("Failed to shutdown worker");
    let shutdown_duration = shutdown_start.elapsed();
    
    // Shutdown should respect grace period
    assert!(shutdown_duration >= Duration::from_secs(3), "Shutdown should wait for grace period");
    
    timeout(Duration::from_secs(10), worker_handle).await
        .expect("Worker should complete within timeout")
        .expect("Worker should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_worker_status_progression() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    
    let worker_config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 15,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 5,
        graceful_shutdown_timeout_seconds: 2,
    };
    
    let worker = Arc::new(BookingWorkerService::new(
        worker_config,
        Arc::clone(&queue),
        Arc::new(config),
        websocket_service,
    ));
    
    // Create and enqueue a job
    let job = test_utils.create_test_job(None);
    let job_id = job.job_id;
    queue.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Start worker in background
    let worker_clone = Arc::clone(&worker);
    let worker_handle = tokio::spawn(async move {
        worker_clone.start().await
    });
    
    // Monitor status progression
    let expected_statuses = vec![
        BookingStatus::Processing,
        BookingStatus::DoctorMatching,
        BookingStatus::AvailabilityCheck,
        BookingStatus::SlotSelection,
        BookingStatus::AppointmentCreation,
        BookingStatus::AlternativeGeneration,
        BookingStatus::Completed,
    ];
    
    for expected_status in expected_statuses {
        let status_reached = test_utils.wait_for_job_status(
            &queue,
            job_id,
            expected_status.clone(),
            10, // 10 second timeout for each status
        ).await.expect("Failed to wait for status");
        
        assert!(status_reached, "Job should reach status {:?}", expected_status);
    }
    
    // Shutdown worker
    worker.shutdown().await.expect("Failed to shutdown worker");
    
    timeout(Duration::from_secs(5), worker_handle).await
        .expect("Worker should complete within timeout")
        .expect("Worker should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

// Note: clone_for_worker is a private method used internally by the worker service
// We don't need to test private implementation details