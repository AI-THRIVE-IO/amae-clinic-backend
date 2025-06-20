use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;

use booking_queue_cell::*;
use super::RedisTestUtils;

#[tokio::test]
async fn test_producer_service_initialization() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let _producer = BookingProducerService::new(queue);
    
    // Producer should be created successfully (no explicit way to verify internal state)
    // Success is implicit if no panic occurs
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_enqueue_smart_booking_success() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    let patient_id = Uuid::new_v4();
    let request = SmartBookingRequest {
        patient_id,
        specialty: Some("cardiology".to_string()),
        urgency: Some(BookingUrgency::High),
        preferred_doctor_id: Some(Uuid::new_v4()),
        preferred_time_slot: Some(Utc::now() + chrono::Duration::hours(24)),
        alternative_time_slots: Some(vec![
            Utc::now() + chrono::Duration::hours(48),
            Utc::now() + chrono::Duration::hours(72),
        ]),
        appointment_type: Some(AppointmentType::InitialConsultation),
        reason_for_visit: Some("Chest pain evaluation".to_string()),
        consultation_mode: Some(ConsultationMode::InPerson),
        is_follow_up: Some(false),
        notes: Some("Patient reports chest discomfort".to_string()),
    };
    
    let auth_token = "test-auth-token";
    let result = producer.enqueue_smart_booking(request.clone(), auth_token).await;
    
    assert!(result.is_ok(), "Smart booking should be enqueued successfully");
    
    let response = result.unwrap();
    assert_eq!(response.status, BookingStatus::Queued);
    assert_eq!(response.retry_count, 0);
    assert_eq!(response.max_retries, 3);
    assert!(response.estimated_completion_time > Utc::now());
    assert_eq!(response.websocket_channel, format!("booking_{}", response.job_id));
    assert_eq!(response.tracking_url, format!("/appointments/booking-status/{}", response.job_id));
    
    // Verify job was actually enqueued
    let job = queue.get_job(response.job_id).await.expect("Failed to get job");
    assert!(job.is_some(), "Job should exist in queue");
    
    let job = job.unwrap();
    assert_eq!(job.patient_id, patient_id);
    assert_eq!(job.status, BookingStatus::Queued);
    assert_eq!(job.request.specialty, request.specialty);
    assert_eq!(job.request.urgency, request.urgency);
    assert_eq!(job.request.reason_for_visit, request.reason_for_visit);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_enqueue_smart_booking_minimal_request() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    let patient_id = Uuid::new_v4();
    let minimal_request = SmartBookingRequest {
        patient_id,
        specialty: None,
        urgency: None,
        preferred_doctor_id: None,
        preferred_time_slot: None,
        alternative_time_slots: None,
        appointment_type: None,
        reason_for_visit: None,
        consultation_mode: None,
        is_follow_up: None,
        notes: None,
    };
    
    let auth_token = "test-auth-token";
    let result = producer.enqueue_smart_booking(minimal_request.clone(), auth_token).await;
    
    assert!(result.is_ok(), "Minimal booking request should be enqueued successfully");
    
    let response = result.unwrap();
    assert_eq!(response.status, BookingStatus::Queued);
    
    // Verify job was enqueued
    let job = queue.get_job(response.job_id).await.expect("Failed to get job");
    assert!(job.is_some(), "Job should exist in queue");
    
    let job = job.unwrap();
    assert_eq!(job.patient_id, patient_id);
    assert_eq!(job.request.specialty, None);
    assert_eq!(job.request.urgency, None);
    assert_eq!(job.request.preferred_doctor_id, None);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_enqueue_smart_booking_with_all_urgency_levels() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    let urgency_levels = vec![
        BookingUrgency::Low,
        BookingUrgency::Normal,
        BookingUrgency::High,
        BookingUrgency::Critical,
    ];
    
    for urgency in urgency_levels {
        let patient_id = Uuid::new_v4();
        let request = SmartBookingRequest {
            patient_id,
            specialty: Some("emergency".to_string()),
            urgency: Some(urgency.clone()),
            preferred_doctor_id: None,
            preferred_time_slot: None,
            alternative_time_slots: None,
            appointment_type: Some(AppointmentType::Emergency),
            reason_for_visit: Some(format!("Test {:?} urgency", urgency)),
            consultation_mode: Some(ConsultationMode::InPerson),
            is_follow_up: Some(false),
            notes: None,
        };
        
        let auth_token = "test-auth-token";
        let result = producer.enqueue_smart_booking(request.clone(), auth_token).await;
        
        assert!(result.is_ok(), "Booking with {:?} urgency should be enqueued", urgency);
        
        let response = result.unwrap();
        let job = queue.get_job(response.job_id).await.expect("Failed to get job");
        assert!(job.is_some(), "Job with {:?} urgency should exist", urgency);
        
        let job = job.unwrap();
        assert_eq!(job.request.urgency, Some(urgency));
    }
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_enqueue_smart_booking_with_all_appointment_types() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    let appointment_types = vec![
        AppointmentType::InitialConsultation,
        AppointmentType::FollowUpConsultation,
        AppointmentType::Emergency,
        AppointmentType::Wellness,
        AppointmentType::Specialist,
        AppointmentType::Procedure,
        AppointmentType::Vaccination,
        AppointmentType::HealthScreening,
    ];
    
    for appointment_type in appointment_types {
        let patient_id = Uuid::new_v4();
        let request = SmartBookingRequest {
            patient_id,
            specialty: Some("general".to_string()),
            urgency: Some(BookingUrgency::Normal),
            preferred_doctor_id: None,
            preferred_time_slot: None,
            alternative_time_slots: None,
            appointment_type: Some(appointment_type.clone()),
            reason_for_visit: Some(format!("Test {:?} appointment", appointment_type)),
            consultation_mode: Some(ConsultationMode::InPerson),
            is_follow_up: Some(matches!(appointment_type, AppointmentType::FollowUpConsultation)),
            notes: None,
        };
        
        let auth_token = "test-auth-token";
        let result = producer.enqueue_smart_booking(request.clone(), auth_token).await;
        
        assert!(result.is_ok(), "Booking with {:?} type should be enqueued", appointment_type);
        
        let response = result.unwrap();
        let job = queue.get_job(response.job_id).await.expect("Failed to get job");
        assert!(job.is_some(), "Job with {:?} type should exist", appointment_type);
        
        let job = job.unwrap();
        assert_eq!(job.request.appointment_type, Some(appointment_type));
    }
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_enqueue_smart_booking_with_all_consultation_modes() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    let consultation_modes = vec![
        ConsultationMode::InPerson,
        ConsultationMode::Video,
        ConsultationMode::Phone,
        ConsultationMode::Hybrid,
    ];
    
    for consultation_mode in consultation_modes {
        let patient_id = Uuid::new_v4();
        let request = SmartBookingRequest {
            patient_id,
            specialty: Some("telemedicine".to_string()),
            urgency: Some(BookingUrgency::Normal),
            preferred_doctor_id: None,
            preferred_time_slot: None,
            alternative_time_slots: None,
            appointment_type: Some(AppointmentType::InitialConsultation),
            reason_for_visit: Some(format!("Test {:?} consultation", consultation_mode)),
            consultation_mode: Some(consultation_mode.clone()),
            is_follow_up: Some(false),
            notes: None,
        };
        
        let auth_token = "test-auth-token";
        let result = producer.enqueue_smart_booking(request.clone(), auth_token).await;
        
        assert!(result.is_ok(), "Booking with {:?} mode should be enqueued", consultation_mode);
        
        let response = result.unwrap();
        let job = queue.get_job(response.job_id).await.expect("Failed to get job");
        assert!(job.is_some(), "Job with {:?} mode should exist", consultation_mode);
        
        let job = job.unwrap();
        assert_eq!(job.request.consultation_mode, Some(consultation_mode));
    }
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_get_job_status_existing_job() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    // First enqueue a job
    let patient_id = Uuid::new_v4();
    let request = SmartBookingRequest {
        patient_id,
        specialty: Some("cardiology".to_string()),
        urgency: Some(BookingUrgency::Normal),
        preferred_doctor_id: None,
        preferred_time_slot: None,
        alternative_time_slots: None,
        appointment_type: Some(AppointmentType::InitialConsultation),
        reason_for_visit: Some("Routine checkup".to_string()),
        consultation_mode: Some(ConsultationMode::InPerson),
        is_follow_up: Some(false),
        notes: None,
    };
    
    let auth_token = "test-auth-token";
    let enqueue_result = producer.enqueue_smart_booking(request, auth_token).await;
    assert!(enqueue_result.is_ok(), "Job should be enqueued successfully");
    
    let job_id = enqueue_result.unwrap().job_id;
    
    // Now get job status
    let status_result = producer.get_job_status(job_id).await;
    assert!(status_result.is_ok(), "Getting job status should succeed");
    
    let job_option = status_result.unwrap();
    assert!(job_option.is_some(), "Job should exist");
    
    let job = job_option.unwrap();
    assert_eq!(job.job_id, job_id);
    assert_eq!(job.patient_id, patient_id);
    assert_eq!(job.status, BookingStatus::Queued);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_get_job_status_nonexistent_job() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue);
    
    let nonexistent_job_id = Uuid::new_v4();
    let result = producer.get_job_status(nonexistent_job_id).await;
    
    assert!(result.is_ok(), "Getting status of nonexistent job should not error");
    
    let job_option = result.unwrap();
    assert!(job_option.is_none(), "Nonexistent job should return None");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_cancel_job_success() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    // First enqueue a job
    let patient_id = Uuid::new_v4();
    let request = SmartBookingRequest {
        patient_id,
        specialty: Some("orthopedics".to_string()),
        urgency: Some(BookingUrgency::Normal),
        preferred_doctor_id: None,
        preferred_time_slot: None,
        alternative_time_slots: None,
        appointment_type: Some(AppointmentType::InitialConsultation),
        reason_for_visit: Some("Knee pain".to_string()),
        consultation_mode: Some(ConsultationMode::InPerson),
        is_follow_up: Some(false),
        notes: None,
    };
    
    let auth_token = "test-auth-token";
    let enqueue_result = producer.enqueue_smart_booking(request, auth_token).await;
    assert!(enqueue_result.is_ok(), "Job should be enqueued successfully");
    
    let job_id = enqueue_result.unwrap().job_id;
    
    // Cancel the job
    let cancel_result = producer.cancel_job(job_id).await;
    assert!(cancel_result.is_ok(), "Job cancellation should succeed");
    
    // Verify job is cancelled
    let job = queue.get_job(job_id).await.expect("Failed to get job");
    assert!(job.is_some(), "Cancelled job should still exist");
    
    let job = job.unwrap();
    assert_eq!(job.status, BookingStatus::Cancelled);
    assert!(job.completed_at.is_some(), "Cancelled job should have completion time");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_cancel_nonexistent_job() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue);
    
    let nonexistent_job_id = Uuid::new_v4();
    let result = producer.cancel_job(nonexistent_job_id).await;
    
    // Should fail for nonexistent job
    assert!(result.is_err(), "Cancelling nonexistent job should fail");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_retry_failed_job_success() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    // First enqueue a job
    let patient_id = Uuid::new_v4();
    let request = SmartBookingRequest {
        patient_id,
        specialty: Some("dermatology".to_string()),
        urgency: Some(BookingUrgency::Normal),
        preferred_doctor_id: None,
        preferred_time_slot: None,
        alternative_time_slots: None,
        appointment_type: Some(AppointmentType::InitialConsultation),
        reason_for_visit: Some("Skin examination".to_string()),
        consultation_mode: Some(ConsultationMode::InPerson),
        is_follow_up: Some(false),
        notes: None,
    };
    
    let auth_token = "test-auth-token";
    let enqueue_result = producer.enqueue_smart_booking(request, auth_token).await;
    assert!(enqueue_result.is_ok(), "Job should be enqueued successfully");
    
    let job_id = enqueue_result.unwrap().job_id;
    
    // Mark job as failed
    queue.update_job_status(job_id, BookingStatus::Failed, Some("Test failure".to_string())).await
        .expect("Failed to mark job as failed");
    
    // Retry the job
    let retry_result = producer.retry_failed_job(job_id).await;
    assert!(retry_result.is_ok(), "Job retry should succeed");
    
    // Verify job is retrying
    let job = queue.get_job(job_id).await.expect("Failed to get job");
    assert!(job.is_some(), "Retried job should exist");
    
    let job = job.unwrap();
    assert_eq!(job.status, BookingStatus::Retrying);
    assert_eq!(job.retry_count, 1);
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_retry_job_max_retries_exceeded() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = BookingProducerService::new(queue.clone());
    
    // Create job with max retries already reached
    let mut job = test_utils.create_test_job(None);
    job.retry_count = job.max_retries;
    queue.enqueue_job(&job).await.expect("Failed to enqueue job");
    
    // Mark as failed
    queue.update_job_status(job.job_id, BookingStatus::Failed, Some("Test failure".to_string())).await
        .expect("Failed to mark job as failed");
    
    // Try to retry - should fail
    let retry_result = producer.retry_failed_job(job.job_id).await;
    assert!(retry_result.is_err(), "Retry should fail when max retries exceeded");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_concurrent_job_enqueueing() {
    let test_utils = RedisTestUtils::new().await.expect("Failed to create test utils");
    let config = test_utils.create_test_config();
    
    let queue = Arc::new(RedisQueueService::new(&config).await.expect("Failed to create queue service"));
    let producer = Arc::new(BookingProducerService::new(queue.clone()));
    
    // Enqueue multiple jobs concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let producer_clone = Arc::clone(&producer);
        let patient_id = Uuid::new_v4();
        
        let handle = tokio::spawn(async move {
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
            
            producer_clone.enqueue_smart_booking(request, "test-token").await
        });
        handles.push(handle);
    }
    
    // Wait for all jobs to be enqueued
    let mut successful_enqueues = 0;
    for handle in handles {
        let result = handle.await.expect("Failed to join handle");
        if result.is_ok() {
            successful_enqueues += 1;
        }
    }
    
    assert_eq!(successful_enqueues, 10, "All 10 jobs should be enqueued successfully");
    
    // Verify queue stats
    let stats = queue.get_queue_stats().await;
    assert_eq!(stats.queued_jobs, 10, "Queue should contain 10 jobs");
    
    test_utils.cleanup().await.expect("Failed to cleanup");
}