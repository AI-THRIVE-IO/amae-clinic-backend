use serde::{Deserialize, Serialize, Deserializer, de};
use uuid::Uuid;
use chrono::{DateTime, Utc, NaiveTime, NaiveDate, Datelike, Timelike};

/// Production-grade deserializer for day_of_week that accepts both string day names and integers
/// Supports: "monday"/"Monday"/1, "tuesday"/"Tuesday"/2, etc., or direct integers 0-6
fn deserialize_day_of_week<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    struct DayOfWeekVisitor;

    impl<'de> de::Visitor<'de> for DayOfWeekVisitor {
        type Value = i32;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a day of week as integer (0-6) or string (monday-sunday)")
        }

        fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if (0..=6).contains(&value) {
                Ok(value)
            } else {
                Err(E::custom(format!("day_of_week must be between 0-6, got {}", value)))
            }
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if (0..=6).contains(&value) {
                Ok(value as i32)
            } else {
                Err(E::custom(format!("day_of_week must be between 0-6, got {}", value)))
            }
        }

        fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value <= 6 {
                Ok(value as i32)
            } else {
                Err(E::custom(format!("day_of_week must be between 0-6, got {}", value)))
            }
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value <= 6 {
                Ok(value as i32)
            } else {
                Err(E::custom(format!("day_of_week must be between 0-6, got {}", value)))
            }
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            match value.to_lowercase().as_str() {
                "sunday" | "sun" => Ok(0),
                "monday" | "mon" => Ok(1),
                "tuesday" | "tue" | "tues" => Ok(2),
                "wednesday" | "wed" => Ok(3),
                "thursday" | "thu" | "thur" | "thurs" => Ok(4),
                "friday" | "fri" => Ok(5),
                "saturday" | "sat" => Ok(6),
                _ => Err(E::custom(format!(
                    "unknown day name '{}', expected sunday-saturday or 0-6",
                    value
                ))),
            }
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(DayOfWeekVisitor)
}

