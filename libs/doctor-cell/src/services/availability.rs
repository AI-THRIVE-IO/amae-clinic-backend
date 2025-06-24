use anyhow::{Result, anyhow};
use chrono::{NaiveDate, DateTime, Utc, Datelike, Weekday, Duration};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, warn, error};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{
    DoctorAvailability, DoctorAvailabilityOverride, AvailableSlot,
    CreateAvailabilityRequest, UpdateAvailabilityRequest,
    CreateAvailabilityOverrideRequest, AvailabilityQueryRequest,
    DoctorAvailabilityResponse, DoctorError, AppointmentType, SlotPriority,
};
use crate::services::doctor::DoctorService;

pub struct AvailabilityService {
    supabase: SupabaseClient,
    config: AppConfig,
}

impl AvailabilityService {
pub fn new(config: &AppConfig) -> Self {
    Self {
        supabase: SupabaseClient::new(config),
        config: config.clone(),
    }
}

    /// Create availability schedule for a doctor
    pub async fn create_availability(
        &self,
        doctor_id: &str,
        request: CreateAvailabilityRequest,
        auth_token: &str,
    ) -> Result<DoctorAvailability> {
        debug!("Creating availability for doctor: {}", doctor_id);

        // Validate day of week (0-6)
        if request.day_of_week < 0 || request.day_of_week > 6 {
            return Err(anyhow!("Day of week must be between 0 (Sunday) and 6 (Saturday)"));
        }

        // Validate that at least one time slot is provided
        let has_morning = request.morning_start_time.is_some() && request.morning_end_time.is_some();
        let has_afternoon = request.afternoon_start_time.is_some() && request.afternoon_end_time.is_some();
        
        if !has_morning && !has_afternoon {
            return Err(anyhow!("At least one time slot (morning or afternoon) must be provided"));
        }

        // Validate morning time range if provided
        if let (Some(morning_start), Some(morning_end)) = (request.morning_start_time, request.morning_end_time) {
            if morning_start >= morning_end {
                return Err(anyhow!("Morning start time must be before morning end time"));
            }
        }

        // Validate afternoon time range if provided
        if let (Some(afternoon_start), Some(afternoon_end)) = (request.afternoon_start_time, request.afternoon_end_time) {
            if afternoon_start >= afternoon_end {
                return Err(anyhow!("Afternoon start time must be before afternoon end time"));
            }
        }

        let availability_data = json!({
            "doctor_id": doctor_id,
            "day_of_week": request.day_of_week,
            "duration_minutes": request.duration_minutes,
            "morning_start_time": request.morning_start_time.map(|t| t.to_rfc3339()),
            "morning_end_time": request.morning_end_time.map(|t| t.to_rfc3339()),
            "afternoon_start_time": request.afternoon_start_time.map(|t| t.to_rfc3339()),
            "afternoon_end_time": request.afternoon_end_time.map(|t| t.to_rfc3339()),
            "is_available": request.is_available.unwrap_or(true),
            // Enhanced medical scheduling fields
            "appointment_type": request.appointment_type,
            "buffer_minutes": request.buffer_minutes.unwrap_or_else(|| request.appointment_type.default_buffer_minutes()),
            "max_concurrent_appointments": request.max_concurrent_appointments.unwrap_or(1),
            "is_recurring": request.is_recurring.unwrap_or(true),
            "specific_date": request.specific_date,
            "created_at": Utc::now().to_rfc3339(),
            "updated_at": Utc::now().to_rfc3339()
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            "/rest/v1/appointment_availabilities",
            Some(auth_token),
            Some(availability_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to create availability"));
        }

        let availability: DoctorAvailability = serde_json::from_value(result[0].clone())?;
        debug!("Availability created with ID: {}", availability.id);

        Ok(availability)
    }

    /// Update availability schedule
    pub async fn update_availability(
        &self,
        availability_id: &str,
        request: UpdateAvailabilityRequest,
        auth_token: &str,
    ) -> Result<DoctorAvailability> {
        debug!("Updating availability: {}", availability_id);

        // Validate morning time range if provided
        if let (Some(morning_start), Some(morning_end)) = (request.morning_start_time, request.morning_end_time) {
            if morning_start >= morning_end {
                return Err(anyhow!("Morning start time must be before morning end time"));
            }
        }

        // Validate afternoon time range if provided
        if let (Some(afternoon_start), Some(afternoon_end)) = (request.afternoon_start_time, request.afternoon_end_time) {
            if afternoon_start >= afternoon_end {
                return Err(anyhow!("Afternoon start time must be before afternoon end time"));
            }
        }

        // Build update object
        let mut update_data = serde_json::Map::new();
        
        if let Some(morning_start_time) = request.morning_start_time {
            update_data.insert("morning_start_time".to_string(), json!(morning_start_time.to_rfc3339()));
        }
        if let Some(morning_end_time) = request.morning_end_time {
            update_data.insert("morning_end_time".to_string(), json!(morning_end_time.to_rfc3339()));
        }
        if let Some(afternoon_start_time) = request.afternoon_start_time {
            update_data.insert("afternoon_start_time".to_string(), json!(afternoon_start_time.to_rfc3339()));
        }
        if let Some(afternoon_end_time) = request.afternoon_end_time {
            update_data.insert("afternoon_end_time".to_string(), json!(afternoon_end_time.to_rfc3339()));
        }
        if let Some(duration) = request.duration_minutes {
            update_data.insert("duration_minutes".to_string(), json!(duration));
        }
        if let Some(is_available) = request.is_available {
            update_data.insert("is_available".to_string(), json!(is_available));
        }
        
        update_data.insert("updated_at".to_string(), json!(Utc::now().to_rfc3339()));

        let path = format!("/rest/v1/appointment_availabilities?id=eq.{}", availability_id);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::PATCH,
            &path,
            Some(auth_token),
            Some(Value::Object(update_data)),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to update availability"));
        }

        let updated_availability: DoctorAvailability = serde_json::from_value(result[0].clone())?;
        Ok(updated_availability)
    }

    /// Get doctor's availability schedules
    pub async fn get_doctor_availability(
        &self,
        doctor_id: &str,
        auth_token: &str,
    ) -> Result<Vec<DoctorAvailability>> {
        debug!("Fetching availability for doctor: {}", doctor_id);

        let path = format!("/rest/v1/appointment_availabilities?doctor_id=eq.{}&order=day_of_week.asc,morning_start_time.asc", doctor_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        let availabilities: Vec<DoctorAvailability> = result.into_iter()
            .map(|avail| serde_json::from_value(avail))
            .collect::<std::result::Result<Vec<DoctorAvailability>, _>>()?;

        Ok(availabilities)
    }

    /// Calculate available slots for a specific date
    /// NOTE: This method calculates theoretical availability based on doctor's schedule.
    /// The appointment-cell should call this and then filter out actually booked slots.
    pub async fn get_available_slots(
        &self,
        doctor_id: &str,
        query: AvailabilityQueryRequest,
        auth_token: &str,
    ) -> Result<Vec<AvailableSlot>> {
        debug!("Calculating theoretical available slots for doctor {} on {}", doctor_id, query.date);

        // Get day of week (0 = Sunday, 1 = Monday, etc.)
        let weekday = query.date.weekday();
        let day_of_week = match weekday {
            Weekday::Sun => 0,
            Weekday::Mon => 1,
            Weekday::Tue => 2,
            Weekday::Wed => 3,
            Weekday::Thu => 4,
            Weekday::Fri => 5,
            Weekday::Sat => 6,
        };

        // Get availability schedules for this day
        let availability_schedules = self.get_availability_for_day(
            doctor_id, 
            day_of_week, 
            Some(query.date),
            auth_token
        ).await?;

        // Enhanced appointment type support restored for medical scheduling

        // Check for availability overrides
        let overrides = self.get_availability_overrides(doctor_id, query.date, auth_token).await?;
        
        // If there's an override saying doctor is not available, return empty
        if let Some(override_entry) = overrides.first() {
            if !override_entry.is_available {
                debug!("Doctor has availability override for {}: not available", query.date);
                return Ok(vec![]);
            }
        }

        let mut available_slots = Vec::new();

        // Calculate theoretical slots for each availability schedule
        for schedule in availability_schedules {
            if !schedule.is_available {
                continue;
            }

            let slots = self.calculate_theoretical_slots_for_schedule(
                &schedule,
                query.date,
                query.duration_minutes,
                query.timezone.as_deref().unwrap_or("UTC"),
            ).await?;

            available_slots.extend(slots);
        }

        // Sort slots by start time
        available_slots.sort_by(|a, b| a.start_time.cmp(&b.start_time));

        // Remove duplicates and overlapping slots
        available_slots = self.remove_overlapping_slots(available_slots);

        debug!("Found {} theoretical available slots", available_slots.len());
        Ok(available_slots)
    }

    /// Create availability override (vacation, sick day, etc.)
    pub async fn create_availability_override(
        &self,
        doctor_id: &str,
        request: CreateAvailabilityOverrideRequest,
        auth_token: &str,
    ) -> Result<DoctorAvailabilityOverride> {
        debug!("Creating availability override for doctor {} on {}", doctor_id, request.override_date);

        // Check if override already exists for this date
        let existing_path = format!(
            "/rest/v1/doctor_availability_overrides?doctor_id=eq.{}&override_date=eq.{}", 
            doctor_id, 
            request.override_date
        );
        let existing: Vec<Value> = self.supabase.request(
            Method::GET,
            &existing_path,
            Some(auth_token),
            None,
        ).await?;

        if !existing.is_empty() {
            return Err(anyhow!("Availability override already exists for this date"));
        }

        let override_data = json!({
            "doctor_id": doctor_id,
            "override_date": request.override_date,
            "is_available": request.is_available,
            "reason": request.reason,
            "created_at": Utc::now().to_rfc3339()
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            "/rest/v1/doctor_availability_overrides",
            Some(auth_token),
            Some(override_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to create availability override"));
        }

        let override_entry: DoctorAvailabilityOverride = serde_json::from_value(result[0].clone())?;
        Ok(override_entry)
    }

    /// Get availability summary for multiple doctors
    pub async fn get_doctors_availability_summary(
        &self,
        doctor_ids: Vec<String>,
        date: NaiveDate,
        auth_token: &str,
    ) -> Result<Vec<DoctorAvailabilityResponse>> {
        debug!("Getting availability summary for {} doctors on {}", doctor_ids.len(), date);

        let mut responses = Vec::new();

        for doctor_id in doctor_ids {
            // Get doctor info
            let doctor_path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
            let doctor_result: Vec<Value> = self.supabase.request(
                Method::GET,
                &doctor_path,
                Some(auth_token),
                None,
            ).await?;

            if doctor_result.is_empty() {
                warn!("Doctor not found: {}", doctor_id);
                continue;
            }

            let doctor_data = &doctor_result[0];
            
            // Get theoretical available slots
            let query = AvailabilityQueryRequest {
                date,
                timezone: Some(doctor_data["timezone"].as_str().unwrap_or("UTC").to_string()),
                duration_minutes: None,
            };

            let available_slots = self.get_available_slots(&doctor_id, query, auth_token).await?;

            responses.push(DoctorAvailabilityResponse {
                doctor_id: Uuid::parse_str(&doctor_id)?,
                doctor_first_name: doctor_data["first_name"].as_str().unwrap_or("Unknown").to_string(),
                doctor_last_name: doctor_data["last_name"].as_str().unwrap_or("Doctor").to_string(),
                specialty: doctor_data["specialty"].as_str().unwrap_or("General").to_string(),
                available_slots,
                timezone: doctor_data["timezone"].as_str().unwrap_or("UTC").to_string(),
            });
        }

        Ok(responses)
    }

    /// Delete availability schedule
    pub async fn delete_availability(
        &self,
        availability_id: &str,
        auth_token: &str,
    ) -> Result<()> {
        debug!("Deleting availability: {}", availability_id);

        let path = format!("/rest/v1/appointment_availabilities?id=eq.{}", availability_id);
        let _: Vec<Value> = self.supabase.request(
            Method::DELETE,
            &path,
            Some(auth_token),
            None,
        ).await?;

        Ok(())
    }

    // Private helper methods

    async fn get_availability_by_id(
        &self,
        availability_id: &str,
        auth_token: &str,
    ) -> Result<DoctorAvailability> {
        let path = format!("/rest/v1/appointment_availabilities?id=eq.{}", availability_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Availability not found"));
        }

        let availability: DoctorAvailability = serde_json::from_value(result[0].clone())?;
        Ok(availability)
    }


    async fn get_availability_for_day(
        &self,
        doctor_id: &str,
        day_of_week: i32,
        _specific_date: Option<NaiveDate>,
        auth_token: &str,
    ) -> Result<Vec<DoctorAvailability>> {
        let path = format!(
            "/rest/v1/appointment_availabilities?doctor_id=eq.{}&day_of_week=eq.{}&is_available=eq.true", 
            doctor_id, 
            day_of_week
        );

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        let availabilities: Vec<DoctorAvailability> = result.into_iter()
            .map(|avail| serde_json::from_value(avail))
            .collect::<std::result::Result<Vec<DoctorAvailability>, _>>()?;

        Ok(availabilities)
    }

    async fn get_availability_overrides(
        &self,
        doctor_id: &str,
        date: NaiveDate,
        auth_token: &str,
    ) -> Result<Vec<DoctorAvailabilityOverride>> {
        let path = format!(
            "/rest/v1/doctor_availability_overrides?doctor_id=eq.{}&override_date=eq.{}", 
            doctor_id, 
            date
        );

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        let overrides: Vec<DoctorAvailabilityOverride> = result.into_iter()
            .map(|override_data| serde_json::from_value(override_data))
            .collect::<std::result::Result<Vec<DoctorAvailabilityOverride>, _>>()?;

        Ok(overrides)
    }

    /// Calculate theoretical slots based on doctor's availability schedule with medical scheduling support
    /// This doesn't check for actual appointments - that's the appointment-cell's responsibility
    async fn calculate_theoretical_slots_for_schedule(
        &self,
        schedule: &DoctorAvailability,
        date: NaiveDate,
        requested_duration: Option<i32>,
        timezone: &str,
    ) -> Result<Vec<AvailableSlot>> {
        let duration_minutes = requested_duration.unwrap_or(schedule.duration_minutes);
        let mut slots = Vec::new();

        // Generate morning slots if available
        if let (Some(morning_start), Some(morning_end)) = (schedule.morning_start_time, schedule.morning_end_time) {
            let morning_slots = self.generate_enhanced_slots_for_time_range(
                schedule,
                date,
                morning_start,
                morning_end,
                duration_minutes,
                timezone,
            )?;
            slots.extend(morning_slots);
        }

        // Generate afternoon slots if available
        if let (Some(afternoon_start), Some(afternoon_end)) = (schedule.afternoon_start_time, schedule.afternoon_end_time) {
            let afternoon_slots = self.generate_enhanced_slots_for_time_range(
                schedule,
                date,
                afternoon_start,
                afternoon_end,
                duration_minutes,
                timezone,
            )?;
            slots.extend(afternoon_slots);
        }

        Ok(slots)
    }

    /// Enhanced slot generation with medical scheduling features
    fn generate_enhanced_slots_for_time_range(
        &self,
        schedule: &DoctorAvailability,
        date: NaiveDate,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        duration_minutes: i32,
        timezone: &str,
    ) -> Result<Vec<AvailableSlot>> {
        let mut slots = Vec::new();
        
        // Extract time components and combine with the target date
        let start_naive_time = start_time.time();
        let end_naive_time = end_time.time();
        
        let start_datetime = date.and_time(start_naive_time).and_utc();
        let end_datetime = date.and_time(end_naive_time).and_utc();
        
        let mut current_time = start_datetime;
        let total_duration = duration_minutes + schedule.buffer_minutes;

        while current_time + Duration::minutes(total_duration as i64) <= end_datetime {
            let slot_end = current_time + Duration::minutes(duration_minutes as i64);

            // Determine slot priority based on appointment type and time
            let slot_priority = self.calculate_slot_priority(&schedule.appointment_type, current_time);

            slots.push(AvailableSlot {
                start_time: current_time,
                end_time: slot_end,
                duration_minutes,
                timezone: timezone.to_string(),
                appointment_type: schedule.appointment_type.clone(),
                buffer_minutes: schedule.buffer_minutes,
                is_concurrent_available: schedule.max_concurrent_appointments > 1,
                max_concurrent_patients: schedule.max_concurrent_appointments,
                slot_priority,
            });

            // Move to next slot including buffer time
            current_time += Duration::minutes(total_duration as i64);
        }

        Ok(slots)
    }

    /// Legacy slot generation for backwards compatibility
    fn generate_slots_for_time_range(
        &self,
        date: NaiveDate,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        duration_minutes: i32,
        timezone: &str,
    ) -> Result<Vec<AvailableSlot>> {
        let mut slots = Vec::new();
        
        // Extract time components and combine with the target date
        let start_naive_time = start_time.time();
        let end_naive_time = end_time.time();
        
        let start_datetime = date.and_time(start_naive_time).and_utc();
        let end_datetime = date.and_time(end_naive_time).and_utc();
        
        let mut current_time = start_datetime;

        while current_time + Duration::minutes(duration_minutes as i64) <= end_datetime {
            let slot_end = current_time + Duration::minutes(duration_minutes as i64);

            slots.push(AvailableSlot {
                start_time: current_time,
                end_time: slot_end,
                duration_minutes,
                timezone: timezone.to_string(),
                appointment_type: AppointmentType::GeneralConsultation, // Default
                buffer_minutes: 10, // Default buffer
                is_concurrent_available: false,
                max_concurrent_patients: 1,
                slot_priority: SlotPriority::Available,
            });

            current_time += Duration::minutes(duration_minutes as i64);
        }

        Ok(slots)
    }

    /// Calculate slot priority based on appointment type and time
    fn calculate_slot_priority(&self, appointment_type: &AppointmentType, slot_time: DateTime<Utc>) -> SlotPriority {
        use chrono::Timelike;
        
        // Priority based on appointment type
        match appointment_type {
            AppointmentType::EmergencyConsultation => SlotPriority::Emergency,
            AppointmentType::InitialConsultation => SlotPriority::Preferred,
            AppointmentType::SpecialtyConsultation => SlotPriority::Preferred,
            _ => {
                // Time-based priority - prefer morning slots and avoid lunch hours
                let hour = slot_time.hour();
                if hour < 9 || hour > 17 {
                    SlotPriority::Limited
                } else if hour >= 12 && hour <= 13 {
                    SlotPriority::Limited  // Lunch hour
                } else if hour >= 9 && hour <= 11 {
                    SlotPriority::Preferred  // Morning preferred
                } else {
                    SlotPriority::Available
                }
            }
        }
    }

    fn remove_overlapping_slots(&self, mut slots: Vec<AvailableSlot>) -> Vec<AvailableSlot> {
        if slots.is_empty() {
            return slots;
        }

        slots.sort_by(|a, b| a.start_time.cmp(&b.start_time));
        
        let mut result = Vec::new();
        let mut last_end_time = DateTime::<Utc>::MIN_UTC;

        for slot in slots {
            if slot.start_time >= last_end_time {
                last_end_time = slot.end_time;
                result.push(slot);
            }
        }

        result
    }

    ///
    /// PUBLIC API METHODS
    /// 

     /// PUBLIC: Get doctor availability without authentication
    pub async fn get_doctor_availability_public(
        &self,
        doctor_id: &str,
        query: AvailabilityQueryRequest,
    ) -> Result<Vec<AvailableSlot>, DoctorError> {
        debug!("Getting doctor availability (public): {}", doctor_id);

        // First verify doctor exists and is verified (public check)
        let doctor_service = DoctorService::new(&self.config);
        doctor_service.get_doctor_public(doctor_id).await?;

        let weekday = query.date.weekday();
        let day_of_week = match weekday {
            chrono::Weekday::Sun => 0,
            chrono::Weekday::Mon => 1,
            chrono::Weekday::Tue => 2,
            chrono::Weekday::Wed => 3,
            chrono::Weekday::Thu => 4,
            chrono::Weekday::Fri => 5,
            chrono::Weekday::Sat => 6,
        };
        let query_parts = vec![
            format!("doctor_id=eq.{}", doctor_id),
            format!("day_of_week=eq.{}", day_of_week),
            "is_available=eq.true".to_string(),
        ];

        // Note: appointment_type filtering removed as it's no longer part of the availability model

        let path = format!("/rest/v1/appointment_availabilities?{}&order=morning_start_time.asc", 
                          query_parts.join("&"));

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            None, // No auth token
            None,
        ).await.map_err(|e| {
            error!("Failed to get availability (public): {}", e);
            DoctorError::ValidationError(e.to_string())
        })?;

        // First, deserialize into DoctorAvailability structs
        let doctor_availabilities: Vec<DoctorAvailability> = result.into_iter()
            .map(|avail| serde_json::from_value(avail))
            .collect::<Result<Vec<DoctorAvailability>, _>>()
            .map_err(|e| {
                error!("Failed to parse doctor availability: {}", e);
                DoctorError::ValidationError(format!("Failed to parse doctor availability: {}", e))
            })?;

        // Convert DoctorAvailability to AvailableSlot objects
        let mut availability_slots = Vec::new();
        for availability in doctor_availabilities {
            let slots = availability.generate_medical_slots(query.date, &[]);
            availability_slots.extend(slots);
        }

        debug!("Found {} availability slots (public) for doctor: {}", availability_slots.len(), doctor_id);
        Ok(availability_slots)
    }

    /// PUBLIC: Get available slots without authentication
    pub async fn get_available_slots_public(
        &self,
        doctor_id: &str,
        query: AvailabilityQueryRequest,
    ) -> Result<Vec<AvailableSlot>, DoctorError> {
        debug!("Getting available slots (public): {}", doctor_id);

        // Get base availability
        let availability = self.get_doctor_availability_public(doctor_id, query).await?;

        // For public access, just return theoretical availability
        // Note: Actual conflict checking would be done by appointment-cell during booking
        
        debug!("Returning {} theoretical slots (public) for doctor: {}", availability.len(), doctor_id);
        Ok(availability)
    }
}