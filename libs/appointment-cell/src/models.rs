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
    pub consultation_fee: f64,  // Always set - no optional pricing
    pub timezone: String,
    pub scheduled_start_time: DateTime<Utc>,
    pub scheduled_end_time: DateTime<Utc>,
    pub actual_start_time: Option<DateTime<Utc>>,
    pub actual_end_time: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub patient_notes: Option<String>,  // Patient's initial notes
    pub doctor_notes: Option<String>,  // Doctor's notes during/after consultation
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
    Pending,       // Just booked, awaiting confirmation
    Confirmed,     // Doctor confirmed, payment processed
    InProgress,    // Consultation started
    Completed,     // Consultation finished
    Cancelled,     // Cancelled by patient or doctor
    NoShow,        // Patient didn't show up
    Rescheduled,   // Moved to different time
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq,Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AppointmentType {
    GeneralConsultation,  // Standard consultation
    FollowUp,            // Follow-up appointment
    Prescription,        // Prescription renewal
    MedicalCertificate,  // Sick note/medical certificate
    Urgent,              // Urgent consultation
    MentalHealth,        // Mental health consultation
    WomensHealth,        // Women's health specific
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
// PRICING MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppointmentPricing {
    pub appointment_type: AppointmentType,
    pub base_price: f64,
    pub promotional_price: Option<f64>,
    pub includes_prescription: bool,
    pub includes_medical_certificate: bool,
    pub includes_report: bool,
}

impl AppointmentPricing {
    pub fn get_standard_pricing() -> Vec<Self> {
        vec![
            Self {
                appointment_type: AppointmentType::GeneralConsultation,
                base_price: 39.0,
                promotional_price: Some(29.0), // Initial promotional pricing
                includes_prescription: true,
                includes_medical_certificate: true,
                includes_report: true,
            },
            Self {
                appointment_type: AppointmentType::FollowUp,
                base_price: 29.0,
                promotional_price: None,
                includes_prescription: true,
                includes_medical_certificate: false,
                includes_report: true,
            },
            Self {
                appointment_type: AppointmentType::Prescription,
                base_price: 19.0,
                promotional_price: None,
                includes_prescription: true,
                includes_medical_certificate: false,
                includes_report: false,
            },
            Self {
                appointment_type: AppointmentType::MedicalCertificate,
                base_price: 25.0,
                promotional_price: None,
                includes_prescription: false,
                includes_medical_certificate: true,
                includes_report: false,
            },
            Self {
                appointment_type: AppointmentType::Urgent,
                base_price: 59.0,
                promotional_price: None,
                includes_prescription: true,
                includes_medical_certificate: true,
                includes_report: true,
            },
            Self {
                appointment_type: AppointmentType::MentalHealth,
                base_price: 49.0,
                promotional_price: None,
                includes_prescription: true,
                includes_medical_certificate: true,
                includes_report: true,
            },
            Self {
                appointment_type: AppointmentType::WomensHealth,
                base_price: 39.0,
                promotional_price: Some(29.0),
                includes_prescription: true,
                includes_medical_certificate: true,
                includes_report: true,
            },
        ]
    }

    pub fn get_effective_price(&self) -> f64 {
        self.promotional_price.unwrap_or(self.base_price)
    }
}

// ==============================================================================
// REQUEST/RESPONSE MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookAppointmentRequest {
    pub patient_id: Uuid,
    pub doctor_id: Uuid,
    pub appointment_date: DateTime<Utc>,
    pub appointment_type: AppointmentType,
    pub duration_minutes: i32,
    pub timezone: String,
    pub patient_notes: Option<String>,
    pub preferred_language: Option<String>,
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
    pub price: f64,
}

// ==============================================================================
// APPOINTMENT SUMMARY MODELS
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
    pub consultation_fee: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppointmentStats {
    pub total_appointments: i32,
    pub completed_appointments: i32,
    pub cancelled_appointments: i32,
    pub no_show_appointments: i32,
    pub total_revenue: f64,
    pub average_consultation_duration: i32,
    pub appointment_type_breakdown: Vec<(AppointmentType, i32)>,
}

// ==============================================================================
// ERROR TYPES
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AppointmentError {
    #[error("Appointment not found")]
    NotFound,
    
    #[error("Appointment slot not available")]
    SlotNotAvailable,
    
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
    
    #[error("Payment required before confirmation")]
    PaymentRequired,
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("External service error: {0}")]
    ExternalServiceError(String),
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
}

impl Default for AppointmentValidationRules {
    fn default() -> Self {
        Self {
            min_advance_booking_hours: 2,    // Must book at least 2 hours ahead
            max_advance_booking_days: 90,    // Can book up to 90 days ahead
            allowed_cancellation_hours: 24,  // Can cancel up to 24 hours before
            allowed_reschedule_hours: 48,    // Can reschedule up to 48 hours before
            max_appointments_per_day: 3,     // Max 3 appointments per patient per day
            min_appointment_duration: 15,    // Minimum 15 minutes
            max_appointment_duration: 120,   // Maximum 2 hours
        }
    }
}