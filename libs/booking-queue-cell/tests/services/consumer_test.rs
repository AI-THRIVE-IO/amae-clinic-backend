use std::sync::Arc;
use tokio::time::{sleep, Duration, timeout};
use uuid::Uuid;
use chrono::Utc;

use booking_queue_cell::*;
use shared_config::AppConfig;
use super::RedisTestUtils;

#[tokio::test]
async fn test_consumer_service_initialization() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig {
        worker_id: "test-consumer-worker".to_string(),
        max_concurrent_jobs: 2,
        job_timeout_seconds: 30,
        retry_delay_seconds: 5,
        health_check_interval_seconds: 10,
        graceful_shutdown_timeout_seconds: 5,
    };
    
    let consumer_result = BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await;
    
    assert!(consumer_result.is_ok(), "Consumer service should initialize successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_service_initialization_invalid_redis() {
    let config = AppConfig {
        supabase_url: "http://localhost:54321".to_string(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
        cloudflare_realtime_app_id: "test-app-id".to_string(),
        cloudflare_realtime_api_token: "test-token".to_string(),
        cloudflare_realtime_base_url: "https://test.cloudflare.com/v1".to_string(),
        redis_url: Some("redis://invalid-host:6379".to_string()),
    };
    
    let worker_config = WorkerConfig::default();
    
    let consumer_result = BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await;
    
    assert!(consumer_result.is_err(), "Consumer service should fail with invalid Redis URL");
    
}

#[tokio::test]
async fn test_consumer_enqueue_booking_success() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig {
        worker_id: "test-consumer-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 30,
        retry_delay_seconds: 5,
        health_check_interval_seconds: 10,
        graceful_shutdown_timeout_seconds: 5,
    };
    
    let consumer = BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service");
    
    let patient_id = Uuid::new_v4();
    let request = SmartBookingRequest {
        patient_id,
        specialty: Some("neurology".to_string()),
        urgency: Some(BookingUrgency::High),
        preferred_doctor_id: Some(Uuid::new_v4()),
        preferred_time_slot: Some(Utc::now() + chrono::Duration::hours(24)),
        alternative_time_slots: Some(vec![
            Utc::now() + chrono::Duration::hours(48),
        ]),
        appointment_type: Some(AppointmentType::Specialist),
        reason_for_visit: Some("Neurological consultation".to_string()),
        consultation_mode: Some(ConsultationMode::InPerson),
        is_follow_up: Some(false),
        notes: Some("Patient has headaches".to_string()),
    };
    
    let auth_token = "test-auth-token";
    let result = consumer.enqueue_booking(request.clone(), auth_token).await;
    
    assert!(result.is_ok(), "Booking should be enqueued successfully via consumer");
    
    let response = result.unwrap();
    assert_eq!(response.status, BookingStatus::Queued);
    assert!(response.estimated_completion_time > Utc::now());
    assert_eq!(response.websocket_channel, format!("booking_{}", response.job_id));
    assert_eq!(response.tracking_url, format!("/appointments/booking-status/{}", response.job_id));
    
    // Verify job can be retrieved via consumer
    let job_status = consumer.get_job_status(response.job_id).await;
    assert!(job_status.is_ok(), "Getting job status should succeed");
    
    let job_option = job_status.unwrap();
    assert!(job_option.is_some(), "Job should exist");
    
    let job = job_option.unwrap();
    assert_eq!(job.patient_id, patient_id);
    assert_eq!(job.request.specialty, request.specialty);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_start_and_shutdown() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig {
        worker_id: "test-consumer-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 10,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 2,
        graceful_shutdown_timeout_seconds: 3,
    };
    
    let consumer = Arc::new(BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service"));
    
    // Start consumer in background
    let consumer_clone = Arc::clone(&consumer);
    let consumer_handle = tokio::spawn(async move {
        consumer_clone.start().await
    });
    
    // Let consumer start up
    sleep(Duration::from_millis(500)).await;
    
    // Shutdown consumer
    let shutdown_result = consumer.shutdown().await;
    assert!(shutdown_result.is_ok(), "Consumer shutdown should succeed");
    
    // Wait for consumer to complete
    let start_result = timeout(Duration::from_secs(10), consumer_handle).await;
    assert!(start_result.is_ok(), "Consumer should complete within timeout");
    assert!(start_result.unwrap().is_ok(), "Consumer start should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_job_processing_workflow() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig {
        worker_id: "test-consumer-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 15,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 5,
        graceful_shutdown_timeout_seconds: 3,
    };
    
    let consumer = Arc::new(BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service"));
    
    // Enqueue a job
    let patient_id = Uuid::new_v4();
    let request = SmartBookingRequest {
        patient_id,
        specialty: Some("psychiatry".to_string()),
        urgency: Some(BookingUrgency::Normal),
        preferred_doctor_id: None,
        preferred_time_slot: None,
        alternative_time_slots: None,
        appointment_type: Some(AppointmentType::InitialConsultation),
        reason_for_visit: Some("Mental health consultation".to_string()),
        consultation_mode: Some(ConsultationMode::Video),
        is_follow_up: Some(false),
        notes: Some("Patient seeking therapy".to_string()),
    };
    
    let auth_token = "test-auth-token";
    let enqueue_result = consumer.enqueue_booking(request, auth_token).await;
    assert!(enqueue_result.is_ok(), "Job should be enqueued successfully");
    
    let job_id = enqueue_result.unwrap().job_id;
    
    // Start consumer in background
    let consumer_clone = Arc::clone(&consumer);
    let consumer_handle = tokio::spawn(async move {
        consumer_clone.start().await
    });
    
    // Wait for job to be processed
    let mut job_completed = false;
    for _ in 0..30 { // Wait up to 30 seconds
        sleep(Duration::from_secs(1)).await;
        
        if let Ok(Some(job)) = consumer.get_job_status(job_id).await {
            if job.status == BookingStatus::Completed || job.status == BookingStatus::Failed {
                job_completed = true;
                break;
            }
        }
    }
    
    assert!(job_completed, "Job should be completed or failed within timeout");
    
    // Verify final job state
    let final_job = consumer.get_job_status(job_id).await.expect("Failed to get final job");
    assert!(final_job.is_some(), "Final job should exist");
    
    let final_job = final_job.unwrap();
    assert!(
        final_job.status == BookingStatus::Completed || final_job.status == BookingStatus::Failed,
        "Job should be in terminal state"
    );
    
    // Shutdown consumer
    consumer.shutdown().await.expect("Failed to shutdown consumer");
    
    timeout(Duration::from_secs(10), consumer_handle).await
        .expect("Consumer should complete within timeout")
        .expect("Consumer should complete successfully");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_cancel_job() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig {
        worker_id: "test-consumer-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 30,
        retry_delay_seconds: 5,
        health_check_interval_seconds: 10,
        graceful_shutdown_timeout_seconds: 5,
    };
    
    let consumer = BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service");
    
    // Enqueue a job
    let patient_id = Uuid::new_v4();
    let request = SmartBookingRequest {
        patient_id,
        specialty: Some("radiology".to_string()),
        urgency: Some(BookingUrgency::Normal),
        preferred_doctor_id: None,
        preferred_time_slot: None,
        alternative_time_slots: None,
        appointment_type: Some(AppointmentType::Procedure),
        reason_for_visit: Some("CT scan".to_string()),
        consultation_mode: Some(ConsultationMode::InPerson),
        is_follow_up: Some(false),
        notes: None,
    };
    
    let auth_token = "test-auth-token";
    let enqueue_result = consumer.enqueue_booking(request, auth_token).await;
    assert!(enqueue_result.is_ok(), "Job should be enqueued successfully");
    
    let job_id = enqueue_result.unwrap().job_id;
    
    // Cancel the job
    let cancel_result = consumer.cancel_job(job_id).await;
    assert!(cancel_result.is_ok(), "Job cancellation should succeed");
    
    // Verify job is cancelled
    let cancelled_job = consumer.get_job_status(job_id).await.expect("Failed to get cancelled job");
    assert!(cancelled_job.is_some(), "Cancelled job should exist");
    
    let cancelled_job = cancelled_job.unwrap();
    assert_eq!(cancelled_job.status, BookingStatus::Cancelled);
    assert!(cancelled_job.completed_at.is_some(), "Cancelled job should have completion time");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_retry_job() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig {
        worker_id: "test-consumer-worker".to_string(),
        max_concurrent_jobs: 1,
        job_timeout_seconds: 30,
        retry_delay_seconds: 5,
        health_check_interval_seconds: 10,
        graceful_shutdown_timeout_seconds: 5,
    };
    
    let consumer = BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service");
    
    // Enqueue a job
    let patient_id = Uuid::new_v4();
    let request = SmartBookingRequest {
        patient_id,
        specialty: Some("pathology".to_string()),
        urgency: Some(BookingUrgency::Normal),
        preferred_doctor_id: None,
        preferred_time_slot: None,
        alternative_time_slots: None,
        appointment_type: Some(AppointmentType::HealthScreening),
        reason_for_visit: Some("Lab results review".to_string()),
        consultation_mode: Some(ConsultationMode::Phone),
        is_follow_up: Some(true),
        notes: None,
    };
    
    let auth_token = "test-auth-token";
    let enqueue_result = consumer.enqueue_booking(request, auth_token).await;
    assert!(enqueue_result.is_ok(), "Job should be enqueued successfully");
    
    let job_id = enqueue_result.unwrap().job_id;
    
    // Manually mark job as failed (simulate failure)
    // We need access to the queue service for this
    let queue_service = RedisQueueService::new(&test_utils.create_test_config()).await
        .expect("Failed to create queue service");
    queue_service.update_job_status(job_id, BookingStatus::Failed, Some("Simulated failure".to_string())).await
        .expect("Failed to mark job as failed");
    
    // Retry the job
    let retry_result = consumer.retry_job(job_id).await;
    assert!(retry_result.is_ok(), "Job retry should succeed");
    
    // Verify job is retrying
    let retried_job = consumer.get_job_status(job_id).await.expect("Failed to get retried job");
    assert!(retried_job.is_some(), "Retried job should exist");
    
    let retried_job = retried_job.unwrap();
    assert_eq!(retried_job.status, BookingStatus::Retrying);
    assert_eq!(retried_job.retry_count, 1);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_get_queue_stats() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig {
        worker_id: "test-consumer-worker".to_string(),
        max_concurrent_jobs: 2,
        job_timeout_seconds: 30,
        retry_delay_seconds: 5,
        health_check_interval_seconds: 10,
        graceful_shutdown_timeout_seconds: 5,
    };
    
    let consumer = BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service");
    
    // Get initial stats
    let initial_stats = consumer.get_queue_stats().await;
    assert_eq!(initial_stats.queued_jobs, 0);
    assert_eq!(initial_stats.processing_jobs, 0);
    assert_eq!(initial_stats.completed_today, 0);
    assert_eq!(initial_stats.failed_today, 0);
    
    // Enqueue some jobs
    for i in 0..3 {
        let patient_id = Uuid::new_v4();
        let request = SmartBookingRequest {
            patient_id,
            specialty: Some(format!("specialty-{}", i)),
            urgency: Some(BookingUrgency::Normal),
            preferred_doctor_id: None,
            preferred_time_slot: None,
            alternative_time_slots: None,
            appointment_type: Some(AppointmentType::InitialConsultation),
            reason_for_visit: Some(format!("Reason {}", i)),
            consultation_mode: Some(ConsultationMode::InPerson),
            is_follow_up: Some(false),
            notes: None,
        };
        
        let result = consumer.enqueue_booking(request, "test-token").await;
        assert!(result.is_ok(), "Job {} should be enqueued successfully", i);
    }
    
    // Get updated stats
    let updated_stats = consumer.get_queue_stats().await;
    assert_eq!(updated_stats.queued_jobs, 3);
    assert_eq!(updated_stats.processing_jobs, 0);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_websocket_service_access() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig::default();
    
    let consumer = BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service");
    
    // Get WebSocket service
    let websocket_service = consumer.get_websocket_service();
    
    // Verify we can create channels
    let job_id = Uuid::new_v4();
    let _receiver = websocket_service.create_channel(job_id).await;
    
    // Verify we can get active channels
    let active_channels = websocket_service.get_active_channels().await;
    assert!(active_channels.contains(&job_id), "Job ID should be in active channels");
    
    // Clean up channel
    websocket_service.remove_channel(job_id).await;
    
    let active_channels = websocket_service.get_active_channels().await;
    assert!(!active_channels.contains(&job_id), "Job ID should be removed from active channels");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_multiple_concurrent_operations() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig {
        worker_id: "test-consumer-worker".to_string(),
        max_concurrent_jobs: 3,
        job_timeout_seconds: 30,
        retry_delay_seconds: 1,
        health_check_interval_seconds: 5,
        graceful_shutdown_timeout_seconds: 5,
    };
    
    let consumer = Arc::new(BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service"));
    
    // Perform multiple operations concurrently
    let mut handles = vec![];
    
    // Enqueue jobs
    for i in 0..5 {
        let consumer_clone = Arc::clone(&consumer);
        let handle = tokio::spawn(async move {
            let patient_id = Uuid::new_v4();
            let request = SmartBookingRequest {
                patient_id,
                specialty: Some(format!("specialty-{}", i)),
                urgency: Some(BookingUrgency::Normal),
                preferred_doctor_id: None,
                preferred_time_slot: None,
                alternative_time_slots: None,
                appointment_type: Some(AppointmentType::InitialConsultation),
                reason_for_visit: Some(format!("Concurrent test {}", i)),
                consultation_mode: Some(ConsultationMode::InPerson),
                is_follow_up: Some(false),
                notes: None,
            };
            
            consumer_clone.enqueue_booking(request, "test-token").await
        });
        handles.push(handle);
    }
    
    // Wait for all enqueue operations
    let mut successful_enqueues = 0;
    let mut job_ids = vec![];
    for handle in handles {
        let result = handle.await.expect("Failed to join handle");
        if let Ok(response) = result {
            successful_enqueues += 1;
            job_ids.push(response.job_id);
        }
    }
    
    assert_eq!(successful_enqueues, 5, "All 5 jobs should be enqueued successfully");
    
    // Perform status checks concurrently
    let mut status_handles = vec![];
    for job_id in &job_ids {
        let consumer_clone = Arc::clone(&consumer);
        let job_id = *job_id;
        let handle = tokio::spawn(async move {
            consumer_clone.get_job_status(job_id).await
        });
        status_handles.push(handle);
    }
    
    // Wait for all status checks
    let mut successful_status_checks = 0;
    for handle in status_handles {
        let result = handle.await.expect("Failed to join status check handle");
        if result.is_ok() && result.unwrap().is_some() {
            successful_status_checks += 1;
        }
    }
    
    assert_eq!(successful_status_checks, 5, "All 5 status checks should succeed");
    
    // Verify queue stats
    let stats = consumer.get_queue_stats().await;
    assert_eq!(stats.queued_jobs, 5, "Queue should contain 5 jobs");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_consumer_error_handling() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let worker_config = WorkerConfig::default();
    
    let consumer = BookingConsumerService::new(
        worker_config,
        Arc::new(config),
    ).await.expect("Failed to create consumer service");
    
    // Test operations on nonexistent jobs
    let nonexistent_job_id = Uuid::new_v4();
    
    let status_result = consumer.get_job_status(nonexistent_job_id).await;
    assert!(status_result.is_ok(), "Getting status of nonexistent job should not error");
    assert!(status_result.unwrap().is_none(), "Nonexistent job should return None");
    
    let cancel_result = consumer.cancel_job(nonexistent_job_id).await;
    assert!(cancel_result.is_err(), "Cancelling nonexistent job should error");
    
    let retry_result = consumer.retry_job(nonexistent_job_id).await;
    assert!(retry_result.is_err(), "Retrying nonexistent job should error");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}