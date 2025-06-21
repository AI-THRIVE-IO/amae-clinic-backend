// libs/appointment-cell/src/models.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc, NaiveDate, NaiveTime};
use std::fmt;

// ==============================================================================
// CORE APPOINTMENT MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appointment {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub doctor_id: Uuid,
    pub appointment_date: DateTime<Utc>,
    pub status: AppointmentStatus,
    pub appointment_type: AppointmentType,
    pub duration_minutes: i32,
    pub timezone: String,
    pub actual_start_time: Option<DateTime<Utc>>,
    pub actual_end_time: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub patient_notes: Option<String>,
    pub doctor_notes: Option<String>,
    pub prescription_issued: bool,
    pub medical_certificate_issued: bool,
    pub report_generated: bool,
    pub video_conference_link: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Appointment {
    /// Calculate the scheduled end time based on appointment_date and duration
    pub fn scheduled_end_time(&self) -> DateTime<Utc> {
        self.appointment_date + chrono::Duration::minutes(self.duration_minutes as i64)
    }
    
    /// Get the scheduled start time (alias for appointment_date for backward compatibility)
    pub fn scheduled_start_time(&self) -> DateTime<Utc> {
        self.appointment_date
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AppointmentStatus {
    Pending,
    Confirmed,
    Ready,        // NEW: Video session created, ready to join (15-30 min before)
    InProgress,
    Completed,
    Cancelled,
    NoShow,
    Rescheduled,
}

impl fmt::Display for AppointmentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppointmentStatus::Pending => write!(f, "pending"),
            AppointmentStatus::Confirmed => write!(f, "confirmed"),
            AppointmentStatus::Ready => write!(f, "ready"),
            AppointmentStatus::InProgress => write!(f, "in_progress"),
            AppointmentStatus::Completed => write!(f, "completed"),
            AppointmentStatus::Cancelled => write!(f, "cancelled"),
            AppointmentStatus::NoShow => write!(f, "no_show"),
            AppointmentStatus::Rescheduled => write!(f, "rescheduled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "PascalCase")]
pub enum AppointmentType {
    // Primary enum variants (PascalCase for API standards)
    #[serde(alias = "initial_consultation", alias = "initial", alias = "new_patient")]
    InitialConsultation,
    
    #[serde(alias = "follow_up_consultation", alias = "followup")]
    FollowUpConsultation,
    
    #[serde(alias = "emergency_consultation", alias = "emergency")]
    EmergencyConsultation,
    
    #[serde(alias = "prescription_renewal", alias = "medication_renewal")]
    PrescriptionRenewal,
    
    #[serde(alias = "specialty_consultation", alias = "specialist")]
    SpecialtyConsultation,
    
    #[serde(alias = "group_session", alias = "workshop")]
    GroupSession,
    
    #[serde(alias = "telehealth_checkin", alias = "telehealth", alias = "remote_checkin", alias = "virtual")]
    TelehealthCheckIn,
    
    // Legacy variants maintained for complete backward compatibility with internal code
    #[serde(alias = "general_consultation", alias = "consultation", alias = "general")]
    GeneralConsultation,
    
    #[serde(alias = "follow_up")]
    FollowUp,
    
    #[serde(alias = "prescription")]
    Prescription,
    
    #[serde(alias = "urgent")]
    Urgent,
    
    #[serde(alias = "medical_certificate")]
    MedicalCertificate,
    
    #[serde(alias = "mental_health", alias = "psychology", alias = "psychiatry")]
    MentalHealth,
    
    #[serde(alias = "womens_health", alias = "gynecology", alias = "obstetrics")]
    WomensHealth,
}

impl fmt::Display for AppointmentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppointmentType::InitialConsultation => write!(f, "InitialConsultation"),
            AppointmentType::FollowUpConsultation => write!(f, "FollowUpConsultation"),
            AppointmentType::EmergencyConsultation => write!(f, "EmergencyConsultation"),
            AppointmentType::PrescriptionRenewal => write!(f, "PrescriptionRenewal"),
            AppointmentType::SpecialtyConsultation => write!(f, "SpecialtyConsultation"),
            AppointmentType::GroupSession => write!(f, "GroupSession"),
            AppointmentType::TelehealthCheckIn => write!(f, "TelehealthCheckIn"),
            AppointmentType::GeneralConsultation => write!(f, "GeneralConsultation"),
            AppointmentType::FollowUp => write!(f, "FollowUp"),
            AppointmentType::Prescription => write!(f, "Prescription"),
            AppointmentType::Urgent => write!(f, "Urgent"),
            AppointmentType::MedicalCertificate => write!(f, "MedicalCertificate"),
            AppointmentType::MentalHealth => write!(f, "MentalHealth"),
            AppointmentType::WomensHealth => write!(f, "WomensHealth"),
        }
    }
}

