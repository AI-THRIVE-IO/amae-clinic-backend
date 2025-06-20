use thiserror::Error;

#[derive(Error, Debug)]
pub enum BookingQueueError {
    #[error("Queue operation failed: {0}")]
    QueueError(String),
    
    #[error("Job not found: {0}")]
    JobNotFound(String),
    
    #[error("Invalid job status transition from {from} to {to}")]
    InvalidStatusTransition { from: String, to: String },
    
    #[error("Booking processing failed: {0}")]
    BookingError(String),
    
    #[error("Redis connection error: {0}")]
    RedisError(#[from] redis::RedisError),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Worker timeout: operation took longer than {timeout_seconds} seconds")]
    WorkerTimeout { timeout_seconds: u64 },
    
    #[error("Maximum retry attempts ({max_retries}) exceeded for job {job_id}")]
    MaxRetriesExceeded { job_id: String, max_retries: u32 },
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Processing error: {0}")]
    ProcessingError(String),
}