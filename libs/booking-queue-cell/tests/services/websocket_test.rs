use std::sync::Arc;
use tokio::time::{Duration, timeout};
use uuid::Uuid;
use chrono::Utc;

use booking_queue_cell::*;

#[tokio::test]
async fn test_websocket_service_initialization() {
    let websocket_service = WebSocketNotificationService::new();
    
    // Service should be created successfully
    let active_channels = websocket_service.get_active_channels().await;
    assert_eq!(active_channels.len(), 0, "New service should have no active channels");
}

#[tokio::test]
async fn test_websocket_service_default_trait() {
    let websocket_service = WebSocketNotificationService::default();
    
    // Default implementation should work the same as new()
    let active_channels = websocket_service.get_active_channels().await;
    assert_eq!(active_channels.len(), 0, "Default service should have no active channels");
}

#[tokio::test]
async fn test_websocket_service_clone() {
    let websocket_service = WebSocketNotificationService::new();
    let job_id = Uuid::new_v4();
    
    // Create a channel
    let _receiver = websocket_service.create_channel(job_id).await;
    
    // Clone the service
    let cloned_service = websocket_service.clone();
    
    // Both services should see the same channels
    let original_channels = websocket_service.get_active_channels().await;
    let cloned_channels = cloned_service.get_active_channels().await;
    
    assert_eq!(original_channels, cloned_channels, "Cloned service should have same channels");
    assert!(original_channels.contains(&job_id), "Both services should see the created channel");
}

#[tokio::test]
async fn test_create_and_remove_channel() {
    let websocket_service = WebSocketNotificationService::new();
    let job_id = Uuid::new_v4();
    
    // Initially no channels
    let initial_channels = websocket_service.get_active_channels().await;
    assert_eq!(initial_channels.len(), 0, "Should start with no channels");
    
    // Create a channel
    let _receiver = websocket_service.create_channel(job_id).await;
    
    let channels_after_create = websocket_service.get_active_channels().await;
    assert_eq!(channels_after_create.len(), 1, "Should have one channel after creation");
    assert!(channels_after_create.contains(&job_id), "Should contain the created job ID");
    
    // Remove the channel
    websocket_service.remove_channel(job_id).await;
    
    let channels_after_remove = websocket_service.get_active_channels().await;
    assert_eq!(channels_after_remove.len(), 0, "Should have no channels after removal");
}

#[tokio::test]
async fn test_multiple_channels() {
    let websocket_service = WebSocketNotificationService::new();
    let mut job_ids = vec![];
    let mut receivers = vec![];
    
    // Create multiple channels
    for _ in 0..5 {
        let job_id = Uuid::new_v4();
        job_ids.push(job_id);
        let receiver = websocket_service.create_channel(job_id).await;
        receivers.push(receiver);
    }
    
    let active_channels = websocket_service.get_active_channels().await;
    assert_eq!(active_channels.len(), 5, "Should have 5 active channels");
    
    for job_id in &job_ids {
        assert!(active_channels.contains(job_id), "Should contain job ID: {}", job_id);
    }
    
    // Remove channels one by one
    for job_id in &job_ids {
        websocket_service.remove_channel(*job_id).await;
    }
    
    let final_channels = websocket_service.get_active_channels().await;
    assert_eq!(final_channels.len(), 0, "Should have no channels after all removals");
}

#[tokio::test]
async fn test_send_booking_update() {
    let websocket_service = WebSocketNotificationService::new();
    let job_id = Uuid::new_v4();
    
    // Create a channel
    let mut receiver = websocket_service.create_channel(job_id).await;
    
    // Send a booking update
    let result = websocket_service.send_booking_update(job_id, BookingStatus::Processing).await;
    assert!(result.is_ok(), "Sending booking update should succeed");
    
    // Receive the message
    let message_result = timeout(Duration::from_secs(1), receiver.recv()).await;
    assert!(message_result.is_ok(), "Should receive message within timeout");
    
    let message = message_result.unwrap();
    assert!(message.is_ok(), "Message should be received successfully");
    
    let message_content = message.unwrap();
    assert!(message_content.contains("Processing"), "Message should contain status");
    assert!(message_content.contains(&job_id.to_string()), "Message should contain job ID");
    
    // Clean up
    websocket_service.remove_channel(job_id).await;
}

