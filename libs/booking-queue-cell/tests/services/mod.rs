use chrono::Utc;
use uuid::Uuid;
use tokio::time::Duration;
use redis::AsyncCommands;
use deadpool_redis::{Config, Runtime, Pool};

use booking_queue_cell::*;
use shared_config::AppConfig;

/// Test utilities for Redis-based queue testing
pub struct RedisTestUtils {
    pub pool: Pool,
    pub test_prefix: String,
}

impl RedisTestUtils {
    /// Create a new Redis test utility with isolated test namespace
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let redis_url = std::env::var("REDIS_TEST_URL")
            .unwrap_or_else(|_| "redis://default:Gfl83sisAvInYg61FHNJLnZkCJsxFO7v@redis-15236.c59.eu-west-1-2.ec2.redns.redis-cloud.com:15236".to_string());
        
        let cfg = Config::from_url(redis_url);
        let pool = cfg.create_pool(Some(Runtime::Tokio1))?;
        
        // Test connection
        let mut conn = pool.get().await?;
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        
        let test_prefix = format!("test_{}", Uuid::new_v4().to_string().replace("-", ""));
        
        Ok(Self {
            pool,
            test_prefix,
        })
    }
    
    /// Get a connection with test namespace isolation
    pub async fn get_connection(&self) -> Result<deadpool_redis::Connection, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.pool.get().await?)
    }
    
    /// Clean up all test data
    pub async fn cleanup(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.get_connection().await?;
        let pattern = format!("{}*", self.test_prefix);
        let keys: Vec<String> = conn.keys(pattern).await?;
        
        if !keys.is_empty() {
            let _: () = conn.del(keys).await?;
        }
        
        Ok(())
    }
    
    /// Create test config with Redis URL
    pub fn create_test_config(&self) -> AppConfig {
        AppConfig {
            supabase_url: "http://localhost:54321".to_string(),
            supabase_anon_key: "test-anon-key".to_string(),
            supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
            cloudflare_realtime_app_id: "test-app-id".to_string(),
            cloudflare_realtime_api_token: "test-token".to_string(),
            cloudflare_realtime_base_url: "https://test.cloudflare.com/v1".to_string(),
            redis_url: Some("redis://default:Gfl83sisAvInYg61FHNJLnZkCJsxFO7v@redis-15236.c59.eu-west-1-2.ec2.redns.redis-cloud.com:15236".to_string()),
        }
    }
    
    /// Create a test booking job
    pub fn create_test_job(&self, patient_id: Option<Uuid>) -> BookingJob {
        let patient_id = patient_id.unwrap_or_else(Uuid::new_v4);
        let request = SmartBookingRequest {
            patient_id,
            specialty: Some("cardiology".to_string()),
            urgency: Some(BookingUrgency::Normal),
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
        
        BookingJob::new(patient_id, request)
    }
    
    /// Wait for job status change with timeout
    pub async fn wait_for_job_status(
        &self,
        queue: &RedisQueueService,
        job_id: Uuid,
        expected_status: BookingStatus,
        timeout_secs: u64,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let timeout_duration = Duration::from_secs(timeout_secs);
        let start = std::time::Instant::now();
        
        loop {
            if start.elapsed() > timeout_duration {
                return Ok(false);
            }
            
            if let Ok(Some(job)) = queue.get_job(job_id).await {
                if job.status == expected_status {
                    return Ok(true);
                }
            }
            
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    /// Assert Redis key exists
    pub async fn assert_key_exists(&self, key: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.get_connection().await?;
        let exists: bool = conn.exists(key).await?;
        Ok(exists)
    }
    
    /// Get Redis key value
    pub async fn get_key_value(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.get_connection().await?;
        let value: Option<String> = conn.get(key).await?;
        Ok(value)
    }
}

// Test modules
mod queue_test;
mod worker_test;
mod producer_test;
mod consumer_test;
mod websocket_test;