use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

use shared_config::AppConfig;
use crate::{BookingQueueError, BookingStatus, WorkerConfig};
use crate::services::{
    queue::RedisQueueService,
    worker::BookingWorkerService,
    websocket::WebSocketNotificationService,
};

pub struct BookingConsumerService {
    worker_service: Arc<BookingWorkerService>,
    queue_service: Arc<RedisQueueService>,
    websocket_service: Arc<WebSocketNotificationService>,
    config: WorkerConfig,
    is_running: tokio::sync::RwLock<bool>,
}

impl BookingConsumerService {
    pub async fn new(
        config: WorkerConfig,
        app_config: Arc<AppConfig>,
    ) -> Result<Self, BookingQueueError> {
        let queue_service = Arc::new(RedisQueueService::new(&app_config).await?);
        let websocket_service = Arc::new(WebSocketNotificationService::new());
        
        let worker_service = Arc::new(BookingWorkerService::new(
            config.clone(),
            Arc::clone(&queue_service),
            app_config,
            Arc::clone(&websocket_service),
        ));
        
        Ok(Self {
            worker_service,
            queue_service,
            websocket_service,
            config,
            is_running: tokio::sync::RwLock::new(false),
        })
    }
    
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), BookingQueueError> {
        {
            let mut running = self.is_running.write().await;
            if *running {
                warn!("Consumer service is already running");
                return Ok(());
            }
            *running = true;
        }
        
        info!("Starting booking consumer service with worker {}", self.config.worker_id);
        
        // Start the worker service
        let worker_service = Arc::clone(&self.worker_service);
        let worker_handle = tokio::spawn(async move {
            if let Err(e) = worker_service.start().await {
                error!("Worker service failed: {}", e);
            }
        });
        
        // Start monitoring service
        let monitoring_service = self.clone_for_monitoring();
        let monitoring_handle = tokio::spawn(async move {
            monitoring_service.monitoring_loop().await
        });
        
        // Start cleanup service
        let cleanup_service = self.clone_for_cleanup();
        let cleanup_handle = tokio::spawn(async move {
            cleanup_service.cleanup_loop().await
        });
        
        // Wait for services to complete or shutdown signal
        tokio::select! {
            _ = worker_handle => {
                warn!("Worker service completed unexpectedly");
            }
            _ = monitoring_handle => {
                warn!("Monitoring service completed unexpectedly");
            }
            _ = cleanup_handle => {
                warn!("Cleanup service completed unexpectedly");
            }
            _ = self.wait_for_shutdown() => {
                info!("Shutdown signal received");
            }
        }
        
        // Graceful shutdown
        self.shutdown().await?;
        
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<(), BookingQueueError> {
        info!("Initiating consumer service shutdown");
        
        {
            let mut running = self.is_running.write().await;
            *running = false;
        }
        
        // Shutdown worker service
        if let Err(e) = self.worker_service.shutdown().await {
            error!("Failed to shutdown worker service: {}", e);
        }
        
        // Clean up active WebSocket channels
        let active_channels = self.websocket_service.get_active_channels().await;
        for job_id in active_channels {
            self.websocket_service.remove_channel(job_id).await;
        }
        
        info!("Consumer service shutdown complete");
        Ok(())
    }
    
    pub async fn enqueue_booking(
        &self,
        request: crate::SmartBookingRequest,
        auth_token: &str,
    ) -> Result<crate::BookingJobResponse, BookingQueueError> {
        let producer = crate::services::producer::BookingProducerService::new(
            Arc::clone(&self.queue_service)
        );
        
        producer.enqueue_smart_booking(request, auth_token).await
    }
    
    pub async fn get_job_status(&self, job_id: Uuid) -> Result<Option<crate::BookingJob>, BookingQueueError> {
        self.queue_service.get_job(job_id).await
    }
    
    pub async fn cancel_job(&self, job_id: Uuid) -> Result<(), BookingQueueError> {
        // Update job status
        self.queue_service.update_job_status(
            job_id, 
            BookingStatus::Cancelled, 
            Some("Cancelled by user request".to_string())
        ).await?;
        
        // Send cancellation notification
        self.websocket_service.send_booking_failure(
            job_id,
            "Booking was cancelled by user".to_string()
        ).await?;
        
        // Clean up WebSocket channel
        self.websocket_service.remove_channel(job_id).await;
        
        info!("Job {} cancelled by user", job_id);
        Ok(())
    }
    
    pub async fn retry_job(&self, job_id: Uuid) -> Result<(), BookingQueueError> {
        self.queue_service.retry_job(job_id).await
    }
    
    pub async fn get_queue_stats(&self) -> crate::QueueStats {
        self.queue_service.get_queue_stats().await
    }
    
    pub fn get_websocket_service(&self) -> Arc<WebSocketNotificationService> {
        Arc::clone(&self.websocket_service)
    }
    
    // Private helper methods
    
    async fn monitoring_loop(&self) -> Result<(), BookingQueueError> {
        let mut monitor_interval = interval(Duration::from_secs(self.config.health_check_interval_seconds));
        
        loop {
            monitor_interval.tick().await;
            
            if !*self.is_running.read().await {
                debug!("Monitoring loop stopping due to shutdown");
                break;
            }
            
            // Check queue health
            let stats = self.queue_service.get_queue_stats().await;
            
            debug!(
                "Queue health check - Queued: {}, Processing: {}, Completed today: {}, Failed today: {}",
                stats.queued_jobs, stats.processing_jobs, stats.completed_today, stats.failed_today
            );
            
            // Check for stuck jobs (jobs that have been processing too long)
            if let Err(e) = self.check_stuck_jobs().await {
                error!("Failed to check for stuck jobs: {}", e);
            }
            
            // Send health metrics to global channel if needed
            if matches!(stats.queue_health, crate::QueueHealth::Degraded { .. } | crate::QueueHealth::Critical { .. }) {
                warn!("Queue health is degraded: {:?}", stats.queue_health);
            }
        }
        
        debug!("Monitoring loop ended");
        Ok(())
    }
    
    async fn cleanup_loop(&self) -> Result<(), BookingQueueError> {
        let mut cleanup_interval = interval(Duration::from_secs(3600)); // Clean up every hour
        
        loop {
            cleanup_interval.tick().await;
            
            if !*self.is_running.read().await {
                debug!("Cleanup loop stopping due to shutdown");
                break;
            }
            
            // Clean up expired jobs
            match self.queue_service.cleanup_expired_jobs().await {
                Ok(cleaned) => {
                    if cleaned > 0 {
                        info!("Cleaned up {} expired jobs", cleaned);
                    }
                }
                Err(e) => {
                    error!("Failed to cleanup expired jobs: {}", e);
                }
            }
            
            // Clean up orphaned WebSocket channels
            self.cleanup_orphaned_channels().await;
        }
        
        debug!("Cleanup loop ended");
        Ok(())
    }
    
    async fn check_stuck_jobs(&self) -> Result<(), BookingQueueError> {
        // This would implement logic to find jobs that have been in "Processing" state
        // for too long and either retry them or mark them as failed
        // For now, we'll just log a debug message
        debug!("Checking for stuck jobs...");
        Ok(())
    }
    
    async fn cleanup_orphaned_channels(&self) {
        let active_channels = self.websocket_service.get_active_channels().await;
        let mut orphaned_count = 0;
        
        for job_id in active_channels {
            // Check if job still exists and is active
            match self.queue_service.get_job(job_id).await {
                Ok(Some(job)) => {
                    if job.status.is_terminal() {
                        self.websocket_service.remove_channel(job_id).await;
                        orphaned_count += 1;
                    }
                }
                Ok(None) => {
                    // Job doesn't exist, remove channel
                    self.websocket_service.remove_channel(job_id).await;
                    orphaned_count += 1;
                }
                Err(e) => {
                    warn!("Failed to check job {} for channel cleanup: {}", job_id, e);
                }
            }
        }
        
        if orphaned_count > 0 {
            debug!("Cleaned up {} orphaned WebSocket channels", orphaned_count);
        }
    }
    
    async fn wait_for_shutdown(&self) {
        loop {
            if !*self.is_running.read().await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    fn clone_for_monitoring(&self) -> Self {
        Self {
            worker_service: Arc::clone(&self.worker_service),
            queue_service: Arc::clone(&self.queue_service),
            websocket_service: Arc::clone(&self.websocket_service),
            config: self.config.clone(),
            is_running: tokio::sync::RwLock::new(true),
        }
    }
    
    fn clone_for_cleanup(&self) -> Self {
        Self {
            worker_service: Arc::clone(&self.worker_service),
            queue_service: Arc::clone(&self.queue_service),
            websocket_service: Arc::clone(&self.websocket_service),
            config: self.config.clone(),
            is_running: tokio::sync::RwLock::new(true),
        }
    }
}