// ==============================================================================
// REQUEST/RESPONSE MODELS  
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookAppointmentRequest {
    pub patient_id: Uuid,
    pub doctor_id: Option<Uuid>, // Made optional - system can find best doctor
    pub appointment_date: DateTime<Utc>,
    pub appointment_type: AppointmentType,
    pub duration_minutes: i32,
    pub timezone: String,
    pub patient_notes: Option<String>,
    pub preferred_language: Option<String>,
    pub specialty_required: Option<String>, // Added for specialty validation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartBookingRequest {
    pub patient_id: Uuid,
    pub preferred_date: Option<NaiveDate>,
    pub preferred_time_start: Option<NaiveTime>,
    pub preferred_time_end: Option<NaiveTime>,
    pub appointment_type: AppointmentType,
    pub duration_minutes: i32,
    pub timezone: String,
    pub specialty_required: Option<String>,
    pub patient_notes: Option<String>,
    pub allow_history_prioritization: Option<bool>, // Enable doctor history matching
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAppointmentRequest {
    pub status: Option<AppointmentStatus>,
    pub doctor_notes: Option<String>,
    pub patient_notes: Option<String>,
    pub reschedule_to: Option<DateTime<Utc>>,
    pub reschedule_duration: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RescheduleAppointmentRequest {
    pub new_start_time: DateTime<Utc>,
    pub new_duration_minutes: Option<i32>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelAppointmentRequest {
    pub reason: String,
    pub cancelled_by: CancelledBy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelledBy {
    Patient,
    Doctor,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppointmentSearchQuery {
    pub patient_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub status: Option<AppointmentStatus>,
    pub appointment_type: Option<AppointmentType>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

// ==============================================================================
// ENHANCED BOOKING RESPONSE MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartBookingResponse {
    pub appointment: Appointment,
    pub doctor_match_score: f32,
    pub match_reasons: Vec<String>,
    pub is_preferred_doctor: bool, // True if patient has seen this doctor before
    pub alternative_slots: Vec<AlternativeSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeSlot {
    pub doctor_id: Uuid,
    pub doctor_first_name: String,
    pub doctor_last_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub match_score: f32,
    pub has_patient_history: bool,
}

impl AlternativeSlot {
    pub fn doctor_full_name(&self) -> String {
        format!("{} {}", self.doctor_first_name, self.doctor_last_name)
    }
}

// ==============================================================================
// CONFLICT DETECTION MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictCheckRequest {
    pub doctor_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub exclude_appointment_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictCheckResponse {
    pub has_conflict: bool,
    pub conflicting_appointments: Vec<Appointment>,
    pub suggested_alternatives: Vec<SuggestedSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedSlot {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub doctor_id: Uuid,
    pub appointment_type: AppointmentType,
}

// ==============================================================================
// STATISTICS AND SUMMARY MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppointmentSummary {
    pub id: Uuid,
    pub patient_first_name: String,
    pub patient_last_name: String,
    pub doctor_first_name: String,
    pub doctor_last_name: String,
    pub appointment_date: DateTime<Utc>,
    pub status: AppointmentStatus,
    pub appointment_type: AppointmentType,
    pub duration_minutes: i32,
}

impl AppointmentSummary {
    pub fn patient_full_name(&self) -> String {
        format!("{} {}", self.patient_first_name, self.patient_last_name)
    }
    
    pub fn doctor_full_name(&self) -> String {
        format!("{} {}", self.doctor_first_name, self.doctor_last_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppointmentStats {
    pub total_appointments: i32,
    pub completed_appointments: i32,
    pub cancelled_appointments: i32,
    pub no_show_appointments: i32,
    pub average_consultation_duration: i32,
    pub appointment_type_breakdown: Vec<(AppointmentType, i32)>,
    pub doctor_continuity_rate: f32, // % of appointments with previously seen doctors
}

// ==============================================================================
// ENHANCED ERROR TYPES
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AppointmentError {
    #[error("Appointment not found")]
    NotFound,
    
    #[error("Appointment slot not available")]
    SlotNotAvailable,
    
    #[error("No {specialty} doctors available at this time")]
    SpecialtyNotAvailable { specialty: String },
    
    #[error("Doctor not available at requested time")]
    DoctorNotAvailable,
    
    #[error("Patient not found")]
    PatientNotFound,
    
    #[error("Doctor not found")]
    DoctorNotFound,
    
    #[error("Invalid appointment time: {0}")]
    InvalidTime(String),
    
    #[error("Appointment cannot be modified in current status: {0}")]
    InvalidStatusTransition(AppointmentStatus),
    
    #[error("Appointment conflicts with existing booking")]
    ConflictDetected,
    
    #[error("Unauthorized access to appointment")]
    Unauthorized,
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("External service error: {0}")]
    ExternalServiceError(String),
    
    #[error("Doctor matching service error: {0}")]
    DoctorMatchingError(String),
    
    #[error("Video session not found")]
    VideoSessionNotFound,
    
    #[error("Video session creation failed")]
    VideoSessionCreationFailed,
    
    #[error("Video session operation failed: {0}")]
    VideoSessionError(String),
    
    #[error("Video conferencing service unavailable")]
    VideoServiceUnavailable,
}

// ==============================================================================
// VALIDATION MODELS
// ==============================================================================

#[derive(Debug, Clone)]
pub struct AppointmentValidationRules {
    pub min_advance_booking_hours: i32,
    pub max_advance_booking_days: i32,
    pub allowed_cancellation_hours: i32,
    pub allowed_reschedule_hours: i32,
    pub max_appointments_per_day: i32,
    pub min_appointment_duration: i32,
    pub max_appointment_duration: i32,
    pub enable_history_prioritization: bool, // New flag for history-based matching
}

impl Default for AppointmentValidationRules {
    fn default() -> Self {
        Self {
            min_advance_booking_hours: 2,
            max_advance_booking_days: 90,
            allowed_cancellation_hours: 24,
            allowed_reschedule_hours: 48,
            max_appointments_per_day: 3,
            min_appointment_duration: 15,
            max_appointment_duration: 120,
            enable_history_prioritization: true, // Enable by default
        }
    }
}

// ==============================================================================
// SCHEDULING CONSISTENCY MODELS
// ==============================================================================

/// Distributed scheduling lock for preventing race conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingLock {
    pub id: Uuid,
    pub lock_key: String,
    pub doctor_id: Uuid,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub process_id: String,
}

/// Result of comprehensive consistency check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyCheckResult {
    pub is_consistent: bool,
    pub issues: Vec<String>,
    pub recommendations: Vec<String>,
    pub suggested_alternatives: Vec<SuggestedSlot>,
}

/// Enhanced appointment conflict check request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedConflictCheckRequest {
    pub doctor_id: Uuid,
    pub patient_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub appointment_type: AppointmentType,
    pub exclude_appointment_id: Option<Uuid>,
    pub check_buffer_time: bool,
    pub buffer_minutes: Option<i32>,
}

/// Scheduling analytics and monitoring data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingMetrics {
    pub total_bookings_attempted: u64,
    pub successful_bookings: u64,
    pub failed_bookings: u64,
    pub conflict_rate: f64,
    pub average_booking_time_ms: f64,
    pub peak_concurrency: u32,
    pub lock_contention_events: u64,
    pub timestamp: DateTime<Utc>,
}

// ==============================================================================
// VIDEO CONFERENCE INTEGRATION MODELS
// ==============================================================================

/// Enhanced appointment model with video session management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppointmentWithVideo {
    pub appointment: Appointment,
    pub video_session: Option<VideoSessionInfo>,
    pub video_readiness: VideoReadinessStatus,
    pub join_urls: Option<VideoJoinUrls>,
}

/// Video session information linked to appointment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSessionInfo {
    pub session_id: Uuid,
    pub cloudflare_session_id: String,
    pub status: VideoSessionStatus,
    pub created_at: DateTime<Utc>,
    pub scheduled_start_time: DateTime<Utc>,
    pub actual_start_time: Option<DateTime<Utc>>,
    pub actual_end_time: Option<DateTime<Utc>>,
    pub participant_count: i32,
    pub session_duration_minutes: Option<i32>,
    pub connection_quality: Option<String>,
}

/// Video session status tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoSessionStatus {
    Pending,      // Video session not yet created
    Created,      // Session created in Cloudflare but not active
    Ready,        // Participants can join (15-30 min before appointment)
    Active,       // Session in progress
    Ended,        // Session completed successfully
    Failed,       // Session failed due to technical issues
    Cancelled,    // Session cancelled before starting
}

/// Video readiness status for appointments
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoReadinessStatus {
    NotRequired,     // Appointment doesn't require video
    Pending,         // Video required but not yet ready
    Ready,           // Video session ready, participants can join
    TestRequired,    // Participant needs to test their connection
    TechnicalIssue,  // Technical problems preventing video access
    Degraded,        // Video available but with quality issues
}

/// Video join URLs and access information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoJoinUrls {
    pub patient_join_url: String,
    pub doctor_join_url: String,
    pub session_id: String,
    pub access_expires_at: DateTime<Utc>,
    pub pre_session_test_url: Option<String>,
    pub session_instructions: SessionInstructions,
}

