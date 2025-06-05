// libs/appointment-cell/src/services/booking.rs
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc, Duration, NaiveTime};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, info, warn, error};
use uuid::Uuid;
use std::sync::Arc;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;
use doctor_cell::services::matching::DoctorMatchingService;
use doctor_cell::models::{DoctorMatchingRequest, DoctorMatch};

use crate::models::{
    Appointment, AppointmentStatus, AppointmentType, BookAppointmentRequest,
    UpdateAppointmentRequest, RescheduleAppointmentRequest, CancelAppointmentRequest,
    AppointmentSearchQuery, AppointmentStats, AppointmentError,
    AppointmentValidationRules, CancelledBy, SmartBookingRequest, SmartBookingResponse,
    AlternativeSlot
};
use crate::services::conflict::ConflictDetectionService;
use crate::services::lifecycle::AppointmentLifecycleService;

pub struct AppointmentBookingService {
    supabase: Arc<SupabaseClient>,
    conflict_service: ConflictDetectionService,
    lifecycle_service: AppointmentLifecycleService,
    doctor_matching_service: DoctorMatchingService, // NEW: Integrated doctor matching
    validation_rules: AppointmentValidationRules,
}

impl AppointmentBookingService {
    pub fn new(config: &AppConfig) -> Self {
        let supabase = Arc::new(SupabaseClient::new(config));

        let conflict_service = ConflictDetectionService::new(Arc::clone(&supabase));
        let lifecycle_service = AppointmentLifecycleService::new();
        let doctor_matching_service = DoctorMatchingService::new(config); // NEW

        Self {
            conflict_service,
            lifecycle_service,
            doctor_matching_service, // NEW
            supabase,
            validation_rules: AppointmentValidationRules::default(),
        }
    }

    /// NEW: Smart booking with automatic doctor selection and history prioritization
    pub async fn smart_book_appointment(
        &self,
        request: SmartBookingRequest,
        auth_token: &str,
    ) -> Result<SmartBookingResponse, AppointmentError> {
        info!("Smart booking appointment for patient {} with specialty {:?}", 
              request.patient_id, request.specialty_required);

        // **Step 1: Comprehensive Validation**
        self.validate_smart_booking_request(&request).await?;
        
        // **Step 2: Find Best Doctor Match with History Prioritization**
        let doctor_match = self.find_best_doctor_match(&request, auth_token).await?;
        
        // **Step 3: Select Best Available Slot**
        let selected_slot = self.select_optimal_slot(&doctor_match, &request).await?;
        
        // **Step 4: Create Traditional Booking Request**
        let booking_request = BookAppointmentRequest {
            patient_id: request.patient_id,
            doctor_id: Some(doctor_match.doctor.id),
            appointment_date: selected_slot.start_time,
            appointment_type: request.appointment_type,
            duration_minutes: request.duration_minutes,
            timezone: request.timezone,
            patient_notes: request.patient_notes,
            preferred_language: None,
            specialty_required: request.specialty_required,
        };
        
        // **Step 5: Book the Appointment**
        let appointment = self.book_appointment(booking_request, auth_token).await?;
        
        // **Step 6: Generate Alternative Slots**
        let alternative_slots = self.generate_alternative_slots(
            &request, 
            &doctor_match.doctor.id, 
            auth_token
        ).await?;

        // **Step 7: Check if this is a preferred doctor (has history)**
        let is_preferred_doctor = doctor_match.match_reasons.iter()
            .any(|reason| reason.contains("Previous patient"));

        info!("Smart booking completed for appointment {} with doctor {} (preferred: {})", 
              appointment.id, doctor_match.doctor.id, is_preferred_doctor);

        Ok(SmartBookingResponse {
            appointment,
            doctor_match_score: doctor_match.match_score,
            match_reasons: doctor_match.match_reasons,
            is_preferred_doctor,
            alternative_slots,
        })
    }

