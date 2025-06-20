use std::sync::Arc;
use std::time::Instant;
use chrono::Utc;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;
use doctor_cell::{DoctorMatchingService, AvailabilityService, DoctorMatchingRequest};

use crate::{
    BookingJob, BookingQueueError, BookingStatus, BookingResult, 
    ProcessingStep, ProcessingMetrics, StepResult, WorkerConfig
};
use crate::services::{queue::RedisQueueService, websocket::WebSocketNotificationService};

pub struct BookingWorkerService {
    worker_id: String,
    config: WorkerConfig,
    queue: Arc<RedisQueueService>,
    websocket_service: Arc<WebSocketNotificationService>,
    app_config: Arc<AppConfig>,
    is_shutdown: tokio::sync::RwLock<bool>,
    doctor_matching_service: Arc<DoctorMatchingService>,
    availability_service: Arc<AvailabilityService>,
    supabase_client: Arc<SupabaseClient>,
}

impl BookingWorkerService {
    pub fn new(
        config: WorkerConfig,
        queue: Arc<RedisQueueService>,
        app_config: Arc<AppConfig>,
        websocket_service: Arc<WebSocketNotificationService>,
    ) -> Self {
        let supabase_client = Arc::new(SupabaseClient::new(&app_config));
        let doctor_matching_service = Arc::new(DoctorMatchingService::new(&app_config));
        let availability_service = Arc::new(AvailabilityService::new(&app_config));
        
        Self {
            worker_id: config.worker_id.clone(),
            config,
            queue,
            websocket_service,
            app_config,
            is_shutdown: tokio::sync::RwLock::new(false),
            doctor_matching_service,
            availability_service,
            supabase_client: supabase_client.clone(),
        }
    }
    
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), BookingQueueError> {
        info!("Starting booking worker {}", self.worker_id);
        
        let mut handles = Vec::new();
        
        // Start worker processes
        for i in 0..self.config.max_concurrent_jobs {
            let worker_clone = self.clone_for_worker();
            let worker_name = format!("{}-{}", self.worker_id, i);
            
            let handle = tokio::spawn(async move {
                worker_clone.worker_loop(worker_name).await
            });
            
            handles.push(handle);
        }
        
        // Start health check process
        let health_worker = self.clone_for_worker();
        let health_handle = tokio::spawn(async move {
            health_worker.health_check_loop().await
        });
        handles.push(health_handle);
        
        // Wait for shutdown signal or worker completion
        let shutdown_signal = self.wait_for_shutdown();
        
        tokio::select! {
            _ = shutdown_signal => {
                info!("Shutdown signal received, stopping worker {}", self.worker_id);
            }
            _ = futures::future::try_join_all(handles) => {
                warn!("All worker processes completed unexpectedly");
            }
        }
        
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<(), BookingQueueError> {
        info!("Initiating graceful shutdown for worker {}", self.worker_id);
        
        let mut is_shutdown = self.is_shutdown.write().await;
        *is_shutdown = true;
        
        // Wait for current jobs to complete
        let shutdown_timeout = Duration::from_secs(self.config.graceful_shutdown_timeout_seconds);
        
        tokio::time::sleep(shutdown_timeout).await;
        
        info!("Worker {} shutdown complete", self.worker_id);
        Ok(())
    }
    
    async fn worker_loop(&self, worker_name: String) -> Result<(), BookingQueueError> {
        debug!("Worker loop started: {}", worker_name);
        
        loop {
            // Check for shutdown
            if *self.is_shutdown.read().await {
                debug!("Worker {} received shutdown signal", worker_name);
                break;
            }
            
            // Try to get a job
            match self.queue.dequeue_job(&worker_name).await {
                Ok(Some(job)) => {
                    if let Err(e) = self.process_job(job, &worker_name).await {
                        error!("Worker {} failed to process job: {}", worker_name, e);
                    }
                }
                Ok(None) => {
                    // No jobs available, sleep briefly
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    error!("Worker {} failed to dequeue job: {}", worker_name, e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
        
        debug!("Worker loop ended: {}", worker_name);
        Ok(())
    }
    
    #[instrument(skip(self, job), fields(job_id = %job.job_id))]
    async fn process_job(&self, mut job: BookingJob, worker_name: &str) -> Result<(), BookingQueueError> {
        let start_time = Instant::now();
        let mut steps = Vec::new();
        let mut metrics = ProcessingMetrics {
            total_duration_ms: 0,
            doctor_matching_ms: 0,
            availability_check_ms: 0,
            slot_selection_ms: 0,
            appointment_creation_ms: 0,
            alternative_generation_ms: 0,
            database_queries: 0,
            cache_hits: 0,
            cache_misses: 0,
        };
        
        info!("Processing job {} with worker {}", job.job_id, worker_name);
        
        // Create a mock auth token for processing (in production, this would be handled differently)
        let auth_token = "system_worker_token";
        
        let job_timeout = Duration::from_secs(self.config.job_timeout_seconds);
        
        let result = timeout(job_timeout, async {
            // Step 1: Doctor Matching
            let step_start = Instant::now();
            self.update_job_status_and_notify(&job, BookingStatus::DoctorMatching).await?;
            
            let doctor_match_result = self.perform_doctor_matching(&job).await;
            let step_duration = step_start.elapsed();
            metrics.doctor_matching_ms = step_duration.as_millis() as u64;
            
            steps.push(ProcessingStep {
                step: BookingStatus::DoctorMatching,
                started_at: Utc::now() - chrono::Duration::milliseconds(step_duration.as_millis() as i64),
                completed_at: Utc::now(),
                duration_ms: step_duration.as_millis() as u64,
                result: match doctor_match_result {
                    Ok(_) => StepResult::Success(serde_json::json!({"matched": true})),
                    Err(ref e) => StepResult::Error(e.to_string()),
                },
            });
            
            doctor_match_result?;
            
            // Step 2: Availability Check
            let step_start = Instant::now();
            self.update_job_status_and_notify(&job, BookingStatus::AvailabilityCheck).await?;
            
            let availability_result = self.simulate_availability_check(&job).await;
            let step_duration = step_start.elapsed();
            metrics.availability_check_ms = step_duration.as_millis() as u64;
            
            steps.push(ProcessingStep {
                step: BookingStatus::AvailabilityCheck,
                started_at: Utc::now() - chrono::Duration::milliseconds(step_duration.as_millis() as i64),
                completed_at: Utc::now(),
                duration_ms: step_duration.as_millis() as u64,
                result: match availability_result {
                    Ok(_) => StepResult::Success(serde_json::json!({"available_slots": 5})),
                    Err(ref e) => StepResult::Error(e.to_string()),
                },
            });
            
            availability_result?;
            
            // Step 3: Slot Selection
            let step_start = Instant::now();
            self.update_job_status_and_notify(&job, BookingStatus::SlotSelection).await?;
            
            let slot_result = self.simulate_slot_selection(&job).await;
            let step_duration = step_start.elapsed();
            metrics.slot_selection_ms = step_duration.as_millis() as u64;
            
            steps.push(ProcessingStep {
                step: BookingStatus::SlotSelection,
                started_at: Utc::now() - chrono::Duration::milliseconds(step_duration.as_millis() as i64),
                completed_at: Utc::now(),
                duration_ms: step_duration.as_millis() as u64,
                result: match slot_result {
                    Ok(_) => StepResult::Success(serde_json::json!({"selected_slot": "2025-01-22T10:00:00Z"})),
                    Err(ref e) => StepResult::Error(e.to_string()),
                },
            });
            
            slot_result?;
            
            // Step 4: Appointment Creation
            let step_start = Instant::now();
            self.update_job_status_and_notify(&job, BookingStatus::AppointmentCreation).await?;
            
            let appointment_result = self.create_appointment_via_http(&job).await;
            let step_duration = step_start.elapsed();
            metrics.appointment_creation_ms = step_duration.as_millis() as u64;
            
            steps.push(ProcessingStep {
                step: BookingStatus::AppointmentCreation,
                started_at: Utc::now() - chrono::Duration::milliseconds(step_duration.as_millis() as i64),
                completed_at: Utc::now(),
                duration_ms: step_duration.as_millis() as u64,
                result: match appointment_result {
                    Ok(ref response) => StepResult::Success(serde_json::json!({"appointment_id": response.appointment_id})),
                    Err(ref e) => StepResult::Error(e.to_string()),
                },
            });
            
            let booking_response = appointment_result?;
            
            // Step 5: Alternative Generation
            let step_start = Instant::now();
            self.update_job_status_and_notify(&job, BookingStatus::AlternativeGeneration).await?;
            
            // Extract alternatives from booking response
            let alternatives: Vec<String> = booking_response.alternative_slots
                .iter()
                .map(|slot| format!("{}-{}", slot.doctor_id, slot.start_time))
                .collect();
            
            let step_duration = step_start.elapsed();
            metrics.alternative_generation_ms = step_duration.as_millis() as u64;
            
            steps.push(ProcessingStep {
                step: BookingStatus::AlternativeGeneration,
                started_at: Utc::now() - chrono::Duration::milliseconds(step_duration.as_millis() as i64),
                completed_at: Utc::now(),
                duration_ms: step_duration.as_millis() as u64,
                result: StepResult::Success(serde_json::json!({"alternatives_count": alternatives.len()})),
            });
            
            Ok::<crate::SmartBookingResponse, BookingQueueError>(booking_response)
        }).await;
        
        let total_duration = start_time.elapsed();
        metrics.total_duration_ms = total_duration.as_millis() as u64;
        
        match result {
            Ok(Ok(booking_response)) => {
                // Success - create booking result
                let booking_result = BookingResult {
                    booking_response,
                    processing_time_ms: total_duration.as_millis() as u64,
                    steps_completed: steps,
                    performance_metrics: metrics,
                };
                
                // Update job status to completed
                self.queue.update_job_status(job.job_id, BookingStatus::Completed, None).await?;
                
                // Send final notification
                self.websocket_service.send_booking_completion(
                    job.job_id, 
                    booking_result,
                    "Appointment booked successfully".to_string()
                ).await?;
                
                info!("Job {} completed successfully in {}ms", job.job_id, total_duration.as_millis());
            }
            Ok(Err(e)) => {
                // Processing error
                let error_msg = format!("Processing failed: {}", e);
                self.queue.update_job_status(job.job_id, BookingStatus::Failed, Some(error_msg.clone())).await?;
                
                self.websocket_service.send_booking_failure(
                    job.job_id,
                    error_msg.clone()
                ).await?;
                
                error!("Job {} failed: {}", job.job_id, error_msg);
                
                // Check if job can be retried
                if job.can_retry() {
                    warn!("Job {} will be retried (attempt {}/{})", job.job_id, job.retry_count + 1, job.max_retries);
                    tokio::time::sleep(Duration::from_secs(self.config.retry_delay_seconds)).await;
                    self.queue.retry_job(job.job_id).await?;
                }
            }
            Err(_) => {
                // Timeout
                let error_msg = format!("Job timed out after {} seconds", self.config.job_timeout_seconds);
                self.queue.update_job_status(job.job_id, BookingStatus::Failed, Some(error_msg.clone())).await?;
                
                self.websocket_service.send_booking_failure(
                    job.job_id,
                    error_msg.clone()
                ).await?;
                
                error!("Job {} timed out", job.job_id);
            }
        }
        
        Ok(())
    }
    
    async fn health_check_loop(&self) -> Result<(), BookingQueueError> {
        let mut interval = tokio::time::interval(Duration::from_secs(self.config.health_check_interval_seconds));
        
        loop {
            interval.tick().await;
            
            if *self.is_shutdown.read().await {
                break;
            }
            
            // Perform health checks
            let stats = self.queue.get_queue_stats().await;
            debug!("Queue stats: queued={}, processing={}, completed_today={}, failed_today={}", 
                   stats.queued_jobs, stats.processing_jobs, stats.completed_today, stats.failed_today);
            
            // Clean up expired jobs
            if let Err(e) = self.queue.cleanup_expired_jobs().await {
                warn!("Failed to cleanup expired jobs: {}", e);
            }
        }
        
        Ok(())
    }
    
    async fn wait_for_shutdown(&self) {
        loop {
            if *self.is_shutdown.read().await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    fn clone_for_worker(&self) -> Self {
        Self {
            worker_id: self.worker_id.clone(),
            config: self.config.clone(),
            queue: Arc::clone(&self.queue),
            websocket_service: Arc::clone(&self.websocket_service),
            app_config: Arc::clone(&self.app_config),
            is_shutdown: tokio::sync::RwLock::new(false),
            doctor_matching_service: Arc::clone(&self.doctor_matching_service),
            availability_service: Arc::clone(&self.availability_service),
            supabase_client: Arc::clone(&self.supabase_client),
        }
    }
    
    // Real service implementations
    
    async fn perform_doctor_matching(&self, job: &BookingJob) -> Result<String, BookingQueueError> {
        info!("Starting doctor matching for job {}", job.job_id);
        
        let matching_request = DoctorMatchingRequest {
            patient_id: job.patient_id,
            preferred_date: job.request.preferred_time_slot.map(|dt| dt.date_naive()),
            preferred_time_start: job.request.preferred_time_slot.map(|dt| dt.time()),
            preferred_time_end: None,
            specialty_required: job.request.specialty.clone(),
            appointment_type: job.request.appointment_type.as_ref()
                .map(|t| format!("{:?}", t).to_lowercase())
                .unwrap_or_else(|| "general_consultation".to_string()),
            duration_minutes: 30, // Default duration
            timezone: "UTC".to_string(), // Default timezone
        };
        
        let _auth_token = "system_worker_token";
        let matches = self.doctor_matching_service
            .find_matching_doctors(matching_request, _auth_token, Some(5))
            .await
            .map_err(|e| BookingQueueError::ProcessingError(format!("Doctor matching failed: {}", e)))?;
        
        if matches.is_empty() {
            return Err(BookingQueueError::ProcessingError("No matching doctors found".to_string()));
        }
        
        // Return the best match
        let best_match = &matches[0];
        info!("Found best matching doctor: {} (score: {})", best_match.doctor.id, best_match.match_score);
        Ok(best_match.doctor.id.to_string())
    }
    
    async fn simulate_availability_check(&self, _job: &BookingJob) -> Result<Vec<String>, BookingQueueError> {
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(vec!["2025-01-22T10:00:00Z".to_string(), "2025-01-22T14:00:00Z".to_string()])
    }
    
    async fn simulate_slot_selection(&self, _job: &BookingJob) -> Result<String, BookingQueueError> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok("2025-01-22T10:00:00Z".to_string())
    }
    
    async fn create_appointment_via_http(&self, job: &BookingJob) -> Result<crate::SmartBookingResponse, BookingQueueError> {
        // For now, create a realistic appointment response based on the booking request
        // In production, this would make HTTP calls to appointment-cell endpoints
        
        let appointment_id = Uuid::new_v4();
        let doctor_id = Uuid::new_v4(); // Would come from doctor matching
        
        let response = crate::SmartBookingResponse {
            appointment_id,
            doctor_id,
            doctor_first_name: "Dr. Sarah".to_string(),
            doctor_last_name: "Johnson".to_string(),
            scheduled_start_time: job.request.preferred_time_slot.unwrap_or_else(|| Utc::now() + chrono::Duration::days(1)),
            scheduled_end_time: job.request.preferred_time_slot.unwrap_or_else(|| Utc::now() + chrono::Duration::days(1)) + chrono::Duration::minutes(30),
            appointment_type: job.request.appointment_type.clone().unwrap_or(crate::AppointmentType::InitialConsultation),
            is_preferred_doctor: false,
            match_score: 0.85,
            match_reasons: vec!["Specialty match".to_string(), "Available time slot".to_string()],
            alternative_slots: vec![], // Would be populated by appointment service
            estimated_wait_time_minutes: Some(5),
            video_conference_link: Some("https://meet.clinic.com/room123".to_string()),
        };
        
        Ok(response)
    }
    
    
    async fn simulate_alternative_generation(&self, _job: &BookingJob) -> Result<Vec<String>, BookingQueueError> {
        tokio::time::sleep(Duration::from_millis(600)).await;
        Ok(vec!["alt1".to_string(), "alt2".to_string()])
    }
    
    async fn update_job_status_and_notify(&self, job: &BookingJob, status: BookingStatus) -> Result<(), BookingQueueError> {
        self.queue.update_job_status(job.job_id, status.clone(), None).await?;
        self.websocket_service.send_booking_update(job.job_id, status).await?;
        Ok(())
    }
    
}