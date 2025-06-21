use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingJob {
    pub job_id: Uuid,
    pub patient_id: Uuid,
    pub request: SmartBookingRequest,
    pub auth_token: String, // CRITICAL: Store JWT token for worker authentication
    pub status: BookingStatus,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartBookingRequest {
    pub patient_id: Uuid,
    pub specialty: Option<String>,
    pub urgency: Option<BookingUrgency>,
    pub preferred_doctor_id: Option<Uuid>,
    pub preferred_time_slot: Option<DateTime<Utc>>,
    pub alternative_time_slots: Option<Vec<DateTime<Utc>>>,
    pub appointment_type: Option<AppointmentType>,
    pub reason_for_visit: Option<String>,
    pub consultation_mode: Option<ConsultationMode>,
    pub is_follow_up: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartBookingResponse {
    pub appointment_id: Uuid,
    pub doctor_id: Uuid,
    pub doctor_first_name: String,
    pub doctor_last_name: String,
    pub scheduled_start_time: DateTime<Utc>,
    pub scheduled_end_time: DateTime<Utc>,
    pub appointment_type: AppointmentType,
    pub is_preferred_doctor: bool,
    pub match_score: f64,
    pub match_reasons: Vec<String>,
    pub alternative_slots: Vec<AlternativeSlot>,
    pub estimated_wait_time_minutes: Option<u32>,
    pub video_conference_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeSlot {
    pub doctor_id: Uuid,
    pub doctor_first_name: String,
    pub doctor_last_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub match_score: f64,
    pub is_urgent_slot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BookingUrgency {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppointmentType {
    InitialConsultation,
    FollowUpConsultation,
    Emergency,
    Wellness,
    Specialist,
    Procedure,
    Vaccination,
    HealthScreening,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConsultationMode {
    InPerson,
    Video,
    Phone,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BookingStatus {
    Queued,
    Processing,
    DoctorMatching,
    AvailabilityCheck,
    SlotSelection,
    AppointmentCreation,
    AlternativeGeneration,
    Completed,
    Failed,
    Retrying,
    Cancelled,
}

impl BookingStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, BookingStatus::Completed | BookingStatus::Failed | BookingStatus::Cancelled)
    }
    
    pub fn can_transition_to(&self, target: &BookingStatus) -> bool {
        use BookingStatus::*;
        match (self, target) {
            (Queued, Processing) => true,
            (Processing, DoctorMatching) => true,
            (DoctorMatching, AvailabilityCheck) => true,
            (AvailabilityCheck, SlotSelection) => true,
            (SlotSelection, AppointmentCreation) => true,
            (AppointmentCreation, AlternativeGeneration) => true,
            (AlternativeGeneration, Completed) => true,
            (_, Failed) => true,
            (_, Cancelled) => !self.is_terminal(),
            (Failed, Retrying) => true,
            (Retrying, Processing) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingJobResponse {
    pub job_id: Uuid,
    pub status: BookingStatus,
    pub estimated_completion_time: DateTime<Utc>,
    pub websocket_channel: String,
    pub tracking_url: String,
    pub retry_count: u32,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingResult {
    pub booking_response: SmartBookingResponse,
    pub processing_time_ms: u64,
    pub steps_completed: Vec<ProcessingStep>,
    pub performance_metrics: ProcessingMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingStep {
    pub step: BookingStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub result: StepResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepResult {
    Success(serde_json::Value),
    Warning(String),
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingMetrics {
    pub total_duration_ms: u64,
    pub doctor_matching_ms: u64,
    pub availability_check_ms: u64,
    pub slot_selection_ms: u64,
    pub appointment_creation_ms: u64,
    pub alternative_generation_ms: u64,
    pub database_queries: u32,
    pub cache_hits: u32,
    pub cache_misses: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingUpdate {
    pub job_id: Uuid,
    pub status: BookingStatus,
    pub message: String,
    pub progress_percentage: u8,
    pub current_step: Option<String>,
    pub estimated_remaining_seconds: Option<u64>,
    pub error_details: Option<String>,
    pub result: Option<BookingResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub queued_jobs: u64,
    pub processing_jobs: u64,
    pub completed_today: u64,
    pub failed_today: u64,
    pub average_processing_time_ms: f64,
    pub active_workers: u32,
    pub queue_health: QueueHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueueHealth {
    Healthy,
    Degraded { reason: String },
    Critical { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub max_concurrent_jobs: u32,
    pub job_timeout_seconds: u64,
    pub retry_delay_seconds: u64,
    pub health_check_interval_seconds: u64,
    pub graceful_shutdown_timeout_seconds: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: format!("worker-{}", Uuid::new_v4()),
            max_concurrent_jobs: 5,
            job_timeout_seconds: 120,
            retry_delay_seconds: 30,
            health_check_interval_seconds: 60,
            graceful_shutdown_timeout_seconds: 30,
        }
    }
}

impl BookingJob {
    pub fn new(patient_id: Uuid, request: SmartBookingRequest, auth_token: String) -> Self {
        let now = Utc::now();
        Self {
            job_id: Uuid::new_v4(),
            patient_id,
            request,
            auth_token, // CRITICAL: Store JWT token with job
            status: BookingStatus::Queued,
            retry_count: 0,
            max_retries: 3,
            created_at: now,
            updated_at: now,
            completed_at: None,
            error_message: None,
            worker_id: None,
        }
    }
    
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries && self.status == BookingStatus::Failed
    }
    
    pub fn estimate_completion_time(&self) -> DateTime<Utc> {
        match self.status {
            BookingStatus::Queued => Utc::now() + Duration::seconds(30),
            BookingStatus::Processing => Utc::now() + Duration::seconds(20),
            BookingStatus::DoctorMatching => Utc::now() + Duration::seconds(15),
            BookingStatus::AvailabilityCheck => Utc::now() + Duration::seconds(10),
            BookingStatus::SlotSelection => Utc::now() + Duration::seconds(8),
            BookingStatus::AppointmentCreation => Utc::now() + Duration::seconds(5),
            BookingStatus::AlternativeGeneration => Utc::now() + Duration::seconds(3),
            _ => Utc::now(),
        }
    }
}

impl BookingUpdate {
    pub fn new(job: &BookingJob, message: String) -> Self {
        let progress = match job.status {
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
        };
        
        Self {
            job_id: job.job_id,
            status: job.status.clone(),
            message,
            progress_percentage: progress,
            current_step: match job.status {
                BookingStatus::DoctorMatching => Some("Finding best doctor match".to_string()),
                BookingStatus::AvailabilityCheck => Some("Checking doctor availability".to_string()),
                BookingStatus::SlotSelection => Some("Selecting optimal time slot".to_string()),
                BookingStatus::AppointmentCreation => Some("Creating appointment".to_string()),
                BookingStatus::AlternativeGeneration => Some("Generating alternatives".to_string()),
                _ => None,
            },
            estimated_remaining_seconds: if job.status.is_terminal() {
                None
            } else {
                Some((job.estimate_completion_time() - Utc::now()).num_seconds() as u64)
            },
            error_details: job.error_message.clone(),
            result: None,
        }
    }
}