    /// Enhanced appointment booking with specialty validation and history awareness
    pub async fn book_appointment(
        &self,
        request: BookAppointmentRequest,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
        info!("Booking appointment for patient {} with doctor {:?}", 
              request.patient_id, request.doctor_id);

        // **Step 1: Comprehensive Validation**
        self.validate_booking_request(&request).await?;
        
        // **Step 2: Verify Patient Exists**
        self.verify_patient_exists(&request.patient_id, auth_token).await?;
        
        // **Step 3: Doctor Selection and Validation**
        let selected_doctor_id = if let Some(doctor_id) = request.doctor_id {
            // Validate specific doctor
            self.validate_specific_doctor(doctor_id, &request, auth_token).await?;
            doctor_id
        } else {
            // Find best doctor automatically using history prioritization
            self.find_best_available_doctor(&request, auth_token).await?
        };
        
        // **Step 4: Detect Conflicts**
        let end_time = request.appointment_date + Duration::minutes(request.duration_minutes as i64);
        let conflict_check = self.conflict_service.check_conflicts(
            selected_doctor_id,
            request.appointment_date,
            end_time,
            None,
            auth_token,
        ).await?;

        if conflict_check.has_conflict {
            warn!("Appointment conflict detected for doctor {} at {}", 
                  selected_doctor_id, request.appointment_date);
            return Err(AppointmentError::ConflictDetected);
        }

        // **Step 5: Create Appointment Record**
        let appointment = self.create_appointment_record(
            selected_doctor_id,
            request,
            auth_token,
        ).await?;

        // **Step 6: Post-Creation Tasks**
        self.handle_post_booking_tasks(&appointment, auth_token).await?;

        info!("Appointment {} booked successfully with doctor {}", 
              appointment.id, selected_doctor_id);
        Ok(appointment)
    }

    /// Update an existing appointment
    pub async fn update_appointment(
        &self,
        appointment_id: Uuid,
        request: UpdateAppointmentRequest,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
        debug!("Updating appointment: {}", appointment_id);

        // Get current appointment
        let current_appointment = self.get_appointment(appointment_id, auth_token).await?;

        // Handle status transitions
        if let Some(new_status) = &request.status {
            self.lifecycle_service.validate_status_transition(
                &current_appointment.status,
                new_status,
            )?;
        }

        // Handle rescheduling
        if let Some(new_start_time) = request.reschedule_to {
            let new_duration = request.reschedule_duration.unwrap_or(current_appointment.duration_minutes);
            let new_end_time = new_start_time + Duration::minutes(new_duration as i64);

            // Validate reschedule timing
            self.validate_reschedule_timing(&current_appointment, new_start_time)?;

            // Check for conflicts with new time
            let conflict_check = self.conflict_service.check_conflicts(
                current_appointment.doctor_id,
                new_start_time,
                new_end_time,
                Some(appointment_id),
                auth_token,
            ).await?;

            if conflict_check.has_conflict {
                return Err(AppointmentError::ConflictDetected);
            }
        }

        // Perform the update
        let updated_appointment = self.update_appointment_record(
            &current_appointment,
            request,
            auth_token,
        ).await?;

        info!("Appointment {} updated successfully", appointment_id);
        Ok(updated_appointment)
    }

    /// Reschedule an appointment to a new time
    pub async fn reschedule_appointment(
        &self,
        appointment_id: Uuid,
        request: RescheduleAppointmentRequest,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
        debug!("Rescheduling appointment: {}", appointment_id);

        let current_appointment = self.get_appointment(appointment_id, auth_token).await?;

        // Validate reschedule is allowed
        self.validate_reschedule_timing(&current_appointment, request.new_start_time)?;

        let update_request = UpdateAppointmentRequest {
            status: Some(AppointmentStatus::Rescheduled),
            doctor_notes: request.reason.clone(),
            patient_notes: None,
            reschedule_to: Some(request.new_start_time),
            reschedule_duration: request.new_duration_minutes,
        };

        self.update_appointment(appointment_id, update_request, auth_token).await
    }