#[tokio::test]
async fn test_send_booking_completion() {
    let websocket_service = WebSocketNotificationService::new();
    let job_id = Uuid::new_v4();
    
    // Create a channel
    let mut receiver = websocket_service.create_channel(job_id).await;
    
    // Create a mock booking result
    let booking_result = BookingResult {
        booking_response: SmartBookingResponse {
            appointment_id: Uuid::new_v4(),
            doctor_id: Uuid::new_v4(),
            doctor_first_name: "Dr. John".to_string(),
            doctor_last_name: "Smith".to_string(),
            scheduled_start_time: Utc::now() + chrono::Duration::hours(24),
            scheduled_end_time: Utc::now() + chrono::Duration::hours(24) + chrono::Duration::minutes(30),
            appointment_type: AppointmentType::InitialConsultation,
            is_preferred_doctor: true,
            match_score: 0.95,
            match_reasons: vec!["Previous consultation history".to_string()],
            alternative_slots: vec![],
            estimated_wait_time_minutes: None,
            video_conference_link: Some("https://meet.example.com/room123".to_string()),
        },
        processing_time_ms: 5000,
        steps_completed: vec![],
        performance_metrics: ProcessingMetrics {
            total_duration_ms: 5000,
            doctor_matching_ms: 1000,
            availability_check_ms: 1500,
            slot_selection_ms: 1000,
            appointment_creation_ms: 1000,
            alternative_generation_ms: 500,
            database_queries: 10,
            cache_hits: 5,
            cache_misses: 3,
        },
    };
    
    // Send completion notification
    let result = websocket_service.send_booking_completion(
        job_id,
        booking_result,
        "Booking completed successfully".to_string(),
    ).await;
    assert!(result.is_ok(), "Sending booking completion should succeed");
    
    // Receive the message
    let message_result = timeout(Duration::from_secs(1), receiver.recv()).await;
    assert!(message_result.is_ok(), "Should receive completion message within timeout");
    
    let message = message_result.unwrap();
    assert!(message.is_ok(), "Completion message should be received successfully");
    
    let message_content = message.unwrap();
    assert!(message_content.contains("Completed"), "Message should contain completed status");
    assert!(message_content.contains("Dr. John"), "Message should contain doctor name");
    
    // Clean up
    websocket_service.remove_channel(job_id).await;
}

#[tokio::test]
async fn test_send_booking_failure() {
    let websocket_service = WebSocketNotificationService::new();
    let job_id = Uuid::new_v4();
    
    // Create a channel
    let mut receiver = websocket_service.create_channel(job_id).await;
    
    // Send failure notification
    let error_message = "Doctor not available at requested time";
    let result = websocket_service.send_booking_failure(job_id, error_message.to_string()).await;
    assert!(result.is_ok(), "Sending booking failure should succeed");
    
    // Receive the message
    let message_result = timeout(Duration::from_secs(1), receiver.recv()).await;
    assert!(message_result.is_ok(), "Should receive failure message within timeout");
    
    let message = message_result.unwrap();
    assert!(message.is_ok(), "Failure message should be received successfully");
    
    let message_content = message.unwrap();
    assert!(message_content.contains("Failed"), "Message should contain failed status");
    assert!(message_content.contains(error_message), "Message should contain error message");
    
    // Clean up
    websocket_service.remove_channel(job_id).await;
}

