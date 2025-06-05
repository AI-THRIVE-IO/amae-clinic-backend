use anyhow::Result;
use chrono::{DateTime, Utc, Duration, Timelike};
use reqwest::Method;
use serde_json::Value;
use tracing::{debug, warn};
use uuid::Uuid;

use std::sync::Arc;
use shared_database::supabase::SupabaseClient;

use crate::models::{
    Appointment, AppointmentStatus, AppointmentType, ConflictCheckRequest, 
    ConflictCheckResponse, SuggestedSlot, AppointmentError
};

pub struct ConflictDetectionService {
    supabase: Arc<SupabaseClient>,
}

impl ConflictDetectionService {
    pub fn new(supabase: Arc<SupabaseClient>) -> Self {
        Self { supabase }
    }

    /// Check for appointment conflicts for a doctor at a specific time
    pub fn check_conflicts<'a>(
        &'a self,
        doctor_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        exclude_appointment_id: Option<Uuid>,
        auth_token: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ConflictCheckResponse, AppointmentError>> + Send + 'a>> {
        Box::pin(async move {
            debug!("Checking conflicts for doctor {} from {} to {}", 
                   doctor_id, start_time, end_time);

            // Get existing appointments for the doctor in the time range
            let existing_appointments = self.get_doctor_appointments_in_range(
                doctor_id,
                start_time,
                end_time,
                exclude_appointment_id,
                auth_token,
            ).await?;

            let mut conflicting_appointments = Vec::new();

            // Check for overlaps
            for appointment in existing_appointments {
                if self.appointments_overlap(
                    start_time, 
                    end_time, 
                    appointment.scheduled_start_time, 
                    appointment.scheduled_end_time
                ) {
                    // Only consider active appointments as conflicts
                    if self.is_active_appointment(&appointment.status) {
                        conflicting_appointments.push(appointment);
                    }
                }
            }

            let has_conflict = !conflicting_appointments.is_empty();

            // Generate suggestions if there's a conflict
            let suggested_alternatives = if has_conflict {
                // Box the recursive call to avoid infinitely sized future
                self.generate_alternative_slots(
                    doctor_id,
                    start_time,
                    end_time,
                    auth_token,
                ).await.unwrap_or_default()
            } else {
                vec![]
            };

            if has_conflict {
                warn!("Conflict detected for doctor {} - {} conflicting appointments", 
                      doctor_id, conflicting_appointments.len());
            }

            Ok(ConflictCheckResponse {
                has_conflict,
                conflicting_appointments,
                suggested_alternatives,
            })
        })
    }

    /// Perform a bulk conflict check for multiple time slots
    pub fn bulk_conflict_check<'a>(
        &'a self,
        requests: Vec<ConflictCheckRequest>,
        auth_token: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<ConflictCheckResponse>, AppointmentError>> + Send + 'a>> {
        Box::pin(async move {
            debug!("Performing bulk conflict check for {} requests", requests.len());

            let mut responses = Vec::with_capacity(requests.len());

            for request in requests {
                let response = self.check_conflicts(
                    request.doctor_id,
                    request.start_time,
                    request.end_time,
                    request.exclude_appointment_id,
                    auth_token,
                ).await?;
                responses.push(response);
            }

            Ok(responses)
        })
    }

    /// Check if a patient has too many appointments in a day
    pub async fn check_patient_daily_limit(
        &self,
        patient_id: Uuid,
        appointment_date: DateTime<Utc>,
        max_per_day: i32,
        auth_token: &str,
    ) -> Result<bool, AppointmentError> {
        debug!("Checking daily appointment limit for patient {} on {}", 
               patient_id, appointment_date.date_naive());

        let start_of_day = appointment_date.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end_of_day = appointment_date.date_naive().and_hms_opt(23, 59, 59).unwrap().and_utc();

        let appointments = self.get_patient_appointments_in_range(
            patient_id,
            start_of_day,
            end_of_day,
            auth_token,
        ).await?;

        // Count only active appointments
        let active_appointments_count = appointments.iter()
            .filter(|apt| self.is_active_appointment(&apt.status))
            .count() as i32;

        Ok(active_appointments_count < max_per_day)
    }

    /// Check for back-to-back appointment conflicts
    pub async fn check_buffer_time_conflicts(
        &self,
        doctor_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        buffer_minutes: i32,
        exclude_appointment_id: Option<Uuid>,
        auth_token: &str,
    ) -> Result<bool, AppointmentError> {
        debug!("Checking buffer time conflicts for doctor {} with {} minute buffer", 
               doctor_id, buffer_minutes);

        let buffer_duration = Duration::minutes(buffer_minutes as i64);
        let extended_start = start_time - buffer_duration;
        let extended_end = end_time + buffer_duration;

        let conflict_response = self.check_conflicts(
            doctor_id,
            extended_start,
            extended_end,
            exclude_appointment_id,
            auth_token,
        ).await?;

        Ok(!conflict_response.has_conflict)
    }

    /// Find the next available slot after a conflict
    pub async fn find_next_available_slot(
        &self,
        doctor_id: Uuid,
        preferred_start: DateTime<Utc>,
        duration_minutes: i32,
        max_search_days: i32,
        auth_token: &str,
    ) -> Result<Option<SuggestedSlot>, AppointmentError> {
        debug!("Finding next available slot for doctor {} after {}", 
               doctor_id, preferred_start);

        let duration = Duration::minutes(duration_minutes as i64);
        let search_end = preferred_start + Duration::days(max_search_days as i64);
        
        // Start searching from the preferred time
        let mut current_time = preferred_start;

        // Search in 30-minute increments
        while current_time < search_end {
            let slot_end = current_time + duration;

            let conflict_response = self.check_conflicts(
                doctor_id,
                current_time,
                slot_end,
                None,
                auth_token,
            ).await?;

            if !conflict_response.has_conflict {
                // Validate that this is during doctor's working hours
                if self.is_during_working_hours(doctor_id, current_time, auth_token).await? {
                    return Ok(Some(SuggestedSlot {
                        start_time: current_time,
                        end_time: slot_end,
                        doctor_id,
                        appointment_type: AppointmentType::GeneralConsultation, // Default
                    }));
                }
            }

            current_time += Duration::minutes(30);
        }

        Ok(None)
    }

    // ==============================================================================
    // PRIVATE HELPER METHODS
    // ==============================================================================

    async fn get_doctor_appointments_in_range(
        &self,
        doctor_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        exclude_appointment_id: Option<Uuid>,
        auth_token: &str,
    ) -> Result<Vec<Appointment>, AppointmentError> {
        let mut query_parts = vec![
            format!("doctor_id=eq.{}", doctor_id),
            format!("scheduled_start_time=lte.{}", end_time.to_rfc3339()),
            format!("scheduled_end_time=gte.{}", start_time.to_rfc3339()),
        ];

        if let Some(exclude_id) = exclude_appointment_id {
            query_parts.push(format!("id=neq.{}", exclude_id));
        }

        let path = format!("/rest/v1/appointments?{}&order=scheduled_start_time.asc", 
                          query_parts.join("&"));

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        let appointments: Vec<Appointment> = result.into_iter()
            .map(|apt| serde_json::from_value(apt))
            .collect::<std::result::Result<Vec<Appointment>, _>>()
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to parse appointments: {}", e)))?;

        Ok(appointments)
    }

    async fn get_patient_appointments_in_range(
        &self,
        patient_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        auth_token: &str,
    ) -> Result<Vec<Appointment>, AppointmentError> {
        let query_parts = vec![
            format!("patient_id=eq.{}", patient_id),
            format!("scheduled_start_time=gte.{}", start_time.to_rfc3339()),
            format!("scheduled_start_time=lte.{}", end_time.to_rfc3339()),
        ];

        let path = format!("/rest/v1/appointments?{}&order=scheduled_start_time.asc", 
                          query_parts.join("&"));

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        let appointments: Vec<Appointment> = result.into_iter()
            .map(|apt| serde_json::from_value(apt))
            .collect::<std::result::Result<Vec<Appointment>, _>>()
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to parse appointments: {}", e)))?;

        Ok(appointments)
    }

    fn appointments_overlap(
        &self,
        start1: DateTime<Utc>,
        end1: DateTime<Utc>,
        start2: DateTime<Utc>,
        end2: DateTime<Utc>,
    ) -> bool {
        // Two appointments overlap if:
        // start1 < end2 AND start2 < end1
        start1 < end2 && start2 < end1
    }

    fn is_active_appointment(&self, status: &AppointmentStatus) -> bool {
        matches!(status,
            AppointmentStatus::Pending |
            AppointmentStatus::Confirmed |
            AppointmentStatus::InProgress
        )
    }

    async fn generate_alternative_slots(
        &self,
        doctor_id: Uuid,
        original_start: DateTime<Utc>,
        original_end: DateTime<Utc>,
        auth_token: &str,
    ) -> Result<Vec<SuggestedSlot>, AppointmentError> {
        debug!("Generating alternative slots for doctor {}", doctor_id);

        let duration_minutes = (original_end - original_start).num_minutes() as i32;
        let mut suggestions = Vec::new();

        // Try to find slots on the same day
        let same_day_start = original_start.date_naive().and_hms_opt(8, 0, 0).unwrap().and_utc();
        let same_day_end = original_start.date_naive().and_hms_opt(20, 0, 0).unwrap().and_utc();

        // Search in 30-minute increments on the same day
        let mut current_time = same_day_start;
        while current_time < same_day_end && suggestions.len() < 3 {
            let slot_end = current_time + Duration::minutes(duration_minutes as i64);

            if current_time != original_start {  // Skip the original conflicting time
                let conflict_response = self.check_conflicts(
                    doctor_id,
                    current_time,
                    slot_end,
                    None,
                    auth_token,
                ).await?;

                if !conflict_response.has_conflict {
                    suggestions.push(SuggestedSlot {
                        start_time: current_time,
                        end_time: slot_end,
                        doctor_id,
                        appointment_type: AppointmentType::GeneralConsultation,
                    });
                }
            }

            current_time += Duration::minutes(30);
        }

        // If not enough slots found on same day, try next few days
        if suggestions.len() < 3 {
            for day_offset in 1..=3 {
                let next_day = original_start + Duration::days(day_offset);
                let day_start = next_day.date_naive().and_hms_opt(8, 0, 0).unwrap().and_utc();

                if let Ok(Some(slot)) = self.find_next_available_slot(
                    doctor_id,
                    day_start,
                    duration_minutes,
                    1, // Search only within that day
                    auth_token,
                ).await {
                    suggestions.push(slot);
                    if suggestions.len() >= 5 {
                        break;
                    }
                }
            }
        }

        Ok(suggestions)
    }

    async fn is_during_working_hours(
        &self,
        _doctor_id: Uuid,
        _time: DateTime<Utc>,
        _auth_token: &str,
    ) -> Result<bool, AppointmentError> {
        // TODO: Integrate with doctor availability service
        // For now, assume standard working hours (8 AM - 8 PM)
        let hour = _time.hour();
        Ok(hour >= 8 && hour < 20)
    }
}