/// Session instructions for optimal user experience
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInstructions {
    pub patient_instructions: Vec<String>,
    pub doctor_instructions: Vec<String>,
    pub technical_requirements: TechnicalRequirements,
    pub troubleshooting_url: String,
    pub support_contact: String,
}

/// Technical requirements for video sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalRequirements {
    pub minimum_bandwidth_mbps: f32,
    pub supported_browsers: Vec<String>,
    pub camera_required: bool,
    pub microphone_required: bool,
    pub screen_sharing_enabled: bool,
    pub mobile_app_available: bool,
}

impl Default for TechnicalRequirements {
    fn default() -> Self {
        Self {
            minimum_bandwidth_mbps: 1.5,
            supported_browsers: vec![
                "Chrome 90+".to_string(),
                "Firefox 88+".to_string(),
                "Safari 14+".to_string(),
                "Edge 90+".to_string(),
            ],
            camera_required: true,
            microphone_required: true,
            screen_sharing_enabled: true,
            mobile_app_available: true,
        }
    }
}

/// Video session lifecycle configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSessionConfig {
    pub session_ready_minutes_before: i32,  // When to make session available (default: 30)
    pub session_auto_start_minutes_before: i32,  // When to auto-transition to Ready (default: 15)
    pub session_timeout_minutes_after: i32,  // When to timeout if no one joins (default: 30)
    pub max_session_duration_minutes: i32,   // Maximum session length (default: 120)
    pub enable_session_recording: bool,       // Whether to record sessions
    pub enable_waiting_room: bool,           // Whether to use waiting room feature
    pub enable_screen_sharing: bool,         // Whether to allow screen sharing
    pub enable_chat: bool,                   // Whether to enable in-session chat
    pub quality_monitoring_enabled: bool,   // Whether to monitor connection quality
}