#[tokio::test]
async fn test_send_custom_update() {
    let websocket_service = WebSocketNotificationService::new();
    let job_id = Uuid::new_v4();
    
    // Create a channel
    let mut receiver = websocket_service.create_channel(job_id).await;
    
    // Create custom update
    let custom_update = BookingUpdate {
        job_id,
        status: BookingStatus::DoctorMatching,
        message: "Finding the best doctor for your needs".to_string(),
        progress_percentage: 25,
        current_step: Some("Analyzing doctor specializations".to_string()),
        estimated_remaining_seconds: Some(180),
        error_details: None,
        result: None,
    };
    
    // Send custom update
    let result = websocket_service.send_custom_update(job_id, custom_update.clone()).await;
    assert!(result.is_ok(), "Sending custom update should succeed");
    
    // Receive the message
    let message_result = timeout(Duration::from_secs(1), receiver.recv()).await;
    assert!(message_result.is_ok(), "Should receive custom message within timeout");
    
    let message = message_result.unwrap();
    assert!(message.is_ok(), "Custom message should be received successfully");
    
    let message_content = message.unwrap();
    assert!(message_content.contains("DoctorMatching"), "Message should contain custom status");
    assert!(message_content.contains("Finding the best doctor"), "Message should contain custom message");
    assert!(message_content.contains("25"), "Message should contain progress percentage");
    
    // Clean up
    websocket_service.remove_channel(job_id).await;
}

#[tokio::test]
async fn test_global_subscription() {
    let websocket_service = WebSocketNotificationService::new();
    let job_id = Uuid::new_v4();
    
    // Subscribe to global channel
    let mut global_receiver = websocket_service.subscribe_global();
    
    // Create a specific channel
    let _specific_receiver = websocket_service.create_channel(job_id).await;
    
    // Send an update
    let result = websocket_service.send_booking_update(job_id, BookingStatus::AvailabilityCheck).await;
    assert!(result.is_ok(), "Sending update should succeed");
    
    // Receive on global channel
    let global_message_result = timeout(Duration::from_secs(1), global_receiver.recv()).await;
    assert!(global_message_result.is_ok(), "Should receive global message within timeout");
    
    let global_message = global_message_result.unwrap();
    assert!(global_message.is_ok(), "Global message should be received successfully");
    
    let global_message_content = global_message.unwrap();
    assert!(global_message_content.contains("booking_update"), "Global message should contain update type");
    assert!(global_message_content.contains(&job_id.to_string()), "Global message should contain job ID");
    assert!(global_message_content.contains("AvailabilityCheck"), "Global message should contain status");
    
    // Clean up
    websocket_service.remove_channel(job_id).await;
}

#[tokio::test]
async fn test_multiple_receivers_same_channel() {
    let websocket_service = WebSocketNotificationService::new();
    let job_id = Uuid::new_v4();
    
    // Create multiple receivers for the same job
    let mut receiver1 = websocket_service.create_channel(job_id).await;
    // Note: Creating another channel for the same job_id will replace the previous one
    let mut receiver2 = websocket_service.create_channel(job_id).await;
    
    // Send an update
    let result = websocket_service.send_booking_update(job_id, BookingStatus::SlotSelection).await;
    assert!(result.is_ok(), "Sending update should succeed");
    
    // Only the latest receiver should get the message (since it replaced the first one)
    let receiver2_result = timeout(Duration::from_millis(500), receiver2.recv()).await;
    assert!(receiver2_result.is_ok(), "Latest receiver should get the message");
    
    let receiver1_result = timeout(Duration::from_millis(100), receiver1.recv()).await;
    assert!(receiver1_result.is_err(), "Original receiver should not get message (channel replaced)");
    
    // Clean up
    websocket_service.remove_channel(job_id).await;
}