    /// Cancel an appointment
    pub async fn cancel_appointment(
        &self,
        appointment_id: Uuid,
        request: CancelAppointmentRequest,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
        debug!("Cancelling appointment: {}", appointment_id);

        let current_appointment = self.get_appointment(appointment_id, auth_token).await?;

        // Validate cancellation is allowed
        self.validate_cancellation_timing(&current_appointment)?;

        // Determine who cancelled for audit trail
        let cancellation_note = format!("Cancelled by {:?}: {}", request.cancelled_by, request.reason);

        let update_request = UpdateAppointmentRequest {
            status: Some(AppointmentStatus::Cancelled),
            doctor_notes: Some(cancellation_note),
            patient_notes: None,
            reschedule_to: None,
            reschedule_duration: None,
        };

        let cancelled_appointment = self.update_appointment(appointment_id, update_request, auth_token).await?;

        // Handle post-cancellation tasks
        self.handle_post_cancellation_tasks(&cancelled_appointment, &request, auth_token).await?;

        info!("Appointment {} cancelled successfully", appointment_id);
        Ok(cancelled_appointment)
    }

    /// Get appointment by ID
    pub async fn get_appointment(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
        debug!("Fetching appointment: {}", appointment_id);

        let path = format!("/rest/v1/appointments?id=eq.{}", appointment_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        if result.is_empty() {
            return Err(AppointmentError::NotFound);
        }

        let appointment: Appointment = serde_json::from_value(result[0].clone())
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to parse appointment: {}", e)))?;

        Ok(appointment)
    }

    /// Search appointments with filters
    pub async fn search_appointments(
        &self,
        query: AppointmentSearchQuery,
        auth_token: &str,
    ) -> Result<Vec<Appointment>, AppointmentError> {
        debug!("Searching appointments with filters: {:?}", query);

        let mut query_parts = Vec::new();

        // Build query filters
        if let Some(patient_id) = query.patient_id {
            query_parts.push(format!("patient_id=eq.{}", patient_id));
        }
        if let Some(doctor_id) = query.doctor_id {
            query_parts.push(format!("doctor_id=eq.{}", doctor_id));
        }
        if let Some(status) = query.status {
            query_parts.push(format!("status=eq.{}", status));
        }
        if let Some(appointment_type) = query.appointment_type {
            query_parts.push(format!("appointment_type=eq.{}", appointment_type));
        }
        if let Some(from_date) = query.from_date {
            query_parts.push(format!("scheduled_start_time=gte.{}", from_date.to_rfc3339()));
        }
        if let Some(to_date) = query.to_date {
            query_parts.push(format!("scheduled_start_time=lte.{}", to_date.to_rfc3339()));
        }

        let mut path = format!("/rest/v1/appointments?{}&order=scheduled_start_time.desc", 
                              query_parts.join("&"));

        if let Some(limit) = query.limit {
            path.push_str(&format!("&limit={}", limit));
        }
        if let Some(offset) = query.offset {
            path.push_str(&format!("&offset={}", offset));
        }

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

    /// Get upcoming appointments (next 24 hours)
    pub async fn get_upcoming_appointments(
        &self,
        patient_id: Option<Uuid>,
        doctor_id: Option<Uuid>,
        auth_token: &str,
    ) -> Result<Vec<Appointment>, AppointmentError> {
        let now = Utc::now();
        let tomorrow = now + Duration::hours(24);

        let query = AppointmentSearchQuery {
            patient_id,
            doctor_id,
            status: None,
            appointment_type: None,
            from_date: Some(now),
            to_date: Some(tomorrow),
            limit: Some(50),
            offset: None,
        };

        let mut appointments = self.search_appointments(query, auth_token).await?;
        
        // Filter to only include active appointments
        appointments.retain(|apt| matches!(apt.status, 
            AppointmentStatus::Pending | 
            AppointmentStatus::Confirmed | 
            AppointmentStatus::InProgress
        ));

        Ok(appointments)
    }

    /// Get appointment statistics with doctor continuity metrics
    pub async fn get_appointment_stats(
        &self,
        patient_id: Option<Uuid>,
        doctor_id: Option<Uuid>,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        auth_token: &str,
    ) -> Result<AppointmentStats, AppointmentError> {
        debug!("Calculating appointment statistics");

        let query = AppointmentSearchQuery {
            patient_id,
            doctor_id,
            status: None,
            appointment_type: None,
            from_date,
            to_date,
            limit: None,
            offset: None,
        };

        let appointments = self.search_appointments(query, auth_token).await?;

        let total_appointments = appointments.len() as i32;
        let completed_appointments = appointments.iter()
            .filter(|apt| apt.status == AppointmentStatus::Completed)
            .count() as i32;
        let cancelled_appointments = appointments.iter()
            .filter(|apt| apt.status == AppointmentStatus::Cancelled)
            .count() as i32;
        let no_show_appointments = appointments.iter()
            .filter(|apt| apt.status == AppointmentStatus::NoShow)
            .count() as i32;

        let average_consultation_duration = if completed_appointments > 0 {
            appointments.iter()
                .filter(|apt| apt.status == AppointmentStatus::Completed)
                .map(|apt| apt.duration_minutes)
                .sum::<i32>() / completed_appointments
        } else {
            0
        };

        // Calculate appointment type breakdown
        let mut type_breakdown = std::collections::HashMap::new();
        for appointment in &appointments {
            *type_breakdown.entry(appointment.appointment_type.clone()).or_insert(0) += 1;
        }
        let appointment_type_breakdown: Vec<(AppointmentType, i32)> = type_breakdown.into_iter().collect();

        // NEW: Calculate doctor continuity rate
        let doctor_continuity_rate = if let Some(patient_id) = patient_id {
            self.calculate_doctor_continuity_rate(patient_id, auth_token).await.unwrap_or(0.0)
        } else {
            0.0
        };

        Ok(AppointmentStats {
            total_appointments,
            completed_appointments,
            cancelled_appointments,
            no_show_appointments,
            average_consultation_duration,
            appointment_type_breakdown,
            doctor_continuity_rate, // NEW
        })
    }

    /// Public method to check appointment conflicts (for handler use)
    pub async fn check_conflicts(
        &self,
        doctor_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        exclude_appointment_id: Option<Uuid>,
        auth_token: &str,
    ) -> Result<crate::models::ConflictCheckResponse, AppointmentError> {
        self.conflict_service
            .check_conflicts(doctor_id, start_time, end_time, exclude_appointment_id, auth_token)
            .await
    }

    // ==============================================================================
    // PRIVATE HELPER METHODS - ENHANCED WITH HISTORY PRIORITIZATION
    // ==============================================================================

    /// NEW: Find best doctor match using history prioritization
    async fn find_best_doctor_match(
        &self,
        request: &SmartBookingRequest,
        auth_token: &str,
    ) -> Result<DoctorMatch, AppointmentError> {
        debug!("Finding best doctor match for patient {} with specialty {:?}", 
               request.patient_id, request.specialty_required);

        let matching_request = DoctorMatchingRequest {
            patient_id: request.patient_id,
            preferred_date: request.preferred_date,
            preferred_time_start: request.preferred_time_start,
            preferred_time_end: request.preferred_time_end,
            specialty_required: request.specialty_required.clone(),
            appointment_type: request.appointment_type.to_string(),
            duration_minutes: request.duration_minutes,
            timezone: request.timezone.clone(),
        };

        let best_match = self.doctor_matching_service
            .find_best_doctor(matching_request, auth_token)
            .await
            .map_err(|e| match e {
                doctor_cell::models::DoctorError::NotAvailable => {
                    if let Some(specialty) = &request.specialty_required {
                        AppointmentError::SpecialtyNotAvailable { 
                            specialty: specialty.clone() 
                        }
                    } else {
                        AppointmentError::DoctorNotAvailable
                    }
                },
                _ => AppointmentError::DoctorMatchingError(e.to_string()),
            })?;

        best_match.ok_or_else(|| {
            if let Some(specialty) = &request.specialty_required {
                AppointmentError::SpecialtyNotAvailable { 
                    specialty: specialty.clone() 
                }
            } else {
                AppointmentError::DoctorNotAvailable
            }
        })
    }

    /// NEW: Select optimal slot from doctor's available slots
    async fn select_optimal_slot(
        &self,
        doctor_match: &DoctorMatch,
        request: &SmartBookingRequest,
    ) -> Result<&doctor_cell::models::AvailableSlot, AppointmentError> {
        if doctor_match.available_slots.is_empty() {
            return Err(AppointmentError::SlotNotAvailable);
        }

        // Prefer slots that match the requested time window
        if let (Some(start_time), Some(end_time)) = (request.preferred_time_start, request.preferred_time_end) {
            for slot in &doctor_match.available_slots {
                let slot_time = slot.start_time.time();
                if slot_time >= start_time && slot_time <= end_time {
                    return Ok(slot);
                }
            }
        }

        // Return the first available slot if no preference match
        Ok(&doctor_match.available_slots[0])
    }

    /// NEW: Generate alternative appointment slots
    async fn generate_alternative_slots(
        &self,
        request: &SmartBookingRequest,
        exclude_doctor_id: &Uuid,
        auth_token: &str,
    ) -> Result<Vec<AlternativeSlot>, AppointmentError> {
        debug!("Generating alternative slots for patient {}", request.patient_id);

        let matching_request = DoctorMatchingRequest {
            patient_id: request.patient_id,
            preferred_date: request.preferred_date,
            preferred_time_start: request.preferred_time_start,
            preferred_time_end: request.preferred_time_end,
            specialty_required: request.specialty_required.clone(),
            appointment_type: request.appointment_type.to_string(),
            duration_minutes: request.duration_minutes,
            timezone: request.timezone.clone(),
        };

        let matches = self.doctor_matching_service
            .find_matching_doctors(matching_request, auth_token, Some(5))
            .await
            .map_err(|e| AppointmentError::DoctorMatchingError(e.to_string()))?;

        let mut alternatives = Vec::new();
        for doctor_match in matches {
            if doctor_match.doctor.id == *exclude_doctor_id {
                continue; // Skip the already selected doctor
            }

            for slot in doctor_match.available_slots.iter().take(2) { // Max 2 slots per doctor
                let has_history = doctor_match.match_reasons.iter()
                    .any(|reason| reason.contains("Previous patient"));

                alternatives.push(AlternativeSlot {
                    doctor_id: doctor_match.doctor.id,
                    doctor_name: doctor_match.doctor.full_name.clone(),
                    start_time: slot.start_time,
                    end_time: slot.end_time,
                    match_score: doctor_match.match_score,
                    has_patient_history: has_history,
                });
            }
        }

        // Sort by match score (history prioritization)
        alternatives.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap());
        alternatives.truncate(10); // Limit to 10 alternatives

        Ok(alternatives)
    }

    /// NEW: Calculate doctor continuity rate for patient
    async fn calculate_doctor_continuity_rate(
        &self,
        patient_id: Uuid,
        auth_token: &str,
    ) -> Result<f32, AppointmentError> {
        let query = AppointmentSearchQuery {
            patient_id: Some(patient_id),
            doctor_id: None,
            status: Some(AppointmentStatus::Completed),
            appointment_type: None,
            from_date: None,
            to_date: None,
            limit: None,
            offset: None,
        };

        let appointments = self.search_appointments(query, auth_token).await?;
        
        if appointments.len() < 2 {
            return Ok(0.0); // Need at least 2 appointments to calculate continuity
        }

        let mut doctor_counts = std::collections::HashMap::new();
        for appointment in &appointments {
            *doctor_counts.entry(appointment.doctor_id).or_insert(0) += 1;
        }

        // Calculate continuity as percentage of appointments with previously seen doctors
        let repeat_appointments = doctor_counts.values()
            .filter(|&&count| count > 1)
            .map(|&count| count - 1) // Subtract first visit
            .sum::<i32>();

        let continuity_rate = repeat_appointments as f32 / appointments.len() as f32;
        Ok(continuity_rate)
    }

    /// ENHANCED: Find best available doctor automatically with history prioritization
    async fn find_best_available_doctor(
        &self,
        request: &BookAppointmentRequest,
        auth_token: &str,
    ) -> Result<Uuid, AppointmentError> {
        debug!("Finding best available doctor for patient {} with specialty {:?}", 
               request.patient_id, request.specialty_required);

        let matching_request = DoctorMatchingRequest {
            patient_id: request.patient_id,
            preferred_date: Some(request.appointment_date.date_naive()),
            preferred_time_start: Some(request.appointment_date.time()),
            preferred_time_end: Some((request.appointment_date + Duration::hours(2)).time()),
            specialty_required: request.specialty_required.clone(),
            appointment_type: request.appointment_type.to_string(),
            duration_minutes: request.duration_minutes,
            timezone: request.timezone.clone(),
        };

        let best_match = self.doctor_matching_service
            .find_best_doctor(matching_request, auth_token)
            .await
            .map_err(|e| match e {
                doctor_cell::models::DoctorError::NotAvailable => {
                    if let Some(specialty) = &request.specialty_required {
                        AppointmentError::SpecialtyNotAvailable { 
                            specialty: specialty.clone() 
                        }
                    } else {
                        AppointmentError::DoctorNotAvailable
                    }
                },
                _ => AppointmentError::DoctorMatchingError(e.to_string()),
            })?;

        match best_match {
            Some(doctor_match) => Ok(doctor_match.doctor.id),
            None => {
                if let Some(specialty) = &request.specialty_required {
                    Err(AppointmentError::SpecialtyNotAvailable { 
                        specialty: specialty.clone() 
                    })
                } else {
                    Err(AppointmentError::DoctorNotAvailable)
                }
            }
        }
    }

    /// ENHANCED: Validate specific doctor with specialty checking
    async fn validate_specific_doctor(
        &self,
        doctor_id: Uuid,
        request: &BookAppointmentRequest,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        debug!("Validating specific doctor: {}", doctor_id);

        let path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        if result.is_empty() {
            return Err(AppointmentError::DoctorNotFound);
        }

        let doctor_info = &result[0];
        
        // Check if doctor is available for appointments
        if !doctor_info["is_available"].as_bool().unwrap_or(false) {
            return Err(AppointmentError::DoctorNotAvailable);
        }

        // Check if doctor is verified
        if !doctor_info["is_verified"].as_bool().unwrap_or(false) {
            return Err(AppointmentError::DoctorNotAvailable);
        }

        // NEW: Validate specialty match if required
        if let Some(ref required_specialty) = request.specialty_required {
            let doctor_specialty = doctor_info["specialty"].as_str().unwrap_or("");
            if !doctor_specialty.to_lowercase().contains(&required_specialty.to_lowercase()) {
                return Err(AppointmentError::SpecialtyNotAvailable { 
                    specialty: required_specialty.clone() 
                });
            }
        }

        Ok(())
    }

    async fn validate_smart_booking_request(&self, request: &SmartBookingRequest) -> Result<(), AppointmentError> {
        let now = Utc::now();

        // Validate duration
        if request.duration_minutes < self.validation_rules.min_appointment_duration {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment duration must be at least {} minutes", 
                       self.validation_rules.min_appointment_duration)
            ));
        }

        if request.duration_minutes > self.validation_rules.max_appointment_duration {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment duration cannot exceed {} minutes", 
                       self.validation_rules.max_appointment_duration)
            ));
        }

        // Validate preferred date if provided
        if let Some(preferred_date) = request.preferred_date {
            let min_advance = Duration::hours(self.validation_rules.min_advance_booking_hours as i64);
            let max_advance = Duration::days(self.validation_rules.max_advance_booking_days as i64);
            
            let preferred_datetime = preferred_date.and_time(
                request.preferred_time_start.unwrap_or(NaiveTime::from_hms_opt(9, 0, 0).unwrap())
            ).and_utc();

            if preferred_datetime <= now + min_advance {
                return Err(AppointmentError::InvalidTime(
                    format!("Appointment must be booked at least {} hours in advance", 
                           self.validation_rules.min_advance_booking_hours)
                ));
            }

            if preferred_datetime >= now + max_advance {
                return Err(AppointmentError::InvalidTime(
                    format!("Appointment cannot be booked more than {} days in advance", 
                           self.validation_rules.max_advance_booking_days)
                ));
            }
        }

        Ok(())
    }

    async fn validate_booking_request(&self, request: &BookAppointmentRequest) -> Result<(), AppointmentError> {
        let now = Utc::now();

        // Check minimum advance booking time
        let min_advance = Duration::hours(self.validation_rules.min_advance_booking_hours as i64);
        if request.appointment_date <= now + min_advance {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment must be booked at least {} hours in advance", 
                       self.validation_rules.min_advance_booking_hours)
            ));
        }

        // Check maximum advance booking time
        let max_advance = Duration::days(self.validation_rules.max_advance_booking_days as i64);
        if request.appointment_date >= now + max_advance {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment cannot be booked more than {} days in advance", 
                       self.validation_rules.max_advance_booking_days)
            ));
        }

        // Validate duration
        if request.duration_minutes < self.validation_rules.min_appointment_duration {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment duration must be at least {} minutes", 
                       self.validation_rules.min_appointment_duration)
            ));
        }

        if request.duration_minutes > self.validation_rules.max_appointment_duration {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment duration cannot exceed {} minutes", 
                       self.validation_rules.max_appointment_duration)
            ));
        }

        Ok(())
    }

    async fn verify_patient_exists(&self, patient_id: &Uuid, auth_token: &str) -> Result<(), AppointmentError> {
        let path = format!("/rest/v1/patients?id=eq.{}", patient_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        if result.is_empty() {
            return Err(AppointmentError::PatientNotFound);
        }

        Ok(())
    }

    async fn create_appointment_record(
        &self,
        doctor_id: Uuid,
        request: BookAppointmentRequest,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
        let end_time = request.appointment_date + Duration::minutes(request.duration_minutes as i64);
        let now = Utc::now();

        let appointment_data = json!({
            "patient_id": request.patient_id,
            "doctor_id": doctor_id,
            "appointment_date": request.appointment_date.to_rfc3339(),
            "status": AppointmentStatus::Pending.to_string(),
            "appointment_type": request.appointment_type.to_string(),
            "duration_minutes": request.duration_minutes,
            "timezone": request.timezone,
            "scheduled_start_time": request.appointment_date.to_rfc3339(),
            "scheduled_end_time": end_time.to_rfc3339(),
            "patient_notes": request.patient_notes,
            "prescription_issued": false,
            "medical_certificate_issued": false,
            "report_generated": false,
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            "/rest/v1/appointments",
            Some(auth_token),
            Some(appointment_data),
            Some(headers),
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        if result.is_empty() {
            return Err(AppointmentError::DatabaseError("Failed to create appointment".to_string()));
        }

        let appointment: Appointment = serde_json::from_value(result[0].clone())
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to parse created appointment: {}", e)))?;

        Ok(appointment)
    }

    async fn update_appointment_record(
        &self,
        current_appointment: &Appointment,
        request: UpdateAppointmentRequest,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
        let mut update_data = serde_json::Map::new();

        // Handle status changes
        if let Some(status) = request.status {
            update_data.insert("status".to_string(), json!(status.to_string()));
            
            // Set timing based on status
            match status {
                AppointmentStatus::InProgress => {
                    update_data.insert("actual_start_time".to_string(), json!(Utc::now().to_rfc3339()));
                },
                AppointmentStatus::Completed => {
                    if current_appointment.actual_start_time.is_some() {
                        update_data.insert("actual_end_time".to_string(), json!(Utc::now().to_rfc3339()));
                    }
                },
                _ => {}
            }
        }

        // Handle notes updates
        if let Some(doctor_notes) = request.doctor_notes {
            update_data.insert("doctor_notes".to_string(), json!(doctor_notes));
        }
        if let Some(patient_notes) = request.patient_notes {
            update_data.insert("patient_notes".to_string(), json!(patient_notes));
        }

        // Handle rescheduling
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

        let path = format!("/rest/v1/appointments?id=eq.{}", current_appointment.id);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::PATCH,
            &path,
            Some(auth_token),
            Some(Value::Object(update_data)),
            Some(headers),
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        if result.is_empty() {
            return Err(AppointmentError::DatabaseError("Failed to update appointment".to_string()));
        }

        let updated_appointment: Appointment = serde_json::from_value(result[0].clone())
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to parse updated appointment: {}", e)))?;

        Ok(updated_appointment)
    }

    fn validate_reschedule_timing(&self, appointment: &Appointment, new_time: DateTime<Utc>) -> Result<(), AppointmentError> {
        let now = Utc::now();
        let min_reschedule_notice = Duration::hours(self.validation_rules.allowed_reschedule_hours as i64);

        if appointment.scheduled_start_time <= now + min_reschedule_notice {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment can only be rescheduled at least {} hours in advance", 
                       self.validation_rules.allowed_reschedule_hours)
            ));
        }

        // Validate new time is in the future
        if new_time <= now {
            return Err(AppointmentError::InvalidTime("Rescheduled time must be in the future".to_string()));
        }

        Ok(())
    }

    fn validate_cancellation_timing(&self, appointment: &Appointment) -> Result<(), AppointmentError> {
        let now = Utc::now();
        let min_cancellation_notice = Duration::hours(self.validation_rules.allowed_cancellation_hours as i64);

        // Check if appointment can be cancelled
        match appointment.status {
            AppointmentStatus::Completed => {
                return Err(AppointmentError::InvalidStatusTransition(appointment.status.clone()));
            },
            AppointmentStatus::Cancelled => {
                return Err(AppointmentError::InvalidStatusTransition(appointment.status.clone()));
            },
            _ => {}
        }

        // Check timing for cancellation
        if appointment.scheduled_start_time <= now + min_cancellation_notice {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment can only be cancelled at least {} hours in advance", 
                       self.validation_rules.allowed_cancellation_hours)
            ));
        }

        Ok(())
    }

    async fn handle_post_booking_tasks(&self, appointment: &Appointment, _auth_token: &str) -> Result<(), AppointmentError> {
        // TODO: Implement post-booking tasks
        // - Send confirmation email/SMS to patient
        // - Send notification to doctor
        // - Create calendar events
        
        debug!("Post-booking tasks completed for appointment {}", appointment.id);
        Ok(())
    }

    async fn handle_post_cancellation_tasks(
        &self, 
        appointment: &Appointment, 
        request: &CancelAppointmentRequest,
        _auth_token: &str
    ) -> Result<(), AppointmentError> {
        // TODO: Implement post-cancellation tasks
        // - Send cancellation notifications
        // - Update calendar events
        // - Log cancellation reason for analytics
        
        debug!("Post-cancellation tasks completed for appointment {} (cancelled by {:?})", 
               appointment.id, request.cancelled_by);
        Ok(())
    }
}