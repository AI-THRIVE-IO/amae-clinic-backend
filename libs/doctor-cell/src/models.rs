use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc, NaiveTime, NaiveDate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doctor {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub specialty: String,
    pub sub_specialty: Option<String>,
    pub bio: Option<String>,
    pub profile_image_url: Option<String>,
    pub license_number: String,
    pub years_experience: Option<i32>,
    pub timezone: Option<String>,
    pub is_verified: bool,
    pub is_available: bool,
    pub rating: f32,
    pub total_consultations: i32,
    pub max_daily_appointments: Option<i32>,
    pub available_days: Vec<i32>,
    pub date_of_birth: NaiveDate,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Doctor {
    pub fn full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }
}

impl DoctorAvailability {
    pub fn has_morning_availability(&self) -> bool {
        self.morning_start_time.is_some() && self.morning_end_time.is_some()
    }
    
    pub fn has_afternoon_availability(&self) -> bool {
        self.afternoon_start_time.is_some() && self.afternoon_end_time.is_some()
    }
    
    pub fn get_time_slots(&self) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
        let mut slots = Vec::new();
        
        if let (Some(morning_start), Some(morning_end)) = (self.morning_start_time, self.morning_end_time) {
            slots.push((morning_start, morning_end));
        }
        
        if let (Some(afternoon_start), Some(afternoon_end)) = (self.afternoon_start_time, self.afternoon_end_time) {
            slots.push((afternoon_start, afternoon_end));
        }
        
        slots
    }
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
    pub duration_minutes: i32,
    pub is_available: bool,
    pub morning_start_time: Option<DateTime<Utc>>,
    pub morning_end_time: Option<DateTime<Utc>>,
    pub afternoon_start_time: Option<DateTime<Utc>>,
    pub afternoon_end_time: Option<DateTime<Utc>>,
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
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorSearchFilters {
    pub specialty: Option<String>,
    pub sub_specialty: Option<String>,
    pub min_experience: Option<i32>,
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
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub specialty: String,
    pub sub_specialty: Option<String>,
    pub bio: Option<String>,
    pub license_number: String,
    pub years_experience: Option<i32>,
    pub timezone: Option<String>,
    pub max_daily_appointments: Option<i32>,
    pub available_days: Option<Vec<i32>>,
    pub date_of_birth: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDoctorRequest {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub bio: Option<String>,
    pub specialty: Option<String>,
    pub sub_specialty: Option<String>,
    pub years_experience: Option<i32>,
    pub timezone: Option<String>,
    pub is_available: Option<bool>,
    pub max_daily_appointments: Option<i32>,
    pub available_days: Option<Vec<i32>>,
    pub date_of_birth: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAvailabilityRequest {
    pub day_of_week: i32,
    pub duration_minutes: i32,
    pub morning_start_time: Option<DateTime<Utc>>,
    pub morning_end_time: Option<DateTime<Utc>>,
    pub afternoon_start_time: Option<DateTime<Utc>>,
    pub afternoon_end_time: Option<DateTime<Utc>>,
    pub is_available: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAvailabilityRequest {
    pub duration_minutes: Option<i32>,
    pub morning_start_time: Option<DateTime<Utc>>,
    pub morning_end_time: Option<DateTime<Utc>>,
    pub afternoon_start_time: Option<DateTime<Utc>>,
    pub afternoon_end_time: Option<DateTime<Utc>>,
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
    pub duration_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorMatchingRequest {
    pub patient_id: Uuid,
    pub preferred_date: Option<NaiveDate>,
    pub preferred_time_start: Option<NaiveTime>,
    pub preferred_time_end: Option<NaiveTime>,
    pub specialty_required: Option<String>,
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
    pub doctor_first_name: String,
    pub doctor_last_name: String,
    pub specialty: String,
    pub available_slots: Vec<AvailableSlot>,
    pub timezone: String,
}

impl DoctorAvailabilityResponse {
    pub fn doctor_full_name(&self) -> String {
        format!("{} {}", self.doctor_first_name, self.doctor_last_name)
    }
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
            DoctorError::NotAvailable => write!(f, "No doctors with the required specialty are available at this time"),
            DoctorError::InvalidTimezone => write!(f, "Invalid timezone specified"),
            DoctorError::InvalidTimeSlot => write!(f, "Invalid time slot"),
            DoctorError::UnauthorizedAccess => write!(f, "Unauthorized access to doctor data"),
            DoctorError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}


impl std::error::Error for DoctorError {}