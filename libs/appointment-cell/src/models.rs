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
    pub scheduled_start_time: DateTime<Utc>,
    pub scheduled_end_time: DateTime<Utc>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AppointmentStatus {
    Pending,
    Confirmed,
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
            AppointmentStatus::InProgress => write!(f, "in_progress"),
            AppointmentStatus::Completed => write!(f, "completed"),
            AppointmentStatus::Cancelled => write!(f, "cancelled"),
            AppointmentStatus::NoShow => write!(f, "no_show"),
            AppointmentStatus::Rescheduled => write!(f, "rescheduled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AppointmentType {
    GeneralConsultation,
    FollowUp,
    Prescription,
    MedicalCertificate,
    Urgent,
    MentalHealth,
    WomensHealth,
}

impl fmt::Display for AppointmentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppointmentType::GeneralConsultation => write!(f, "general_consultation"),
            AppointmentType::FollowUp => write!(f, "follow_up"),
            AppointmentType::Prescription => write!(f, "prescription"),
            AppointmentType::MedicalCertificate => write!(f, "medical_certificate"),
            AppointmentType::Urgent => write!(f, "urgent"),
            AppointmentType::MentalHealth => write!(f, "mental_health"),
            AppointmentType::WomensHealth => write!(f, "womens_health"),
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
    pub doctor_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub match_score: f32,
    pub has_patient_history: bool,
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
    pub patient_name: String,
    pub doctor_name: String,
    pub appointment_date: DateTime<Utc>,
    pub status: AppointmentStatus,
    pub appointment_type: AppointmentType,
    pub duration_minutes: i32,
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