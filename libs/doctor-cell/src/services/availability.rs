use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc, NaiveDate, NaiveTime, Datelike, Weekday, TimeZone, Duration};
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
    DoctorAvailabilityResponse, Appointment
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

        // Check for overlapping availability
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
            "buffer_minutes": request.buffer_minutes.unwrap_or(15),
            "max_concurrent_appointments": request.max_concurrent_appointments.unwrap_or(1),
            "price_per_session": request.price_per_session,
            "is_recurring": request.is_recurring.unwrap_or(true),
            "specific_date": request.specific_date,
            "is_available": true,
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

        // Get current availability to check for conflicts
        let current = self.get_availability_by_id(availability_id, auth_token).await?;

        // Validate time range if both times are provided
        if let (Some(start), Some(end)) = (request.start_time, request.end_time) {
            if start >= end {
                return Err(anyhow!("Start time must be before end time"));
            }

            // Check for conflicts
            if let Err(e) = self.check_availability_conflicts(
                &current.doctor_id.to_string(),
                current.day_of_week,
                start,
                end,
                current.specific_date,
                Some(availability_id),
                auth_token,
            ).await {
                return Err(e);
            }
        }

        // Build update object
        let mut update_data = serde_json::Map::new();
        
        if let Some(start_time) = request.start_time {
            update_data.insert("start_time".to_string(), json!(start_time.format("%H:%M:%S").to_string()));
        }
        if let Some(end_time) = request.end_time {
            update_data.insert("end_time".to_string(), json!(end_time.format("%H:%M:%S").to_string()));
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
        if let Some(price) = request.price_per_session {
            update_data.insert("price_per_session".to_string(), json!(price));
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

        let path = format!("/rest/v1/appointment_availabilities?doctor_id=eq.{}&order=day_of_week.asc,start_time.asc", doctor_id);
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
    pub async fn get_available_slots(
        &self,
        doctor_id: &str,
        query: AvailabilityQueryRequest,
        auth_token: &str,
    ) -> Result<Vec<AvailableSlot>> {
        debug!("Calculating available slots for doctor {} on {}", doctor_id, query.date);

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

        // Get existing appointments for this date
        let existing_appointments = self.get_appointments_for_date(
            doctor_id, 
            query.date, 
            auth_token
        ).await?;

        let mut available_slots = Vec::new();

        // Calculate slots for each availability schedule
        for schedule in availability_schedules {
            if !schedule.is_available {
                continue;
            }

            let slots = self.calculate_slots_for_schedule(
                &schedule,
                query.date,
                &existing_appointments,
                query.duration_minutes,
                query.timezone.as_deref().unwrap_or(schedule.timezone.as_str()),
            ).await?;

            available_slots.extend(slots);
        }

        // Sort slots by start time
        available_slots.sort_by(|a, b| a.start_time.cmp(&b.start_time));

        // Remove duplicates and overlapping slots
        available_slots = self.remove_overlapping_slots(available_slots);

        debug!("Found {} available slots", available_slots.len());
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
        appointment_type: Option<String>,
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
            
            // Get available slots
            let query = AvailabilityQueryRequest {
                date,
                timezone: Some(doctor_data["timezone"].as_str().unwrap_or("UTC").to_string()),
                appointment_type: appointment_type.clone(),
                duration_minutes: None,
            };

            let available_slots = self.get_available_slots(&doctor_id, query, auth_token).await?;

            responses.push(DoctorAvailabilityResponse {
                doctor_id: Uuid::parse_str(&doctor_id)?,
                doctor_name: doctor_data["full_name"].as_str().unwrap_or("Unknown").to_string(),
                specialty: doctor_data["specialty"].as_str().unwrap_or("General").to_string(),
                available_slots,
                timezone: doctor_data["timezone"].as_str().unwrap_or("UTC").to_string(),
                consultation_fee: doctor_data["consultation_fee"].as_f64(),
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

        // Check if there are any future appointments using this availability
        // This would require more complex logic to determine which availability slot an appointment uses
        // For now, we'll just delete it

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
            "/rest/v1/appointment_availabilities?doctor_id=eq.{}&day_of_week=eq.{}", 
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

        for avail in existing {
            let existing_start = NaiveTime::parse_from_str(
                avail["start_time"].as_str().unwrap(), 
                "%H:%M:%S"
            )?;
            let existing_end = NaiveTime::parse_from_str(
                avail["end_time"].as_str().unwrap(), 
                "%H:%M:%S"
            )?;

            // Check for overlap
            if (start_time < existing_end && end_time > existing_start) {
                return Err(anyhow!("Availability conflicts with existing schedule"));
            }
        }

        Ok(())
    }

    async fn get_availability_for_day(
        &self,
        doctor_id: &str,
        day_of_week: i32,
        specific_date: Option<NaiveDate>,
        auth_token: &str,
    ) -> Result<Vec<DoctorAvailability>> {
        let mut path = format!(
            "/rest/v1/appointment_availabilities?doctor_id=eq.{}&day_of_week=eq.{}&is_available=eq.true&order=start_time.asc", 
            doctor_id, 
            day_of_week
        );

        // Include both recurring and specific date availabilities
        if let Some(date) = specific_date {
            path = format!(
                "/rest/v1/appointment_availabilities?doctor_id=eq.{}&day_of_week=eq.{}&is_available=eq.true&or=(is_recurring.eq.true,specific_date.eq.{})&order=start_time.asc",
                doctor_id, day_of_week, date
            );
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
            .map(|override_data| serde_json::from_value(override_data))
            .collect::<std::result::Result<Vec<DoctorAvailabilityOverride>, _>>()?;

        Ok(overrides)
    }

    async fn get_appointments_for_date(
        &self,
        doctor_id: &str,
        date: NaiveDate,
        auth_token: &str,
    ) -> Result<Vec<Appointment>> {
        let start_of_day = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end_of_day = date.and_hms_opt(23, 59, 59).unwrap().and_utc();

        let path = format!(
            "/rest/v1/appointments?doctor_id=eq.{}&scheduled_start_time=gte.{}&scheduled_start_time=lte.{}&status=in.(confirmed,in_progress)&order=scheduled_start_time.asc",
            doctor_id,
            start_of_day.to_rfc3339(),
            end_of_day.to_rfc3339()
        );

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        let appointments: Vec<Appointment> = result.into_iter()
            .map(|apt| serde_json::from_value(apt))
            .collect::<std::result::Result<Vec<Appointment>, _>>()?;

        Ok(appointments)
    }

    async fn calculate_slots_for_schedule(
        &self,
        schedule: &DoctorAvailability,
        date: NaiveDate,
        existing_appointments: &[Appointment],
        requested_duration: Option<i32>,
        timezone: &str,
    ) -> Result<Vec<AvailableSlot>> {
        let duration_minutes = requested_duration.unwrap_or(schedule.duration_minutes);
        let buffer_minutes = schedule.buffer_minutes;
        let total_slot_duration = duration_minutes + buffer_minutes;

        // Convert schedule times to UTC for the given date
        let start_datetime = date.and_time(schedule.start_time).and_utc();
        let end_datetime = date.and_time(schedule.end_time).and_utc();

        let mut slots = Vec::new();
        let mut current_time = start_datetime;

        while current_time + Duration::minutes(duration_minutes as i64) <= end_datetime {
            let slot_end = current_time + Duration::minutes(duration_minutes as i64);

            // Check if this slot conflicts with existing appointments
            let has_conflict = existing_appointments.iter().any(|apt| {
                let apt_start = apt.scheduled_start_time;
                let apt_end = apt.scheduled_end_time;
                
                // Check for overlap
                current_time < apt_end && slot_end > apt_start
            });

            if !has_conflict {
                slots.push(AvailableSlot {
                    start_time: current_time,
                    end_time: slot_end,
                    duration_minutes,
                    appointment_type: schedule.appointment_type.clone(),
                    price: schedule.price_per_session,
                    timezone: timezone.to_string(),
                });
            }

            current_time += Duration::minutes(total_slot_duration as i64);
        }

        Ok(slots)
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
}