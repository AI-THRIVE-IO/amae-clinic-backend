use std::sync::Arc;
use chrono::Utc;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{BookingJob, BookingJobResponse, BookingQueueError, BookingStatus};
use crate::services::queue::RedisQueueService;

pub struct BookingProducerService {
    queue: Arc<RedisQueueService>,
}

impl BookingProducerService {
    pub fn new(queue: Arc<RedisQueueService>) -> Self {
        Self { queue }
    }
    
    pub async fn enqueue_smart_booking(
        &self,
        request: crate::SmartBookingRequest,
        auth_token: &str,
    ) -> Result<BookingJobResponse, BookingQueueError> {
        let job = BookingJob::new(request.patient_id, request, auth_token.to_string());
        
        // Store job in queue
        self.queue.enqueue_job(&job).await?;
        
        let response = BookingJobResponse {
            job_id: job.job_id,
            status: BookingStatus::Queued,
            estimated_completion_time: job.estimate_completion_time(),
            websocket_channel: format!("booking_{}", job.job_id),
            tracking_url: format!("/appointments/booking-status/{}", job.job_id),
            retry_count: job.retry_count,
            max_retries: job.max_retries,
        };
        
        info!("Smart booking request queued for patient {} with job ID {}", 
              job.patient_id, job.job_id);
        
        Ok(response)
    }
    
    pub async fn get_job_status(&self, job_id: Uuid) -> Result<Option<BookingJob>, BookingQueueError> {
        self.queue.get_job(job_id).await
    }
    
    pub async fn cancel_job(&self, job_id: Uuid) -> Result<(), BookingQueueError> {
        self.queue.update_job_status(job_id, BookingStatus::Cancelled, Some("Cancelled by user".to_string())).await
    }
    
    pub async fn retry_failed_job(&self, job_id: Uuid) -> Result<(), BookingQueueError> {
        self.queue.retry_job(job_id).await
    }
}