impl Default for VideoSessionConfig {
    fn default() -> Self {
        Self {
            session_ready_minutes_before: 30,
            session_auto_start_minutes_before: 15,
            session_timeout_minutes_after: 30,
            max_session_duration_minutes: 120,
            enable_session_recording: false,  // Privacy-first default
            enable_waiting_room: true,
            enable_screen_sharing: true,
            enable_chat: true,
            quality_monitoring_enabled: true,
        }
    }
}

/// Appointment status transition request with video session management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppointmentStatusUpdateRequest {
    pub appointment_id: Uuid,
    pub new_status: AppointmentStatus,
    pub reason: Option<String>,
    pub updated_by: Uuid,  // User ID of who made the change
    pub video_session_action: VideoSessionAction,
    pub notify_participants: bool,
}

/// Actions to take on video session during status transitions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoSessionAction {
    NoAction,        // Don't modify video session
    Create,          // Create new video session
    Activate,        // Make session ready for participants
    Start,           // Start the session (transition to Active)
    End,             // End the session gracefully
    Cancel,          // Cancel and cleanup session
    Recreate,        // Cancel existing and create new session
}

/// Video session lifecycle events for audit and monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSessionLifecycleEvent {
    pub event_id: Uuid,
    pub appointment_id: Uuid,
    pub session_id: Uuid,
    pub event_type: VideoSessionEventType,
    pub event_timestamp: DateTime<Utc>,
    pub triggered_by: VideoSessionTrigger,
    pub event_data: Option<serde_json::Value>,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Types of video session lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoSessionEventType {
    SessionCreated,
    SessionReady,
    ParticipantJoined,
    ParticipantLeft,
    SessionStarted,
    SessionEnded,
    SessionCancelled,
    SessionFailed,
    QualityDegraded,
    QualityRestored,
    RecordingStarted,
    RecordingStopped,
    ScreenShareStarted,
    ScreenShareStopped,
}

