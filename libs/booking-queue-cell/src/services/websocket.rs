use std::sync::Arc;
use std::collections::HashMap;
use chrono::Utc;
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use serde_json;

use crate::{BookingQueueError, BookingStatus, BookingResult, BookingUpdate};

pub type WebSocketSender = broadcast::Sender<String>;
pub type WebSocketReceiver = broadcast::Receiver<String>;

pub struct WebSocketNotificationService {
    channels: Arc<RwLock<HashMap<Uuid, WebSocketSender>>>,
    global_sender: WebSocketSender,
}

impl WebSocketNotificationService {
    pub fn new() -> Self {
        let (global_sender, _) = broadcast::channel(1000);
        
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            global_sender,
        }
    }
    
    pub async fn create_channel(&self, job_id: Uuid) -> WebSocketReceiver {
        let (sender, receiver) = broadcast::channel(100);
        
        let mut channels = self.channels.write().await;
        channels.insert(job_id, sender);
        
        debug!("Created WebSocket channel for job {}", job_id);
        receiver
    }
    
    pub async fn remove_channel(&self, job_id: Uuid) {
        let mut channels = self.channels.write().await;
        channels.remove(&job_id);
        debug!("Removed WebSocket channel for job {}", job_id);
    }
    
    pub async fn send_booking_update(
        &self,
        job_id: Uuid,
        status: BookingStatus,
    ) -> Result<(), BookingQueueError> {
        let update = BookingUpdate {
            job_id,
            status: status.clone(),
            message: self.get_status_message(&status),
            progress_percentage: self.get_progress_percentage(&status),
            current_step: self.get_current_step(&status),
            estimated_remaining_seconds: self.get_estimated_remaining(&status),
            error_details: None,
            result: None,
        };
        
        self.send_update(job_id, &update).await
    }
    
    pub async fn send_booking_completion(
        &self,
        job_id: Uuid,
        result: BookingResult,
        message: String,
    ) -> Result<(), BookingQueueError> {
        let update = BookingUpdate {
            job_id,
            status: BookingStatus::Completed,
            message,
            progress_percentage: 100,
            current_step: None,
            estimated_remaining_seconds: None,
            error_details: None,
            result: Some(result),
        };
        
        self.send_update(job_id, &update).await
    }
    
    pub async fn send_booking_failure(
        &self,
        job_id: Uuid,
        error_message: String,
    ) -> Result<(), BookingQueueError> {
        let update = BookingUpdate {
            job_id,
            status: BookingStatus::Failed,
            message: "Booking failed".to_string(),
            progress_percentage: 100,
            current_step: None,
            estimated_remaining_seconds: None,
            error_details: Some(error_message),
            result: None,
        };
        
        self.send_update(job_id, &update).await
    }
    
    pub async fn send_custom_update(
        &self,
        job_id: Uuid,
        update: BookingUpdate,
    ) -> Result<(), BookingQueueError> {
        self.send_update(job_id, &update).await
    }
    
    pub fn subscribe_global(&self) -> WebSocketReceiver {
        self.global_sender.subscribe()
    }
    
    pub async fn get_active_channels(&self) -> Vec<Uuid> {
        let channels = self.channels.read().await;
        channels.keys().cloned().collect()
    }
    
    // Private helper methods
    
    async fn send_update(
        &self,
        job_id: Uuid,
        update: &BookingUpdate,
    ) -> Result<(), BookingQueueError> {
        let message = serde_json::to_string(update)
            .map_err(|e| BookingQueueError::SerializationError(e))?;
        
        // Send to specific job channel
        {
            let channels = self.channels.read().await;
            if let Some(sender) = channels.get(&job_id) {
                if let Err(e) = sender.send(message.clone()) {
                    warn!("Failed to send WebSocket message for job {}: {}", job_id, e);
                    // Channel might be closed, but we continue
                }
            }
        }
        
        // Send to global channel for monitoring
        let global_message = serde_json::json!({
            "type": "booking_update",
            "job_id": job_id,
            "timestamp": Utc::now().to_rfc3339(),
            "data": update
        }).to_string();
        
        if let Err(e) = self.global_sender.send(global_message) {
            debug!("Failed to send to global channel: {}", e);
            // Not critical, continue
        }
        
        debug!("Sent WebSocket update for job {} with status {:?}", job_id, update.status);
        Ok(())
    }
    
    fn get_status_message(&self, status: &BookingStatus) -> String {
        match status {
            BookingStatus::Queued => "Booking request queued for processing".to_string(),
            BookingStatus::Processing => "Processing your booking request".to_string(),
            BookingStatus::DoctorMatching => "Finding the best doctor for your needs".to_string(),
            BookingStatus::AvailabilityCheck => "Checking doctor availability".to_string(),
            BookingStatus::SlotSelection => "Selecting optimal appointment time".to_string(),
            BookingStatus::AppointmentCreation => "Creating your appointment".to_string(),
            BookingStatus::AlternativeGeneration => "Generating alternative options".to_string(),
            BookingStatus::Completed => "Appointment successfully booked".to_string(),
            BookingStatus::Failed => "Booking failed - please try again".to_string(),
            BookingStatus::Retrying => "Retrying booking request".to_string(),
            BookingStatus::Cancelled => "Booking request cancelled".to_string(),
        }
    }
    
    fn get_progress_percentage(&self, status: &BookingStatus) -> u8 {
        match status {
            BookingStatus::Queued => 0,
            BookingStatus::Processing => 10,
            BookingStatus::DoctorMatching => 25,
            BookingStatus::AvailabilityCheck => 40,
            BookingStatus::SlotSelection => 60,
            BookingStatus::AppointmentCreation => 80,
            BookingStatus::AlternativeGeneration => 90,
            BookingStatus::Completed => 100,
            BookingStatus::Failed | BookingStatus::Cancelled => 100,
            BookingStatus::Retrying => 5,
        }
    }
    
    fn get_current_step(&self, status: &BookingStatus) -> Option<String> {
        match status {
            BookingStatus::DoctorMatching => Some("Finding best doctor match".to_string()),
            BookingStatus::AvailabilityCheck => Some("Checking doctor availability".to_string()),
            BookingStatus::SlotSelection => Some("Selecting optimal time slot".to_string()),
            BookingStatus::AppointmentCreation => Some("Creating appointment".to_string()),
            BookingStatus::AlternativeGeneration => Some("Generating alternatives".to_string()),
            _ => None,
        }
    }
    
    fn get_estimated_remaining(&self, status: &BookingStatus) -> Option<u64> {
        if status.is_terminal() {
            None
        } else {
            Some(match status {
                BookingStatus::Queued => 30,
                BookingStatus::Processing => 25,
                BookingStatus::DoctorMatching => 20,
                BookingStatus::AvailabilityCheck => 15,
                BookingStatus::SlotSelection => 10,
                BookingStatus::AppointmentCreation => 5,
                BookingStatus::AlternativeGeneration => 3,
                _ => 0,
            })
        }
    }
}

impl Default for WebSocketNotificationService {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for WebSocketNotificationService {
    fn clone(&self) -> Self {
        Self {
            channels: Arc::clone(&self.channels),
            global_sender: self.global_sender.clone(),
        }
    }
}