#[tokio::test]
async fn test_websocket_status_messages() {
    let websocket_service = WebSocketNotificationService::new();
    
    // Test all status messages
    let statuses = vec![
        (BookingStatus::Queued, "Booking request queued for processing"),
        (BookingStatus::Processing, "Processing your booking request"),
        (BookingStatus::DoctorMatching, "Finding the best doctor for your needs"),
        (BookingStatus::AvailabilityCheck, "Checking doctor availability"),
        (BookingStatus::SlotSelection, "Selecting optimal appointment time"),
        (BookingStatus::AppointmentCreation, "Creating your appointment"),
        (BookingStatus::AlternativeGeneration, "Generating alternative options"),
        (BookingStatus::Completed, "Appointment successfully booked"),
        (BookingStatus::Failed, "Booking failed - please try again"),
        (BookingStatus::Retrying, "Retrying booking request"),
        (BookingStatus::Cancelled, "Booking request cancelled"),
    ];
    
    for (status, expected_message) in statuses {
        let job_id = Uuid::new_v4();
        let mut receiver = websocket_service.create_channel(job_id).await;
        
        // Send update with this status
        let result = websocket_service.send_booking_update(job_id, status.clone()).await;
        assert!(result.is_ok(), "Sending update for {:?} should succeed", status);
        
        // Receive and verify message
        let message_result = timeout(Duration::from_secs(1), receiver.recv()).await;
        assert!(message_result.is_ok(), "Should receive message for {:?}", status);
        
        let message = message_result.unwrap().unwrap();
        assert!(message.contains(expected_message), 
               "Message for {:?} should contain '{}'", status, expected_message);
        
        websocket_service.remove_channel(job_id).await;
    }
}

#[tokio::test]
async fn test_websocket_progress_percentages() {
    let websocket_service = WebSocketNotificationService::new();
    
    // Test progress percentages for different statuses
    let status_progress = vec![
        (BookingStatus::Queued, 0),
        (BookingStatus::Processing, 10),
        (BookingStatus::DoctorMatching, 25),
        (BookingStatus::AvailabilityCheck, 40),
        (BookingStatus::SlotSelection, 60),
        (BookingStatus::AppointmentCreation, 80),
        (BookingStatus::AlternativeGeneration, 90),
        (BookingStatus::Completed, 100),
        (BookingStatus::Failed, 100),
        (BookingStatus::Cancelled, 100),
        (BookingStatus::Retrying, 5),
    ];
    
    for (status, expected_progress) in status_progress {
        let job_id = Uuid::new_v4();
        let mut receiver = websocket_service.create_channel(job_id).await;
        
        // Send update with this status
        let result = websocket_service.send_booking_update(job_id, status.clone()).await;
        assert!(result.is_ok(), "Sending update for {:?} should succeed", status);
        
        // Receive and verify progress
        let message_result = timeout(Duration::from_secs(1), receiver.recv()).await;
        assert!(message_result.is_ok(), "Should receive message for {:?}", status);
        
        let message = message_result.unwrap().unwrap();
        let progress_str = format!("\"progress_percentage\":{}", expected_progress);
        assert!(message.contains(&progress_str), 
               "Message for {:?} should contain progress {}", status, expected_progress);
        
        websocket_service.remove_channel(job_id).await;
    }
}

#[tokio::test]
async fn test_websocket_current_step_messages() {
    let websocket_service = WebSocketNotificationService::new();
    
    // Test current step messages for processing statuses
    let status_steps = vec![
        (BookingStatus::DoctorMatching, Some("Finding best doctor match")),
        (BookingStatus::AvailabilityCheck, Some("Checking doctor availability")),
        (BookingStatus::SlotSelection, Some("Selecting optimal time slot")),
        (BookingStatus::AppointmentCreation, Some("Creating appointment")),
        (BookingStatus::AlternativeGeneration, Some("Generating alternatives")),
        (BookingStatus::Queued, None),
        (BookingStatus::Processing, None),
        (BookingStatus::Completed, None),
    ];
    
    for (status, expected_step) in status_steps {
        let job_id = Uuid::new_v4();
        let mut receiver = websocket_service.create_channel(job_id).await;
        
        // Send update with this status
        let result = websocket_service.send_booking_update(job_id, status.clone()).await;
        assert!(result.is_ok(), "Sending update for {:?} should succeed", status);
        
        // Receive and verify step message
        let message_result = timeout(Duration::from_secs(1), receiver.recv()).await;
        assert!(message_result.is_ok(), "Should receive message for {:?}", status);
        
        let message = message_result.unwrap().unwrap();
        
        if let Some(step) = expected_step {
            assert!(message.contains(step), 
                   "Message for {:?} should contain step '{}'", status, step);
        } else {
            assert!(message.contains("\"current_step\":null"), 
                   "Message for {:?} should have null current_step", status);
        }
        
        websocket_service.remove_channel(job_id).await;
    }
}

