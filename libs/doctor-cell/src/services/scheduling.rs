use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc, Duration};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, error, warn};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{Appointment, AvailableSlot};
use crate::services::availability::AvailabilityService;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BookAppointmentRequest {
    pub patient_id: Uuid,
    pub doctor_id: Uuid,
    pub slot_start_time: DateTime<Utc>,
    pub slot_end_time: DateTime<Utc>,
    pub appointment_type: String,
    pub duration_minutes: i32,
    pub notes: Option<String>,
    pub timezone: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateAppointmentRequest {
    pub status: Option<String>,
    pub notes: Option<String>,
    pub reschedule_to: Option<DateTime<Utc>>,
    pub reschedule_duration: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppointmentConflictCheck {
    pub has_conflict: bool,
    pub conflicting_appointments: Vec<Appointment>,
    pub suggested_alternatives: Vec<AvailableSlot>,
}

pub struct SchedulingService {
    supabase: SupabaseClient,
    availability_service: AvailabilityService,
}

impl SchedulingService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
            availability_service: AvailabilityService::new(config),
        }
    }

    /// Book a new appointment
    pub async fn book_appointment(
        &self,
        request: BookAppointmentRequest,
        auth_token: &str,
    ) -> Result<Appointment> {
        debug!("Booking appointment for patient {} with doctor {}", 
               request.patient_id, request.doctor_id);

        // Validate the booking request
        self.validate_booking_request(&request, auth_token).await?;

        // Check for conflicts
        let conflict_check = self.check_appointment_conflicts(
            &request.doctor_id.to_string(),
            request.slot_start_time,
            request.slot_end_time,
            None, // No existing appointment to exclude
            auth_token,
        ).await?;

        if conflict_check.has_conflict {
            return Err(anyhow!("Time slot conflicts with existing appointment"));
        }

        // Get doctor information for consultation fee
        let doctor_info = self.get_doctor_info(&request.doctor_id.to_string(), auth_token).await?;
        let consultation_fee = doctor_info["consultation_fee"].as_f64();

        // Create the appointment
        let appointment_data = json!({
            "patient_id": request.patient_id,
            "doctor_id": request.doctor_id,
            "appointment_date": request.slot_start_time.to_rfc3339(),
            "status": "pending",
            "appointment_type": request.appointment_type,
            "duration_minutes": request.duration_minutes,
            "consultation_fee": consultation_fee,
            "timezone": request.timezone,
            "scheduled_start_time": request.slot_start_time.to_rfc3339(),
            "scheduled_end_time": request.slot_end_time.to_rfc3339(),
            "notes": request.notes,
            "created_at": Utc::now().to_rfc3339(),
            "updated_at": Utc::now().to_rfc3339()
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            "/rest/v1/appointments",
            Some(auth_token),
            Some(appointment_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to create appointment"));
        }

        let appointment: Appointment = serde_json::from_value(result[0].clone())?;
        
        // Update doctor's total consultations counter
        self.update_doctor_consultation_count(&request.doctor_id.to_string(), auth_token).await?;

        debug!("Appointment booked successfully with ID: {}", appointment.id);
        
        // TODO: Send confirmation notifications
        
        Ok(appointment)
    }

    /// Update an existing appointment
    pub async fn update_appointment(
        &self,
        appointment_id: &str,
        request: UpdateAppointmentRequest,
        auth_token: &str,
    ) -> Result<Appointment> {
        debug!("Updating appointment: {}", appointment_id);

        // Get current appointment
        let current_appointment = self.get_appointment(appointment_id, auth_token).await?;

        // Handle rescheduling
        if let Some(new_start_time) = request.reschedule_to {
            let duration = request.reschedule_duration.unwrap_or(current_appointment.duration_minutes);
            let new_end_time = new_start_time + Duration::minutes(duration as i64);

            // Check for conflicts with the new time
            let conflict_check = self.check_appointment_conflicts(
                &current_appointment.doctor_id.to_string(),
                new_start_time,
                new_end_time,
                Some(appointment_id), // Exclude current appointment
                auth_token,
            ).await?;

            if conflict_check.has_conflict {
                return Err(anyhow!("Reschedule time conflicts with existing appointment"));
            }
        }

        // Build update object
        let mut update_data = serde_json::Map::new();
        
        if let Some(status) = request.status {
            // Validate status transition
            self.validate_status_transition(&current_appointment.status, &status)?;
            update_data.insert("status".to_string(), json!(status));
            
            // Set actual times based on status
            match status.as_str() {
                "in_progress" => {
                    update_data.insert("actual_start_time".to_string(), json!(Utc::now().to_rfc3339()));
                },
                "completed" | "cancelled" => {
                    if current_appointment.actual_start_time.is_some() && status == "completed" {
                        update_data.insert("actual_end_time".to_string(), json!(Utc::now().to_rfc3339()));
                    }
                },
                _ => {}
            }
        }
        
        if let Some(notes) = request.notes {
            update_data.insert("notes".to_string(), json!(notes));
        }
        
        if let Some(new_start_time) = request.reschedule_to {
            let duration = request.reschedule_duration.unwrap_or(current_appointment.duration_minutes);
            let new_end_time = new_start_time + Duration::minutes(duration as i64);
            
            update_data.insert("scheduled_start_time".to_string(), json!(new_start_time.to_rfc3339()));
            update_data.insert("scheduled_end_time".to_string(), json!(new_end_time.to_rfc3339()));
            update_data.insert("appointment_date".to_string(), json!(new_start_time.to_rfc3339()));
            
            if let Some(new_duration) = request.reschedule_duration {
                update_data.insert("duration_minutes".to_string(), json!(new_duration));
            }
        }
        
        update_data.insert("updated_at".to_string(), json!(Utc::now().to_rfc3339()));

        let path = format!("/rest/v1/appointments?id=eq.{}", appointment_id);
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
            return Err(anyhow!("Failed to update appointment"));
        }

        let updated_appointment: Appointment = serde_json::from_value(result[0].clone())?;
        
        debug!("Appointment updated successfully");
        
        // TODO: Send update notifications
        
        Ok(updated_appointment)
    }

    /// Cancel an appointment
    pub async fn cancel_appointment(
        &self,
        appointment_id: &str,
        reason: Option<String>,
        auth_token: &str,
    ) -> Result<Appointment> {
        debug!("Cancelling appointment: {}", appointment_id);

        let current_appointment = self.get_appointment(appointment_id, auth_token).await?;

        // Check if appointment can be cancelled
        match current_appointment.status.as_str() {
            "completed" => return Err(anyhow!("Cannot cancel completed appointment")),
            "cancelled" => return Err(anyhow!("Appointment is already cancelled")),
            _ => {}
        }

        // Update appointment status
        let update_request = UpdateAppointmentRequest {
            status: Some("cancelled".to_string()),
            notes: reason.map(|r| format!("Cancelled: {}", r)),
            reschedule_to: None,
            reschedule_duration: None,
        };

        self.update_appointment(appointment_id, update_request, auth_token).await
    }

    /// Get appointment by ID
    pub async fn get_appointment(
        &self,
        appointment_id: &str,
        auth_token: &str,
    ) -> Result<Appointment> {
        debug!("Fetching appointment: {}", appointment_id);

        let path = format!("/rest/v1/appointments?id=eq.{}", appointment_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Appointment not found"));
        }

        let appointment: Appointment = serde_json::from_value(result[0].clone())?;
        Ok(appointment)
    }

    /// Get appointments for a patient
    pub async fn get_patient_appointments(
        &self,
        patient_id: &str,
        status_filter: Option<String>,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        auth_token: &str,
        limit: Option<i32>,
    ) -> Result<Vec<Appointment>> {
        debug!("Fetching appointments for patient: {}", patient_id);

        let mut query_parts = vec![format!("patient_id=eq.{}", patient_id)];

        if let Some(status) = status_filter {
            query_parts.push(format!("status=eq.{}", status));
        }

        if let Some(from) = from_date {
            query_parts.push(format!("scheduled_start_time=gte.{}", from.to_rfc3339()));
        }

        if let Some(to) = to_date {
            query_parts.push(format!("scheduled_start_time=lte.{}", to.to_rfc3339()));
        }

        let mut path = format!("/rest/v1/appointments?{}&order=scheduled_start_time.desc", 
                              query_parts.join("&"));

        if let Some(limit_val) = limit {
            path.push_str(&format!("&limit={}", limit_val));
        }

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

    /// Get appointments for a doctor
    pub async fn get_doctor_appointments(
        &self,
        doctor_id: &str,
        status_filter: Option<String>,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        auth_token: &str,
        limit: Option<i32>,
    ) -> Result<Vec<Appointment>> {
        debug!("Fetching appointments for doctor: {}", doctor_id);

        let mut query_parts = vec![format!("doctor_id=eq.{}", doctor_id)];

        if let Some(status) = status_filter {
            query_parts.push(format!("status=eq.{}", status));
        }

        if let Some(from) = from_date {
            query_parts.push(format!("scheduled_start_time=gte.{}", from.to_rfc3339()));
        }

        if let Some(to) = to_date {
            query_parts.push(format!("scheduled_start_time=lte.{}", to.to_rfc3339()));
        }

        let mut path = format!("/rest/v1/appointments?{}&order=scheduled_start_time.asc", 
                              query_parts.join("&"));

        if let Some(limit_val) = limit {
            path.push_str(&format!("&limit={}", limit_val));
        }

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

    /// Check for appointment conflicts
    pub async fn check_appointment_conflicts(
        &self,
        doctor_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        exclude_appointment_id: Option<&str>,
        auth_token: &str,
    ) -> Result<AppointmentConflictCheck> {
        debug!("Checking conflicts for doctor {} from {} to {}", 
               doctor_id, start_time, end_time);

        let mut query_parts = vec![
            format!("doctor_id=eq.{}", doctor_id),
            format!("status=in.(pending,confirmed,in_progress)"),
        ];

        if let Some(exclude_id) = exclude_appointment_id {
            query_parts.push(format!("id=neq.{}", exclude_id));
        }

        let path = format!("/rest/v1/appointments?{}", query_parts.join("&"));
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        let mut conflicting_appointments = Vec::new();

        for apt_value in result {
            if let Ok(appointment) = serde_json::from_value::<Appointment>(apt_value) {
                // Check for time overlap
                if start_time < appointment.scheduled_end_time && end_time > appointment.scheduled_start_time {
                    conflicting_appointments.push(appointment);
                }
            }
        }

        let has_conflict = !conflicting_appointments.is_empty();

        // If there's a conflict, suggest alternative times
        let suggested_alternatives = if has_conflict {
            self.suggest_alternative_slots(doctor_id, start_time, end_time, auth_token).await.unwrap_or_default()
        } else {
            vec![]
        };

        Ok(AppointmentConflictCheck {
            has_conflict,
            conflicting_appointments,
            suggested_alternatives,
        })
    }

    /// Get upcoming appointments (next 24 hours)
    pub async fn get_upcoming_appointments(
        &self,
        doctor_id: Option<String>,
        patient_id: Option<String>,
        auth_token: &str,
    ) -> Result<Vec<Appointment>> {
        let now = Utc::now();
        let tomorrow = now + Duration::hours(24);

        let mut query_parts = vec![
            format!("scheduled_start_time=gte.{}", now.to_rfc3339()),
            format!("scheduled_start_time=lte.{}", tomorrow.to_rfc3339()),
            "status=in.(pending,confirmed)".to_string(),
        ];

        if let Some(doc_id) = doctor_id {
            query_parts.push(format!("doctor_id=eq.{}", doc_id));
        }

        if let Some(pat_id) = patient_id {
            query_parts.push(format!("patient_id=eq.{}", pat_id));
        }

        let path = format!("/rest/v1/appointments?{}&order=scheduled_start_time.asc", 
                          query_parts.join("&"));

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

    // Private helper methods

    async fn validate_booking_request(
        &self,
        request: &BookAppointmentRequest,
        auth_token: &str,
    ) -> Result<()> {
        // Check if doctor exists and is available
        let doctor_info = self.get_doctor_info(&request.doctor_id.to_string(), auth_token).await?;
        
        if !doctor_info["is_available"].as_bool().unwrap_or(false) {
            return Err(anyhow!("Doctor is not available for appointments"));
        }

        if !doctor_info["is_verified"].as_bool().unwrap_or(false) {
            return Err(anyhow!("Doctor is not verified"));
        }

        // Check if patient exists
        let patient_path = format!("/rest/v1/patients?id=eq.{}", request.patient_id);
        let patient_result: Vec<Value> = self.supabase.request(
            Method::GET,
            &patient_path,
            Some(auth_token),
            None,
        ).await?;

        if patient_result.is_empty() {
            return Err(anyhow!("Patient not found"));
        }

        // Validate appointment time is in the future
        if request.slot_start_time <= Utc::now() {
            return Err(anyhow!("Appointment time must be in the future"));
        }

        // Validate time slot makes sense
        if request.slot_start_time >= request.slot_end_time {
            return Err(anyhow!("Invalid time slot: start time must be before end time"));
        }

        let actual_duration = (request.slot_end_time - request.slot_start_time).num_minutes() as i32;
        if actual_duration != request.duration_minutes {
            return Err(anyhow!("Duration mismatch: slot duration doesn't match requested duration"));
        }

        Ok(())
    }

    async fn get_doctor_info(&self, doctor_id: &str, auth_token: &str) -> Result<Value> {
        let path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Doctor not found"));
        }

        Ok(result[0].clone())
    }

    async fn update_doctor_consultation_count(
        &self,
        doctor_id: &str,
        auth_token: &str,
    ) -> Result<()> {
        // Get current count
        let doctor_info = self.get_doctor_info(doctor_id, auth_token).await?;
        let current_count = doctor_info["total_consultations"].as_i64().unwrap_or(0);

        // Update count
        let update_data = json!({
            "total_consultations": current_count + 1,
            "updated_at": Utc::now().to_rfc3339()
        });

        let path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
        let _: Vec<Value> = self.supabase.request(
            Method::PATCH,
            &path,
            Some(auth_token),
            Some(update_data),
        ).await?;

        Ok(())
    }

    fn validate_status_transition(&self, current_status: &str, new_status: &str) -> Result<()> {
        let valid_transitions = match current_status {
            "pending" => vec!["confirmed", "cancelled"],
            "confirmed" => vec!["in_progress", "cancelled"],
            "in_progress" => vec!["completed", "cancelled"],
            "completed" => vec![], // No transitions from completed
            "cancelled" => vec![], // No transitions from cancelled
            _ => return Err(anyhow!("Invalid current status")),
        };

        if !valid_transitions.contains(&new_status) {
            return Err(anyhow!(
                "Invalid status transition from '{}' to '{}'", 
                current_status, 
                new_status
            ));
        }

        Ok(())
    }

    async fn suggest_alternative_slots(
        &self,
        doctor_id: &str,
        _preferred_start: DateTime<Utc>,
        _preferred_end: DateTime<Utc>,
        auth_token: &str,
    ) -> Result<Vec<AvailableSlot>> {
        // Get next few days of availability
        let today = Utc::now().date_naive();
        let mut alternatives = Vec::new();

        for days_ahead in 1..=7 {
            let check_date = today + Duration::days(days_ahead);
            
            let query = crate::models::AvailabilityQueryRequest {
                date: check_date,
                timezone: Some("UTC".to_string()),
                appointment_type: None,
                duration_minutes: None,
            };

            if let Ok(slots) = self.availability_service.get_available_slots(
                doctor_id, 
                query, 
                auth_token
            ).await {
                alternatives.extend(slots);
                
                // Limit to first 5 alternatives
                if alternatives.len() >= 5 {
                    alternatives.truncate(5);
                    break;
                }
            }
        }

        Ok(alternatives)
    }
}