/// What triggered a video session event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoSessionTrigger {
    AppointmentStatusChange,
    ScheduledTime,
    UserAction,
    SystemAutomatic,
    TechnicalIssue,
    AdminIntervention,
}

impl VideoSessionStatus {
    /// Check if the session is in a state where participants can join
    pub fn can_join(&self) -> bool {
        matches!(self, VideoSessionStatus::Ready | VideoSessionStatus::Active)
    }

    /// Check if the session is active or ended
    pub fn is_concluded(&self) -> bool {
        matches!(self, VideoSessionStatus::Ended | VideoSessionStatus::Failed | VideoSessionStatus::Cancelled)
    }
}

impl AppointmentStatus {
    /// Determine if this status should trigger video session creation
    pub fn should_create_video_session(&self) -> bool {
        matches!(self, AppointmentStatus::Confirmed)
    }

    /// Determine if this status should make video session ready for joining
    pub fn should_activate_video_session(&self) -> bool {
        matches!(self, AppointmentStatus::Ready)
    }

    /// Determine if this status should start the video session
    pub fn should_start_video_session(&self) -> bool {
        matches!(self, AppointmentStatus::InProgress)
    }

    /// Determine if this status should end the video session
    pub fn should_end_video_session(&self) -> bool {
        matches!(self, AppointmentStatus::Completed | AppointmentStatus::NoShow | AppointmentStatus::Cancelled)
    }

    /// Get the optimal video session action for this status transition
    pub fn get_video_session_action(&self, previous_status: &AppointmentStatus) -> VideoSessionAction {
        match (previous_status, self) {
            (_, AppointmentStatus::Confirmed) => VideoSessionAction::Create,
            (_, AppointmentStatus::Ready) => VideoSessionAction::Activate,
            (_, AppointmentStatus::InProgress) => VideoSessionAction::Start,
            (_, AppointmentStatus::Completed) => VideoSessionAction::End,
            (_, AppointmentStatus::Cancelled) => VideoSessionAction::Cancel,
            (_, AppointmentStatus::NoShow) => VideoSessionAction::End,
            (_, AppointmentStatus::Rescheduled) => VideoSessionAction::Recreate,
            _ => VideoSessionAction::NoAction,
        }
    }
}