/// Flexible deserializer for time fields that accepts time strings like "09:00:00" and converts to DateTime<Utc>
fn deserialize_optional_time_from_string<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    use chrono::{NaiveTime, Utc, TimeZone};
    use serde::de::Error;
    
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(time_str) => {
            // Parse time string like "09:00:00" or "9:00"
            let naive_time = NaiveTime::parse_from_str(&time_str, "%H:%M:%S")
                .or_else(|_| NaiveTime::parse_from_str(&time_str, "%H:%M"))
                .or_else(|_| NaiveTime::parse_from_str(&time_str, "%I:%M %p"))
                .map_err(|e| D::Error::custom(format!("Invalid time format '{}': {}", time_str, e)))?;
            
            // Convert to today's UTC DateTime for consistency
            let today = Utc::now().date_naive();
            let datetime_utc = Utc.from_local_datetime(&today.and_time(naive_time))
                .single()
                .ok_or_else(|| D::Error::custom("Invalid datetime conversion"))?;
            
            Ok(Some(datetime_utc))
        }
        None => Ok(None),
    }
}

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
    /// Get the full name by combining first_name and last_name
    pub fn get_full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name).trim().to_string()
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

    /// Calculate effective duration including buffer time
    pub fn effective_slot_duration(&self) -> i32 {
        self.duration_minutes + self.buffer_minutes
    }

    /// Check if this availability supports concurrent appointments
    pub fn supports_concurrent_appointments(&self) -> bool {
        self.max_concurrent_appointments > 1 && self.appointment_type.allows_concurrent()
    }

    /// Get priority score for slot scheduling
    pub fn get_priority_score(&self) -> i32 {
        self.appointment_type.priority_score()
    }

    /// Check if this availability is for a specific date or recurring
    pub fn is_specific_date_availability(&self) -> bool {
        self.specific_date.is_some()
    }

    /// Get the actual appointment duration for this type
    pub fn get_appointment_duration(&self) -> i32 {
        if self.duration_minutes > 0 {
            self.duration_minutes
        } else {
            self.appointment_type.default_duration_minutes()
        }
    }

    /// Check if this availability is active for a specific date
    pub fn is_active_for_date(&self, date: NaiveDate) -> bool {
        if let Some(specific_date) = self.specific_date {
            // If it's a specific date availability, it's only active for that date
            specific_date == date
        } else {
            // If it's recurring, check if it matches the day of week
            self.is_recurring && date.weekday().num_days_from_monday() as i32 + 1 == self.day_of_week % 7
        }
    }

    /// Generate available time slots for a specific date with medical scheduling logic
    pub fn generate_medical_slots(&self, date: NaiveDate, existing_appointments: &[DateTime<Utc>]) -> Vec<AvailableSlot> {
        if !self.is_available || !self.is_active_for_date(date) {
            return vec![];
        }

        let mut slots = Vec::new();
        let appointment_duration = self.get_appointment_duration();
        let effective_duration = self.effective_slot_duration();

        // Generate morning slots
        if let (Some(morning_start), Some(morning_end)) = (self.morning_start_time, self.morning_end_time) {
            let morning_slots = self.generate_slots_for_period(
                morning_start, morning_end, appointment_duration, effective_duration, existing_appointments
            );
            slots.extend(morning_slots);
        }

        // Generate afternoon slots
        if let (Some(afternoon_start), Some(afternoon_end)) = (self.afternoon_start_time, self.afternoon_end_time) {
            let afternoon_slots = self.generate_slots_for_period(
                afternoon_start, afternoon_end, appointment_duration, effective_duration, existing_appointments
            );
            slots.extend(afternoon_slots);
        }

        // Sort slots chronologically and assign priorities
        slots.sort_by(|a, b| a.start_time.cmp(&b.start_time));
        
        // Assign slot priorities based on availability and demand
        self.assign_slot_priorities(&mut slots, existing_appointments);
        
        slots
    }

    fn generate_slots_for_period(
        &self,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        appointment_duration: i32,
        effective_duration: i32,
        existing_appointments: &[DateTime<Utc>],
    ) -> Vec<AvailableSlot> {
        let mut slots = Vec::new();
        let mut current_time = period_start;

        while current_time + chrono::Duration::minutes(appointment_duration as i64) <= period_end {
            let slot_end = current_time + chrono::Duration::minutes(appointment_duration as i64);
            
            // Check for conflicts with existing appointments
            let has_conflict = existing_appointments.iter().any(|&appointment_time| {
                let appointment_end = appointment_time + chrono::Duration::minutes(appointment_duration as i64);
                // Check for overlap considering buffer time
                !(slot_end <= appointment_time || current_time >= appointment_end)
            });

            if !has_conflict {
                slots.push(AvailableSlot {
                    start_time: current_time,
                    end_time: slot_end,
                    duration_minutes: appointment_duration,
                    timezone: "UTC".to_string(), // TODO: Use doctor's timezone
                    appointment_type: self.appointment_type.clone(),
                    buffer_minutes: self.buffer_minutes,
                    is_concurrent_available: self.supports_concurrent_appointments(),
                    max_concurrent_patients: self.max_concurrent_appointments,
                    slot_priority: SlotPriority::Available, // Will be updated by assign_slot_priorities
                });
            }

            current_time += chrono::Duration::minutes(effective_duration as i64);
        }

        slots
    }

    fn assign_slot_priorities(&self, slots: &mut [AvailableSlot], existing_appointments: &[DateTime<Utc>]) {
        let total_slots = slots.len();
        let appointment_density = existing_appointments.len() as f32 / (total_slots.max(1) as f32);

        for (index, slot) in slots.iter_mut().enumerate() {
            slot.slot_priority = match self.appointment_type {
                AppointmentType::EmergencyConsultation => SlotPriority::Emergency,
                _ => {
                    // Assign priority based on time of day and availability density
                    let is_peak_hour = self.is_peak_hour(&slot.start_time);
                    
                    if appointment_density > 0.8 {
                        SlotPriority::Limited
                    } else if is_peak_hour && appointment_density > 0.6 {
                        SlotPriority::Limited
                    } else if index < total_slots / 3 {
                        SlotPriority::Preferred // Early slots are preferred
                    } else {
                        SlotPriority::Available
                    }
                }
            };
        }
    }

    fn is_peak_hour(&self, time: &DateTime<Utc>) -> bool {
        let hour = time.hour();
        // Consider 9-11 AM and 2-4 PM as peak hours for medical appointments
        (hour >= 9 && hour <= 11) || (hour >= 14 && hour <= 16)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorSpecialty {
    pub id: String,
    pub doctor_id: String,
    pub specialty_name: String,
    pub sub_specialty: Option<String>,
    pub certification_number: Option<String>,
    pub certification_date: Option<NaiveDate>,
    pub certification_body: Option<String>,
    pub is_primary: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorAvailability {
    pub id: Uuid,
    pub doctor_id: Uuid,
    #[serde(deserialize_with = "deserialize_day_of_week")]
    pub day_of_week: i32, // 0 = Sunday, 1 = Monday, etc.
    pub duration_minutes: i32,
    pub is_available: bool,
    pub morning_start_time: Option<DateTime<Utc>>,
    pub morning_end_time: Option<DateTime<Utc>>,
    pub afternoon_start_time: Option<DateTime<Utc>>,
    pub afternoon_end_time: Option<DateTime<Utc>>,
    // Enhanced medical scheduling fields
    #[serde(default)]
    pub appointment_type: AppointmentType,
    #[serde(default = "default_buffer_minutes_i32")]
    pub buffer_minutes: i32,
    #[serde(default = "default_max_concurrent_i32")]
    pub max_concurrent_appointments: i32,
    #[serde(default = "default_true_bool")]
    pub is_recurring: bool,
    pub specific_date: Option<NaiveDate>,
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
    pub timezone: String,
    pub appointment_type: AppointmentType,
    pub buffer_minutes: i32,
    pub is_concurrent_available: bool,
    pub max_concurrent_patients: i32,
    pub slot_priority: SlotPriority,
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
    #[serde(alias = "years_of_experience")]
    pub years_experience: Option<i32>,
    pub timezone: Option<String>,
    pub max_daily_appointments: Option<i32>,
    pub available_days: Option<Vec<i32>>,
    pub date_of_birth: NaiveDate,
    // Additional curl compatibility fields
    pub user_id: Option<String>,
    pub phone: Option<String>,
    pub education: Option<String>,    
    pub certifications: Option<Vec<String>>,
    pub languages: Option<Vec<String>>,
    pub consultation_fee: Option<f64>,
    pub emergency_fee: Option<f64>,
    pub is_available: Option<bool>,
    pub accepts_insurance: Option<bool>,
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
    #[serde(deserialize_with = "deserialize_day_of_week")]
    pub day_of_week: i32,
    #[serde(alias = "slot_duration_minutes")]
    pub duration_minutes: i32,
    #[serde(deserialize_with = "deserialize_optional_time_from_string")]
    pub morning_start_time: Option<DateTime<Utc>>,
    #[serde(deserialize_with = "deserialize_optional_time_from_string")]
    pub morning_end_time: Option<DateTime<Utc>>,
    #[serde(deserialize_with = "deserialize_optional_time_from_string")]
    pub afternoon_start_time: Option<DateTime<Utc>>,
    #[serde(deserialize_with = "deserialize_optional_time_from_string")]
    pub afternoon_end_time: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_available: Option<bool>,
    #[serde(default)]
    pub appointment_type: AppointmentType,
    #[serde(default = "default_buffer_minutes")]
    pub buffer_minutes: Option<i32>,
    #[serde(alias = "max_concurrent_patients", default = "default_max_concurrent")]
    pub max_concurrent_appointments: Option<i32>,
    #[serde(default = "default_true")]
    pub is_recurring: Option<bool>,
    pub specific_date: Option<NaiveDate>,
    // Additional fields for curl compatibility
    pub timezone: Option<String>,
    pub appointment_types: Option<Vec<String>>,
    #[serde(alias = "is_available")]
    pub is_active: Option<bool>,
}

// Helper functions for defaults
fn default_true() -> Option<bool> { Some(true) }
fn default_true_bool() -> bool { true }
fn default_buffer_minutes() -> Option<i32> { Some(10) }
fn default_buffer_minutes_i32() -> i32 { 10 }
fn default_max_concurrent() -> Option<i32> { Some(1) }
fn default_max_concurrent_i32() -> i32 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAvailabilityRequest {
    pub duration_minutes: Option<i32>,
    pub morning_start_time: Option<DateTime<Utc>>,
    pub morning_end_time: Option<DateTime<Utc>>,
    pub afternoon_start_time: Option<DateTime<Utc>>,
    pub afternoon_end_time: Option<DateTime<Utc>>,
    pub is_available: Option<bool>,
    pub appointment_type: Option<AppointmentType>,
    pub buffer_minutes: Option<i32>,
    pub max_concurrent_appointments: Option<i32>,
    pub is_recurring: Option<bool>,
    pub specific_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpecialtyRequest {
    pub specialty_name: String,
    pub sub_specialty: Option<String>,
    pub certification_number: Option<String>,
    pub certification_date: Option<NaiveDate>,
    pub certification_body: Option<String>,
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
    #[serde(alias = "image_data")]
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

// Enhanced Medical Appointment Types with Comprehensive Alias Support
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AppointmentType {
    // 45-60 minutes, new patients, higher priority
    #[serde(alias = "initial_consultation", alias = "initial", alias = "new_patient")]
    InitialConsultation,
    
    // 15-30 minutes, general consultations  
    #[serde(alias = "general_consultation", alias = "consultation", alias = "general")]
    GeneralConsultation,
    
    // 15-30 minutes, existing patients
    #[serde(alias = "follow_up_consultation", alias = "follow_up", alias = "followup")]
    FollowUpConsultation,
    
    // 15-45 minutes, urgent care, highest priority
    #[serde(alias = "emergency_consultation", alias = "emergency", alias = "urgent")]
    EmergencyConsultation,
    
    // 10-15 minutes, medication management
    #[serde(alias = "prescription_renewal", alias = "prescription", alias = "medication_renewal")]
    PrescriptionRenewal,
    
    // 30-60 minutes, specialist referrals
    #[serde(alias = "specialty_consultation", alias = "specialty", alias = "specialist")]
    SpecialtyConsultation,
    
    // 60-90 minutes, multiple patients
    #[serde(alias = "group_session", alias = "group", alias = "workshop")]
    GroupSession,
    
    // 10-15 minutes, remote monitoring
    #[serde(alias = "telehealth_checkin", alias = "telehealth", alias = "remote_checkin", alias = "virtual")]
    TelehealthCheckIn,
}

impl AppointmentType {
    pub fn default_duration_minutes(&self) -> i32 {
        match self {
            AppointmentType::InitialConsultation => 45,
            AppointmentType::GeneralConsultation => 30,
            AppointmentType::FollowUpConsultation => 20,
            AppointmentType::EmergencyConsultation => 30,
            AppointmentType::PrescriptionRenewal => 10,
            AppointmentType::SpecialtyConsultation => 45,
            AppointmentType::GroupSession => 60,
            AppointmentType::TelehealthCheckIn => 15,
        }
    }

    pub fn default_buffer_minutes(&self) -> i32 {
        match self {
            AppointmentType::InitialConsultation => 15,  // More time for documentation
            AppointmentType::GeneralConsultation => 10,
            AppointmentType::FollowUpConsultation => 10,
            AppointmentType::EmergencyConsultation => 5,  // Quick turnaround needed
            AppointmentType::PrescriptionRenewal => 5,
            AppointmentType::SpecialtyConsultation => 15,
            AppointmentType::GroupSession => 20,  // Cleanup time
            AppointmentType::TelehealthCheckIn => 5,
        }
    }

    pub fn allows_concurrent(&self) -> bool {
        matches!(self, AppointmentType::GroupSession | AppointmentType::TelehealthCheckIn)
    }

    pub fn priority_score(&self) -> i32 {
        match self {
            AppointmentType::EmergencyConsultation => 100,
            AppointmentType::InitialConsultation => 80,
            AppointmentType::SpecialtyConsultation => 70,
            AppointmentType::GeneralConsultation => 65,
            AppointmentType::FollowUpConsultation => 60,
            AppointmentType::TelehealthCheckIn => 40,
            AppointmentType::PrescriptionRenewal => 30,
            AppointmentType::GroupSession => 20,
        }
    }
}

impl Default for AppointmentType {
    fn default() -> Self {
        AppointmentType::GeneralConsultation
    }
}

// Slot Priority for Frontend Optimization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SlotPriority {
    Emergency,      // Red - immediate availability
    Preferred,      // Green - optimal time slots
    Available,      // Blue - standard availability
    Limited,        // Yellow - few slots remaining
    WaitList,       // Gray - overbooked, waitlist only
}

// Enhanced Availability Response for Frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedDoctorAvailabilityResponse {
    pub doctor_id: Uuid,
    pub doctor_first_name: String,
    pub doctor_last_name: String,
    pub specialty: String,
    pub sub_specialty: Option<String>,
    pub rating: f32,
    pub total_consultations: i32,
    pub has_previous_consultation: bool,
    pub morning_slots: Vec<AvailableSlot>,
    pub afternoon_slots: Vec<AvailableSlot>,
    pub timezone: String,
    pub next_available_emergency: Option<DateTime<Utc>>,
    pub patient_continuity_score: f32,
    pub estimated_wait_time_minutes: Option<i32>,
}

// Medical Scheduling Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalSchedulingConfig {
    pub clinic_timezone: String,
    pub max_advance_booking_days: i32,
    pub min_advance_booking_hours: i32,
    pub emergency_slot_percentage: f32,
    pub group_session_max_patients: i32,
    pub default_buffer_minutes: i32,
    pub doctor_break_duration_minutes: i32,
    pub lunch_break_start: NaiveTime,
    pub lunch_break_end: NaiveTime,
}

impl Default for MedicalSchedulingConfig {
    fn default() -> Self {
        Self {
            clinic_timezone: "UTC".to_string(),
            max_advance_booking_days: 90,
            min_advance_booking_hours: 2,
            emergency_slot_percentage: 0.1,
            group_session_max_patients: 6,
            default_buffer_minutes: 10,
            doctor_break_duration_minutes: 15,
            lunch_break_start: NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            lunch_break_end: NaiveTime::from_hms_opt(13, 0, 0).unwrap(),
        }
    }
}