#[tokio::test]
async fn test_websocket_estimated_remaining_time() {
    let websocket_service = WebSocketNotificationService::new();
    
    // Test estimated remaining time for different statuses
    let status_times = vec![
        (BookingStatus::Queued, Some(30)),
        (BookingStatus::Processing, Some(25)),
        (BookingStatus::DoctorMatching, Some(20)),
        (BookingStatus::AvailabilityCheck, Some(15)),
        (BookingStatus::SlotSelection, Some(10)),
        (BookingStatus::AppointmentCreation, Some(5)),
        (BookingStatus::AlternativeGeneration, Some(3)),
        (BookingStatus::Completed, None),
        (BookingStatus::Failed, None),
        (BookingStatus::Cancelled, None),
    ];
    
    for (status, expected_time) in status_times {
        let job_id = Uuid::new_v4();
        let mut receiver = websocket_service.create_channel(job_id).await;
        
        // Send update with this status
        let result = websocket_service.send_booking_update(job_id, status.clone()).await;
        assert!(result.is_ok(), "Sending update for {:?} should succeed", status);
        
        // Receive and verify estimated time
        let message_result = timeout(Duration::from_secs(1), receiver.recv()).await;
        assert!(message_result.is_ok(), "Should receive message for {:?}", status);
        
        let message = message_result.unwrap().unwrap();
        
        if let Some(time) = expected_time {
            let time_str = format!("\"estimated_remaining_seconds\":{}", time);
            assert!(message.contains(&time_str), 
                   "Message for {:?} should contain estimated time {}", status, time);
        } else {
            assert!(message.contains("\"estimated_remaining_seconds\":null"), 
                   "Message for {:?} should have null estimated time", status);
        }
        
        websocket_service.remove_channel(job_id).await;
    }
}

#[tokio::test]
async fn test_websocket_concurrent_operations() {
    let websocket_service = Arc::new(WebSocketNotificationService::new());
    let mut handles = vec![];
    let mut job_ids = vec![];
    
    // Create multiple channels concurrently
    for _i in 0..10 {
        let service_clone = Arc::clone(&websocket_service);
        let handle = tokio::spawn(async move {
            let job_id = Uuid::new_v4();
            let _receiver = service_clone.create_channel(job_id).await;
            job_id
        });
        handles.push(handle);
    }
    
    // Wait for all channels to be created
    for handle in handles {
        let job_id = handle.await.expect("Failed to join handle");
        job_ids.push(job_id);
    }
    
    // Verify all channels were created
    let active_channels = websocket_service.get_active_channels().await;
    assert_eq!(active_channels.len(), 10, "Should have 10 active channels");
    
    for job_id in &job_ids {
        assert!(active_channels.contains(job_id), "Should contain job ID: {}", job_id);
    }
    
    // Send messages to all channels concurrently
    let mut send_handles = vec![];
    for job_id in &job_ids {
        let service_clone = Arc::clone(&websocket_service);
        let job_id = *job_id;
        let handle = tokio::spawn(async move {
            service_clone.send_booking_update(job_id, BookingStatus::Processing).await
        });
        send_handles.push(handle);
    }
    
    // Wait for all messages to be sent
    for handle in send_handles {
        let result = handle.await.expect("Failed to join send handle");
        assert!(result.is_ok(), "All message sends should succeed");
    }
    
    // Clean up all channels
    for job_id in &job_ids {
        websocket_service.remove_channel(*job_id).await;
    }
    
    let final_channels = websocket_service.get_active_channels().await;
    assert_eq!(final_channels.len(), 0, "Should have no channels after cleanup");
}