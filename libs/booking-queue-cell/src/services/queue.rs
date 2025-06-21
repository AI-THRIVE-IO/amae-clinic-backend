use anyhow::Result;
use chrono::Utc;
use deadpool_redis::{Config, Runtime, Pool, Connection};
use redis::AsyncCommands;
use serde_json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{BookingJob, BookingQueueError, BookingStatus, QueueStats, QueueHealth};
use shared_config::AppConfig;

pub struct RedisQueueService {
    pool: Pool,
    config: Arc<AppConfig>,
    stats: Arc<RwLock<QueueStats>>,
}

impl RedisQueueService {
    pub async fn new(config: &AppConfig) -> Result<Self, BookingQueueError> {
        let redis_url = config.redis_url.clone()
            .unwrap_or_else(|| "redis://localhost:6379".to_string());
        
        let cfg = Config::from_url(redis_url);
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).map_err(|e| {
            BookingQueueError::RedisError(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Failed to create Redis pool",
                format!("Pool creation error: {}", e),
            )))
        })?;
        
        // Test connection
        let mut conn = pool.get().await.map_err(|e| {
            BookingQueueError::RedisError(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Failed to connect to Redis",
                format!("Connection error: {}", e),
            )))
        })?;
        
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        info!("Redis queue service initialized successfully");
        
        let stats = Arc::new(RwLock::new(QueueStats {
            queued_jobs: 0,
            processing_jobs: 0,
            completed_today: 0,
            failed_today: 0,
            average_processing_time_ms: 0.0,
            active_workers: 0,
            queue_health: QueueHealth::Healthy,
        }));
        
        Ok(Self {
            pool,
            config: Arc::new(config.clone()),
            stats,
        })
    }
    
    pub async fn enqueue_job(&self, job: &BookingJob) -> Result<(), BookingQueueError> {
        let mut conn = self.get_connection().await?;
        
        // Serialize job
        let job_data = serde_json::to_string(job)?;
        
        // Store job details in hash
        let job_key = format!("booking_job:{}", job.job_id);
        let _: () = conn.hset_multiple(&job_key, &[
            ("data", job_data.as_str()),
            ("status", &serde_json::to_string(&job.status)?),
            ("created_at", &job.created_at.to_rfc3339()),
            ("patient_id", &job.patient_id.to_string()),
        ]).await?;
        
        // Set expiration (7 days)
        let _: () = conn.expire(&job_key, 604800).await?;
        
        // Add to processing queue
        let queue_key = "booking_queue:pending";
        let _: () = conn.lpush(queue_key, job.job_id.to_string()).await?;
        
        // Update stats
        self.increment_queued_jobs().await;
        
        debug!("Job {} enqueued successfully", job.job_id);
        Ok(())
    }
    
    pub async fn dequeue_job(&self, worker_id: &str) -> Result<Option<BookingJob>, BookingQueueError> {
        let mut conn = self.get_connection().await?;
        
        // Atomic pop from pending queue and push to processing queue
        let queue_pending = "booking_queue:pending";
        let queue_processing = "booking_queue:processing";
        
        let job_id: Option<String> = conn.brpoplpush(queue_pending, queue_processing, 1.0).await?;
        
        if let Some(job_id_str) = job_id {
            let job_key = format!("booking_job:{}", job_id_str);
            
            // Get job data
            let job_data: Option<String> = conn.hget(&job_key, "data").await?;
            
            if let Some(data) = job_data {
                let mut job: BookingJob = serde_json::from_str(&data)?;
                
                // Update job with worker assignment
                job.worker_id = Some(worker_id.to_string());
                job.status = BookingStatus::Processing;
                job.updated_at = Utc::now();
                
                // Update stored job
                self.update_job_in_redis(&mut conn, &job).await?;
                
                // Update stats
                self.move_job_to_processing().await;
                
                debug!("Job {} dequeued by worker {}", job.job_id, worker_id);
                return Ok(Some(job));
            }
        }
        
        Ok(None)
    }
    
    pub async fn update_job_status(
        &self,
        job_id: Uuid,
        status: BookingStatus,
        error_message: Option<String>,
    ) -> Result<(), BookingQueueError> {
        let mut conn = self.get_connection().await?;
        let job_key = format!("booking_job:{}", job_id);
        
        // Get current job
        let job_data: Option<String> = conn.hget(&job_key, "data").await?;
        
        if let Some(data) = job_data {
            let mut job: BookingJob = serde_json::from_str(&data)?;
            
            // Validate status transition
            if !job.status.can_transition_to(&status) {
                return Err(BookingQueueError::InvalidStatusTransition {
                    from: format!("{:?}", job.status),
                    to: format!("{:?}", status),
                });
            }
            
            let old_status = job.status.clone();
            job.status = status.clone();
            job.updated_at = Utc::now();
            job.error_message = error_message;
            
            if status.is_terminal() {
                job.completed_at = Some(Utc::now());
                
                // Remove from processing queue
                let queue_processing = "booking_queue:processing";
                let _: () = conn.lrem(queue_processing, 1, job_id.to_string()).await?;
                
                // Update stats
                self.complete_job(&status).await;
            }
            
            // Update stored job
            self.update_job_in_redis(&mut conn, &job).await?;
            
            debug!("Job {} status updated from {:?} to {:?}", job_id, old_status, status);
            Ok(())
        } else {
            Err(BookingQueueError::JobNotFound(job_id.to_string()))
        }
    }
    
    pub async fn get_job(&self, job_id: Uuid) -> Result<Option<BookingJob>, BookingQueueError> {
        let mut conn = self.get_connection().await?;
        let job_key = format!("booking_job:{}", job_id);
        
        let job_data: Option<String> = conn.hget(&job_key, "data").await?;
        
        if let Some(data) = job_data {
            let job: BookingJob = serde_json::from_str(&data)?;
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }
    
    pub async fn retry_job(&self, job_id: Uuid) -> Result<(), BookingQueueError> {
        let mut conn = self.get_connection().await?;
        let job_key = format!("booking_job:{}", job_id);
        
        let job_data: Option<String> = conn.hget(&job_key, "data").await?;
        
        if let Some(data) = job_data {
            let mut job: BookingJob = serde_json::from_str(&data)?;
            
            if !job.can_retry() {
                return Err(BookingQueueError::MaxRetriesExceeded {
                    job_id: job_id.to_string(),
                    max_retries: job.max_retries,
                });
            }
            
            job.retry_count += 1;
            job.status = BookingStatus::Retrying;
            job.updated_at = Utc::now();
            job.error_message = None;
            job.worker_id = None;
            
            // Update stored job
            self.update_job_in_redis(&mut conn, &job).await?;
            
            // Re-enqueue job
            let queue_key = "booking_queue:pending";
            let _: () = conn.lpush(queue_key, job.job_id.to_string()).await?;
            
            info!("Job {} retried (attempt {}/{})", job_id, job.retry_count, job.max_retries);
            Ok(())
        } else {
            Err(BookingQueueError::JobNotFound(job_id.to_string()))
        }
    }
    
    pub async fn get_queue_stats(&self) -> QueueStats {
        let stats = self.stats.read().await;
        stats.clone()
    }
    
    pub async fn cleanup_expired_jobs(&self) -> Result<u64, BookingQueueError> {
        let mut conn = self.get_connection().await?;
        
        // Find jobs older than 7 days
        let cutoff = Utc::now() - chrono::Duration::days(7);
        let pattern = "booking_job:*";
        
        let keys: Vec<String> = conn.keys(pattern).await?;
        let mut cleaned = 0;
        
        for key in keys {
            let created_at_str: Option<String> = conn.hget(&key, "created_at").await?;
            
            if let Some(created_str) = created_at_str {
                if let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(&created_str) {
                    if created_at.with_timezone(&Utc) < cutoff {
                        let _: () = conn.del(&key).await?;
                        cleaned += 1;
                    }
                }
            }
        }
        
        if cleaned > 0 {
            info!("Cleaned up {} expired jobs", cleaned);
        } else {
            debug!("No expired jobs found to clean up");
        }
        Ok(cleaned)
    }
    
    // Private helper methods
    
    async fn get_connection(&self) -> Result<Connection, BookingQueueError> {
        self.pool.get().await.map_err(|e| {
            BookingQueueError::RedisError(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Failed to get Redis connection",
                e.to_string(),
            )))
        })
    }
    
    async fn update_job_in_redis(
        &self,
        conn: &mut Connection,
        job: &BookingJob,
    ) -> Result<(), BookingQueueError> {
        let job_key = format!("booking_job:{}", job.job_id);
        let job_data = serde_json::to_string(job)?;
        
        let _: () = conn.hset_multiple(&job_key, &[
            ("data", job_data.as_str()),
            ("status", &serde_json::to_string(&job.status)?),
            ("updated_at", &job.updated_at.to_rfc3339()),
        ]).await?;
        
        Ok(())
    }
    
    async fn increment_queued_jobs(&self) {
        let mut stats = self.stats.write().await;
        stats.queued_jobs += 1;
    }
    
    async fn move_job_to_processing(&self) {
        let mut stats = self.stats.write().await;
        if stats.queued_jobs > 0 {
            stats.queued_jobs -= 1;
        }
        stats.processing_jobs += 1;
    }
    
    async fn complete_job(&self, status: &BookingStatus) {
        let mut stats = self.stats.write().await;
        if stats.processing_jobs > 0 {
            stats.processing_jobs -= 1;
        }
        
        match status {
            BookingStatus::Completed => stats.completed_today += 1,
            BookingStatus::Failed => stats.failed_today += 1,
            _ => {}
        }
    }
}