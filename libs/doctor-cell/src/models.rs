use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc, NaiveTime, NaiveDate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doctor {
    pub id: Uuid,
    pub full_name: String,
    pub email: String,
    pub specialty: String,
    pub bio: Option<String>,
    pub profile_image_url: Option<String>,
    pub license_number: Option<String>,
    pub years_experience: Option<i32>,
    pub consultation_fee: Option<f64>,
    pub timezone: String,
    pub is_verified: bool,
    pub is_available: bool,
    pub rating: f32,
    pub total_consultations: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorSpecialty {
    pub id: Uuid,
    pub doctor_id: Uuid,
    pub specialty_name: String,
    pub sub_specialty: Option<String>,
    pub certification_number: Option<String>,
    pub certification_date: Option<NaiveDate>,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorAvailability {
    pub id: Uuid,
    pub doctor_id: Uuid,
    pub day_of_week: i32, // 0 = Sunday, 1 = Monday, etc.
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub duration_minutes: i32,
    pub timezone: String,
    pub appointment_type: String,
    pub buffer_minutes: i32,
    pub max_concurrent_appointments: i32,
    pub price_per_session: Option<f64>,
    pub is_recurring: bool,
    pub specific_date: Option<NaiveDate>,
    pub is_available: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorAvailabilityOverride {
    pub id: Uuid,
    pub doctor_id: Uuid,
    pub override_date: NaiveDate,
    pub is_available: bool,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableSlot {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_minutes: i32,
    pub appointment_type: String,
    pub price: Option<f64>,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorSearchFilters {
    pub specialty: Option<String>,
    pub sub_specialty: Option<String>,
    pub min_experience: Option<i32>,
    pub max_consultation_fee: Option<f64>,
    pub min_rating: Option<f32>,
    pub available_date: Option<NaiveDate>,
    pub available_time_start: Option<NaiveTime>,
    pub available_time_end: Option<NaiveTime>,
    pub timezone: Option<String>,
    pub appointment_type: Option<String>,
    pub is_verified_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDoctorRequest {
    pub full_name: String,
    pub email: String,
    pub specialty: String,
    pub bio: Option<String>,
    pub license_number: Option<String>,
    pub years_experience: Option<i32>,
    pub consultation_fee: Option<f64>,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDoctorRequest {
    pub full_name: Option<String>,
    pub bio: Option<String>,
    pub specialty: Option<String>,
    pub years_experience: Option<i32>,
    pub consultation_fee: Option<f64>,
    pub timezone: Option<String>,
    pub is_available: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAvailabilityRequest {
    pub day_of_week: i32,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub duration_minutes: i32,
    pub timezone: String,
    pub appointment_type: String,
    pub buffer_minutes: Option<i32>,
    pub max_concurrent_appointments: Option<i32>,
    pub price_per_session: Option<f64>,
    pub is_recurring: Option<bool>,
    pub specific_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAvailabilityRequest {
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub duration_minutes: Option<i32>,
    pub timezone: Option<String>,
    pub buffer_minutes: Option<i32>,
    pub max_concurrent_appointments: Option<i32>,
    pub price_per_session: Option<f64>,
    pub is_available: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpecialtyRequest {
    pub specialty_name: String,
    pub sub_specialty: Option<String>,
    pub certification_number: Option<String>,
    pub certification_date: Option<NaiveDate>,
    pub is_primary: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAvailabilityOverrideRequest {
    pub override_date: NaiveDate,
    pub is_available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityQueryRequest {
    pub date: NaiveDate,
    pub timezone: Option<String>,
    pub appointment_type: Option<String>,
    pub duration_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorMatchingRequest {
    pub patient_id: Uuid,
    pub preferred_date: Option<NaiveDate>,
    pub preferred_time_start: Option<NaiveTime>,
    pub preferred_time_end: Option<NaiveTime>,
    pub specialty_required: Option<String>,
    pub max_consultation_fee: Option<f64>,
    pub appointment_type: String,
    pub duration_minutes: i32,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorMatch {
    pub doctor: Doctor,
    pub available_slots: Vec<AvailableSlot>,
    pub match_score: f32, // 0.0 to 1.0, higher is better match
    pub match_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorStats {
    pub total_appointments: i32,
    pub completed_appointments: i32,
    pub avg_session_duration_minutes: i32,
    pub avg_rating: f32,
    pub total_reviews: i32,
    pub specialties: Vec<DoctorSpecialty>,
    pub next_available_slot: Option<AvailableSlot>,
}

// DTO for available time slots response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorAvailabilityResponse {
    pub doctor_id: Uuid,
    pub doctor_name: String,
    pub specialty: String,
    pub available_slots: Vec<AvailableSlot>,
    pub timezone: String,
    pub consultation_fee: Option<f64>,
}

// Request/Response DTOs for profile image upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorImageUpload {
    pub file_data: String, // Base64 encoded image
}

// Error types specific to doctor operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DoctorError {
    NotFound,
    NotAvailable,
    InvalidTimezone,
    InvalidTimeSlot,
    UnauthorizedAccess,
    ValidationError(String),
}

impl std::fmt::Display for DoctorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DoctorError::NotFound => write!(f, "Doctor not found"),
            DoctorError::NotAvailable => write!(f, "Doctor is not available"),
            DoctorError::InvalidTimezone => write!(f, "Invalid timezone specified"),
            DoctorError::InvalidTimeSlot => write!(f, "Invalid time slot"),
            DoctorError::UnauthorizedAccess => write!(f, "Unauthorized access to doctor data"),
            DoctorError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for DoctorError {}