// libs/appointment-cell/src/services/conflict.rs
use anyhow::Result;
use chrono::{DateTime, Utc, Duration, Timelike};
use reqwest::Method;
use serde_json::Value;
use tracing::{debug, warn, info};
use uuid::Uuid;

use std::sync::Arc;
use shared_database::supabase::SupabaseClient;
// Doctor cell types not needed in conflict detection

use crate::models::{
    Appointment, AppointmentStatus, AppointmentType, ConflictCheckRequest, 
    ConflictCheckResponse, SuggestedSlot, AppointmentError
};

pub struct ConflictDetectionService {
    supabase: Arc<SupabaseClient>,
    default_buffer_minutes: i32,
    enable_concurrent_appointments: bool,
}

impl ConflictDetectionService {
    pub fn new(supabase: Arc<SupabaseClient>) -> Self {
        Self { 
            supabase,
            default_buffer_minutes: 10,
            enable_concurrent_appointments: true,
        }
    }

    pub fn with_config(supabase: Arc<SupabaseClient>, buffer_minutes: i32, enable_concurrent: bool) -> Self {
        Self {
            supabase,
            default_buffer_minutes: buffer_minutes,
            enable_concurrent_appointments: enable_concurrent,
        }
    }

    /// Enhanced conflict check with buffer times and concurrent appointment support
    pub fn check_conflicts<'a>(
        &'a self,
        doctor_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        exclude_appointment_id: Option<Uuid>,
        auth_token: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ConflictCheckResponse, AppointmentError>> + Send + 'a>> {
        Box::pin(async move {
            self.check_conflicts_with_details(
                doctor_id,
                start_time,
                end_time,
                exclude_appointment_id,
                None, // Use default appointment type
                None, // Use default buffer
                auth_token,
            ).await
        })
    }

    /// Enhanced conflict check with appointment type and buffer time support
    pub fn check_conflicts_with_details<'a>(
        &'a self,
        doctor_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        exclude_appointment_id: Option<Uuid>,
        appointment_type: Option<AppointmentType>,
        custom_buffer_minutes: Option<i32>,
        auth_token: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ConflictCheckResponse, AppointmentError>> + Send + 'a>> {
        Box::pin(async move {
        debug!("Enhanced conflict check for doctor {} from {} to {} with type {:?}", 
               doctor_id, start_time, end_time, appointment_type);

        let buffer_minutes = custom_buffer_minutes.unwrap_or(self.default_buffer_minutes);
        let buffered_start = start_time - Duration::minutes(buffer_minutes as i64);
        let buffered_end = end_time + Duration::minutes(buffer_minutes as i64);

        // Get existing appointments for the doctor in the buffered time range
        let existing_appointments = self.get_doctor_appointments_in_range(
            doctor_id,
            buffered_start,
            buffered_end,
            exclude_appointment_id,
            auth_token,
        ).await?;

        let mut conflicting_appointments = Vec::new();
        let mut concurrent_appointment_count = 0;

        // Enhanced conflict detection with concurrent appointment support
        for appointment in existing_appointments {
            if self.is_active_appointment(&appointment.status) {
                let appointment_buffered_start = appointment.scheduled_start_time() - Duration::minutes(buffer_minutes as i64);
                let appointment_buffered_end = appointment.scheduled_end_time() + Duration::minutes(buffer_minutes as i64);
                
                if self.appointments_overlap(
                    buffered_start,
                    buffered_end,
                    appointment_buffered_start,
                    appointment_buffered_end
                ) {
                    // CRITICAL FIX: Overlapping appointments are ALWAYS conflicts
                    // Concurrent appointments should be for different time slots, not overlapping ones
                    // 
                    // Check for exact time overlap (same time slot)
                    let exact_overlap = self.appointments_overlap(
                        start_time,
                        end_time,
                        appointment.scheduled_start_time(),
                        appointment.scheduled_end_time()
                    );
                    
                    if exact_overlap {
                        // Exact time overlap is ALWAYS a conflict, regardless of appointment type
                        conflicting_appointments.push(appointment);
                        info!("Time slot conflict detected for doctor {} - exact overlap", doctor_id);
                    } else {
                        // Only buffer overlap (nearby appointments) can be concurrent
                        if self.can_be_concurrent_appointment(&appointment.appointment_type, &appointment_type) {
                            concurrent_appointment_count += 1;
                            info!("Potential concurrent appointment detected for doctor {}", doctor_id);
                        } else {
                            conflicting_appointments.push(appointment);
                        }
                    }
                }
            }
        }

        // Check if concurrent appointments exceed the limit
        if concurrent_appointment_count > 0 {
            let max_concurrent = self.get_max_concurrent_appointments(&appointment_type).await;
            if concurrent_appointment_count >= max_concurrent {
                info!("Maximum concurrent appointments ({}) exceeded for doctor {}", 
                      max_concurrent, doctor_id);
                // Add the concurrent appointments as conflicts if limit exceeded
                let concurrent_appointments = self.get_concurrent_appointments(
                    doctor_id, buffered_start, buffered_end, exclude_appointment_id, auth_token
                ).await?;
                conflicting_appointments.extend(concurrent_appointments);
            }
        }

        let has_conflict = !conflicting_appointments.is_empty();

        // Generate enhanced suggestions if there's a conflict
        let suggested_alternatives = if has_conflict {
            self.generate_enhanced_alternative_slots(
                doctor_id,
                start_time,
                end_time,
                appointment_type.clone(),
                buffer_minutes,
                auth_token,
            ).await.unwrap_or_default()
        } else {
            vec![]
        };

        if has_conflict {
            warn!("Enhanced conflict detected for doctor {} - {} conflicting appointments", 
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

    /// Check if a patient has too many appointments in a day (business rule validation)
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

    /// Check for back-to-back appointment conflicts with buffer time
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

        let conflict_response = self.check_conflicts_with_details(
            doctor_id,
            start_time,
            end_time,
            exclude_appointment_id,
            None,
            Some(buffer_minutes),
            auth_token,
        ).await?;

        Ok(!conflict_response.has_conflict)
    }

    /// Check concurrent appointment capacity for a specific appointment type
    pub async fn check_concurrent_capacity(
        &self,
        doctor_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        appointment_type: &AppointmentType,
        exclude_appointment_id: Option<Uuid>,
        auth_token: &str,
    ) -> Result<bool, AppointmentError> {
        debug!("Checking concurrent capacity for doctor {} and type {:?}", 
               doctor_id, appointment_type);

        if !self.supports_concurrent_appointments(appointment_type) {
            return Ok(true); // Non-concurrent types always have capacity if no conflicts
        }

        let concurrent_appointments = self.get_concurrent_appointments(
            doctor_id, start_time, end_time, exclude_appointment_id, auth_token
        ).await?;

        let max_concurrent = self.get_max_concurrent_appointments(&Some(appointment_type.clone())).await;
        let has_capacity = concurrent_appointments.len() < max_concurrent as usize;
        
        info!("Concurrent capacity check: {}/{} slots used for doctor {}", 
              concurrent_appointments.len(), max_concurrent, doctor_id);
        
        Ok(has_capacity)
    }

    /// Find the next available slot after a conflict (enhanced implementation)
    pub async fn find_next_available_slot(
        &self,
        doctor_id: Uuid,
        preferred_start: DateTime<Utc>,
        duration_minutes: i32,
        max_search_days: i32,
        auth_token: &str,
    ) -> Result<Option<SuggestedSlot>, AppointmentError> {
        self.find_enhanced_available_slot(
            doctor_id,
            preferred_start,
            duration_minutes,
            &AppointmentType::GeneralConsultation,
            self.default_buffer_minutes,
            max_search_days,
            auth_token,
        ).await
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
        
        let start_str = start_time.to_rfc3339();
        let end_str = end_time.to_rfc3339();
        let encoded_start = urlencoding::encode(&start_str);
        let encoded_end = urlencoding::encode(&end_str);
        
        let mut query_parts = vec![
            format!("doctor_id=eq.{}", doctor_id),
            format!("scheduled_start_time=lte.{}", encoded_end),
            format!("scheduled_end_time=gte.{}", encoded_start),
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
        let start_str = start_time.to_rfc3339();
        let end_str = end_time.to_rfc3339();
        let encoded_start = urlencoding::encode(&start_str);
        let encoded_end = urlencoding::encode(&end_str);
        
        let query_parts = vec![
            format!("patient_id=eq.{}", patient_id),
            format!("scheduled_start_time=gte.{}", encoded_start),
            format!("scheduled_start_time=lte.{}", encoded_end),
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
        // Two appointments overlap if: start1 < end2 AND start2 < end1
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
        self.generate_enhanced_alternative_slots(
            doctor_id,
            original_start,
            original_end,
            None,
            self.default_buffer_minutes,
            auth_token,
        ).await
    }

    /// Enhanced alternative slot generation with appointment type awareness
    async fn generate_enhanced_alternative_slots(
        &self,
        doctor_id: Uuid,
        original_start: DateTime<Utc>,
        original_end: DateTime<Utc>,
        appointment_type: Option<AppointmentType>,
        buffer_minutes: i32,
        auth_token: &str,
    ) -> Result<Vec<SuggestedSlot>, AppointmentError> {
        debug!("Generating enhanced alternative slots for doctor {} with type {:?}", 
               doctor_id, appointment_type);

        let duration_minutes = (original_end - original_start).num_minutes() as i32;
        let mut suggestions = Vec::new();
        let slot_type = appointment_type.unwrap_or(AppointmentType::GeneralConsultation);

        // Try to find slots on the same day with enhanced logic
        let same_day_start = original_start.date_naive().and_hms_opt(8, 0, 0).unwrap().and_utc();
        let same_day_end = original_start.date_naive().and_hms_opt(20, 0, 0).unwrap().and_utc();

        // Use appointment-type specific increment
        let search_increment = self.get_search_increment(&slot_type);
        let mut current_time = same_day_start;
        
        while current_time < same_day_end && suggestions.len() < 5 {
            let slot_end = current_time + Duration::minutes(duration_minutes as i64);

            if current_time != original_start {  // Skip the original conflicting time
                let conflict_response = self.check_conflicts_with_details(
                    doctor_id,
                    current_time,
                    slot_end,
                    None,
                    Some(slot_type.clone()),
                    Some(buffer_minutes),
                    auth_token,
                ).await?;

                if !conflict_response.has_conflict && self.is_during_working_hours(current_time) {
                    suggestions.push(SuggestedSlot {
                        start_time: current_time,
                        end_time: slot_end,
                        doctor_id,
                        appointment_type: slot_type.clone(),
                    });
                }
            }

            current_time += Duration::minutes(search_increment as i64);
        }

        // If not enough slots found on same day, try next few days
        if suggestions.len() < 3 {
            for day_offset in 1..=5 {
                let next_day = original_start + Duration::days(day_offset);
                let day_start = next_day.date_naive().and_hms_opt(8, 0, 0).unwrap().and_utc();

                if let Ok(Some(slot)) = self.find_enhanced_available_slot(
                    doctor_id,
                    day_start,
                    duration_minutes,
                    &slot_type,
                    buffer_minutes,
                    1, // Search only within that day
                    auth_token,
                ).await {
                    suggestions.push(slot);
                    if suggestions.len() >= 8 {
                        break;
                    }
                }
            }
        }

        // Sort suggestions by proximity to original time and appointment type priority
        suggestions.sort_by(|a, b| {
            let a_diff = (a.start_time - original_start).abs();
            let b_diff = (b.start_time - original_start).abs();
            a_diff.cmp(&b_diff)
        });

        Ok(suggestions)
    }

    /// Basic working hours check (8 AM - 8 PM)
    /// NOTE: In production, this should integrate with doctor availability service
    fn is_during_working_hours(&self, time: DateTime<Utc>) -> bool {
        let hour = time.hour();
        hour >= 8 && hour < 20
    }

    // ==============================================================================
    // ENHANCED MEDICAL SCHEDULING HELPER METHODS
    // ==============================================================================

    /// Check if an appointment type can be concurrent with another
    fn can_be_concurrent_appointment(
        &self,
        existing_type: &AppointmentType,
        new_type: &Option<AppointmentType>,
    ) -> bool {
        if !self.enable_concurrent_appointments {
            return false;
        }

        let new_appointment_type = new_type.as_ref().unwrap_or(&AppointmentType::GeneralConsultation);
        
        // Only specific types support concurrent appointments
        matches!(
            (existing_type, new_appointment_type),
            (AppointmentType::GeneralConsultation, AppointmentType::GeneralConsultation) |
            (AppointmentType::FollowUp, AppointmentType::FollowUp) |
            (AppointmentType::MentalHealth, AppointmentType::MentalHealth)
        )
    }

    /// Get maximum concurrent appointments for an appointment type
    async fn get_max_concurrent_appointments(&self, appointment_type: &Option<AppointmentType>) -> i32 {
        match appointment_type {
            Some(AppointmentType::GeneralConsultation) => 2,
            Some(AppointmentType::FollowUp) => 3,
            Some(AppointmentType::MentalHealth) => 1, // Mental health needs full attention
            Some(AppointmentType::Urgent) => 1,       // Urgent care can't be concurrent
            _ => 1, // Default to single appointment
        }
    }

    /// Check if an appointment type supports concurrent scheduling
    fn supports_concurrent_appointments(&self, appointment_type: &AppointmentType) -> bool {
        matches!(
            appointment_type,
            AppointmentType::GeneralConsultation |
            AppointmentType::FollowUp |
            AppointmentType::MentalHealth
        )
    }

    /// Get search increment for appointment type (how often to check for slots)
    fn get_search_increment(&self, appointment_type: &AppointmentType) -> i32 {
        match appointment_type {
            AppointmentType::Urgent => 15,        // Check every 15 minutes for urgent
            AppointmentType::Prescription => 15,  // Quick prescription renewals
            AppointmentType::FollowUp => 20,      // Regular follow-ups
            _ => 30,                              // Standard 30-minute increments
        }
    }

    /// Get concurrent appointments in a time range
    async fn get_concurrent_appointments(
        &self,
        doctor_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        exclude_appointment_id: Option<Uuid>,
        auth_token: &str,
    ) -> Result<Vec<Appointment>, AppointmentError> {
        let appointments = self.get_doctor_appointments_in_range(
            doctor_id, start_time, end_time, exclude_appointment_id, auth_token
        ).await?;

        // Filter to only concurrent-capable appointments
        let concurrent_appointments: Vec<Appointment> = appointments
            .into_iter()
            .filter(|apt| {
                self.is_active_appointment(&apt.status) &&
                self.supports_concurrent_appointments(&apt.appointment_type)
            })
            .collect();

        Ok(concurrent_appointments)
    }

    /// Enhanced available slot finder with appointment type support
    async fn find_enhanced_available_slot(
        &self,
        doctor_id: Uuid,
        preferred_start: DateTime<Utc>,
        duration_minutes: i32,
        appointment_type: &AppointmentType,
        buffer_minutes: i32,
        max_search_days: i32,
        auth_token: &str,
    ) -> Result<Option<SuggestedSlot>, AppointmentError> {
        debug!("Finding enhanced available slot for doctor {} with type {:?}", 
               doctor_id, appointment_type);

        let duration = Duration::minutes(duration_minutes as i64);
        let search_end = preferred_start + Duration::days(max_search_days as i64);
        let search_increment = self.get_search_increment(appointment_type);
        
        let mut current_time = preferred_start;

        while current_time < search_end {
            let slot_end = current_time + duration;

            let conflict_response = self.check_conflicts_with_details(
                doctor_id,
                current_time,
                slot_end,
                None,
                Some(appointment_type.clone()),
                Some(buffer_minutes),
                auth_token,
            ).await?;

            if !conflict_response.has_conflict {
                // Validate that this is during working hours
                if self.is_during_working_hours(current_time) {
                    return Ok(Some(SuggestedSlot {
                        start_time: current_time,
                        end_time: slot_end,
                        doctor_id,
                        appointment_type: appointment_type.clone(),
                    }));
                }
            }

            current_time += Duration::minutes(search_increment as i64);
        }

        Ok(None)
    }
}