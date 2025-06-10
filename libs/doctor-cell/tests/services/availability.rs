// libs/doctor-cell/src/services/availability.rs

use anyhow::{Result, anyhow};
use chrono::{NaiveDate, NaiveTime, DateTime, Utc, Datelike, Weekday, Duration};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, warn};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{
    DoctorAvailability, DoctorAvailabilityOverride, AvailableSlot,
    CreateAvailabilityRequest, UpdateAvailabilityRequest,
    CreateAvailabilityOverrideRequest, AvailabilityQueryRequest,
    DoctorAvailabilityResponse
};

pub struct AvailabilityService {
    supabase: SupabaseClient,
}

impl AvailabilityService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
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

        // Validate time range
        if request.start_time >= request.end_time {
            return Err(anyhow!("Start time must be before end time"));
        }

        // Validate day of week (0-6)
        if request.day_of_week < 0 || request.day_of_week > 6 {
            return Err(anyhow!("Day of week must be between 0 (Sunday) and 6 (Saturday)"));
        }

        // Check for overlapping availability - FIXED TABLE NAME
        if let Err(e) = self.check_availability_conflicts(
            doctor_id,
            request.day_of_week,
            request.start_time,
            request.end_time,
            request.specific_date,
            None, // No existing ID to exclude
            auth_token,
        ).await {
            return Err(e);
        }

        let availability_data = json!({
            "doctor_id": doctor_id,
            "day_of_week": request.day_of_week,
            "start_time": request.start_time.format("%H:%M:%S").to_string(),
            "end_time": request.end_time.format("%H:%M:%S").to_string(),
            "duration_minutes": request.duration_minutes,
            "timezone": request.timezone,
            "appointment_type": request.appointment_type,
            "buffer_minutes": request.buffer_minutes.unwrap_or(0),
            "max_concurrent_appointments": request.max_concurrent_appointments.unwrap_or(1),
            "is_recurring": request.is_recurring.unwrap_or(true),
            "specific_date": request.specific_date,
            "is_available": true,
            "created_at": Utc::now().to_rfc3339(),
            "updated_at": Utc::now().to_rfc3339()
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        // FIXED: Use consistent table name
        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            "/rest/v1/doctor_availability",
            Some(auth_token),
            Some(availability_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to create availability"));
        }

        let availability: DoctorAvailability = serde_json::from_value(result[0].clone())?;
        debug!("Availability created successfully with ID: {}", availability.id);

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

        // Get existing availability to validate ownership
        let existing = self.get_availability_by_id(availability_id, auth_token).await?;

        // Validate time range if provided
        let start_time = request.start_time.unwrap_or(existing.start_time);
        let end_time = request.end_time.unwrap_or(existing.end_time);
        
        if start_time >= end_time {
            return Err(anyhow!("Start time must be before end time"));
        }

        // Build update object with only provided fields
        let mut update_data = serde_json::Map::new();

        if let Some(start) = request.start_time {
            update_data.insert("start_time".to_string(), json!(start.format("%H:%M:%S").to_string()));
        }
        if let Some(end) = request.end_time {
            update_data.insert("end_time".to_string(), json!(end.format("%H:%M:%S").to_string()));
        }
        if let Some(duration) = request.duration_minutes {
            update_data.insert("duration_minutes".to_string(), json!(duration));
        }
        if let Some(timezone) = request.timezone {
            update_data.insert("timezone".to_string(), json!(timezone));
        }
        if let Some(buffer) = request.buffer_minutes {
            update_data.insert("buffer_minutes".to_string(), json!(buffer));
        }
        if let Some(max_concurrent) = request.max_concurrent_appointments {
            update_data.insert("max_concurrent_appointments".to_string(), json!(max_concurrent));
        }
        if let Some(available) = request.is_available {
            update_data.insert("is_available".to_string(), json!(available));
        }
        
        update_data.insert("updated_at".to_string(), json!(Utc::now().to_rfc3339()));

        // FIXED: Use consistent table name
        let path = format!("/rest/v1/doctor_availability?id=eq.{}", availability_id);
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

    /// Get doctor's availability schedules - FIXED TABLE NAME
    pub async fn get_doctor_availability(
        &self,
        doctor_id: &str,
        auth_token: &str,
    ) -> Result<Vec<DoctorAvailability>> {
        debug!("Fetching availability for doctor: {}", doctor_id);

        // FIXED: Use consistent table name
        let path = format!("/rest/v1/doctor_availability?doctor_id=eq.{}&order=day_of_week.asc,start_time.asc", doctor_id);
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
        let mut availability_schedules = self.get_availability_for_day(
            doctor_id, 
            day_of_week, 
            Some(query.date),
            auth_token
        ).await?;

        // Filter by appointment type if specified
        if let Some(ref appointment_type) = query.appointment_type {
            availability_schedules.retain(|avail| avail.appointment_type == *appointment_type);
        }

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
                query.timezone.as_deref().unwrap_or(schedule.timezone.as_str()),
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

        // Check if override already exists for this date - FIXED TABLE NAME
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

    /// Delete availability schedule - FIXED TABLE NAME
    pub async fn delete_availability(
        &self,
        availability_id: &str,
        auth_token: &str,
    ) -> Result<()> {
        debug!("Deleting availability: {}", availability_id);

        // FIXED: Use consistent table name
        let path = format!("/rest/v1/doctor_availability?id=eq.{}", availability_id);
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
        // FIXED: Use consistent table name
        let path = format!("/rest/v1/doctor_availability?id=eq.{}", availability_id);
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
        day_of_week: u32,
        specific_date: Option<NaiveDate>,
        auth_token: &str,
    ) -> Result<Vec<DoctorAvailability>> {
        let mut path = format!(
            "/rest/v1/doctor_availability?doctor_id=eq.{}&day_of_week=eq.{}&is_available=eq.true", 
            doctor_id, 
            day_of_week
        );

        // If specific date is provided, also check for date-specific availability
        if let Some(date) = specific_date {
            path.push_str(&format!("&specific_date=eq.{}", date));
        }

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
            .map(|override_val| serde_json::from_value(override_val))
            .collect::<std::result::Result<Vec<DoctorAvailabilityOverride>, _>>()?;

        Ok(overrides)
    }

    async fn check_availability_conflicts(
        &self,
        doctor_id: &str,
        day_of_week: i32,
        start_time: NaiveTime,
        end_time: NaiveTime,
        specific_date: Option<NaiveDate>,
        exclude_id: Option<&str>,
        auth_token: &str,
    ) -> Result<()> {
        let mut path = format!(
            "/rest/v1/doctor_availability?doctor_id=eq.{}&day_of_week=eq.{}", 
            doctor_id, 
            day_of_week
        );

        if let Some(date) = specific_date {
            path.push_str(&format!("&specific_date=eq.{}", date));
        }

        if let Some(id) = exclude_id {
            path.push_str(&format!("&id=neq.{}", id));
        }

        let existing: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        for availability in existing {
            let existing_start = NaiveTime::parse_from_str(
                availability["start_time"].as_str().unwrap_or("00:00:00"), 
                "%H:%M:%S"
            )?;
            let existing_end = NaiveTime::parse_from_str(
                availability["end_time"].as_str().unwrap_or("23:59:59"), 
                "%H:%M:%S"
            )?;

            // Check for time overlap
            if (start_time < existing_end && end_time > existing_start) {
                return Err(anyhow!(
                    "Availability conflicts with existing schedule: {} - {}", 
                    existing_start, 
                    existing_end
                ));
            }
        }

        Ok(())
    }

    async fn calculate_theoretical_slots_for_schedule(
        &self,
        schedule: &DoctorAvailability,
        date: NaiveDate,
        duration_minutes: Option<i32>,
        timezone: &str,
    ) -> Result<Vec<AvailableSlot>> {
        let slot_duration = duration_minutes.unwrap_or(schedule.duration_minutes);
        let buffer = schedule.buffer_minutes.unwrap_or(0);
        
        let mut slots = Vec::new();
        let mut current_time = schedule.start_time;
        
        // Generate slots within the availability window
        while current_time.clone() + Duration::minutes((slot_duration + buffer) as i64) <= schedule.end_time {
            let slot_start = date.and_time(current_time).and_utc();
            let slot_end = slot_start + Duration::minutes(slot_duration as i64);
            
            slots.push(AvailableSlot {
                start_time: slot_start,
                end_time: slot_end,
                duration_minutes: slot_duration,
                appointment_type: schedule.appointment_type.clone(),
                timezone: timezone.to_string(),
                is_available: true,
            });
            
            current_time = current_time + Duration::minutes((slot_duration + buffer) as i64);
        }
        
        Ok(slots)
    }

    fn remove_overlapping_slots(&self, mut slots: Vec<AvailableSlot>) -> Vec<AvailableSlot> {
        slots.sort_by(|a, b| a.start_time.cmp(&b.start_time));
        
        let mut result = Vec::new();
        let mut last_end_time: Option<DateTime<Utc>> = None;
        
        for slot in slots {
            if let Some(last_end) = last_end_time {
                if slot.start_time >= last_end {
                    result.push(slot.clone());
                    last_end_time = Some(slot.end_time);
                }
                // Skip overlapping slots
            } else {
                result.push(slot.clone());
                last_end_time = Some(slot.end_time);
            }
        }
        
        result
    }
}