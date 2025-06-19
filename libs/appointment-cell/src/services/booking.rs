// libs/appointment-cell/src/services/booking.rs
use anyhow::Result;
use chrono::{DateTime, Utc, Duration as ChronoDuration, NaiveTime, Timelike};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, info, warn};
use uuid::Uuid;
use std::sync::Arc;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;
use doctor_cell::services::matching::DoctorMatchingService;
use doctor_cell::models::{
    DoctorMatchingRequest, DoctorMatch,
    MedicalSchedulingConfig, SlotPriority
};

use crate::models::{
    Appointment, AppointmentStatus, AppointmentType, BookAppointmentRequest,
    UpdateAppointmentRequest, RescheduleAppointmentRequest, CancelAppointmentRequest,
    AppointmentSearchQuery, AppointmentStats, AppointmentError,
    AppointmentValidationRules, SmartBookingRequest, SmartBookingResponse,
    AlternativeSlot
};
use crate::services::conflict::ConflictDetectionService;
use crate::services::lifecycle::AppointmentLifecycleService;
use crate::services::telemedicine::TelemedicineService;

pub struct AppointmentBookingService {
    supabase: Arc<SupabaseClient>,
    conflict_service: ConflictDetectionService,
    lifecycle_service: AppointmentLifecycleService,
    doctor_matching_service: DoctorMatchingService,
    telemedicine_service: TelemedicineService,
    validation_rules: AppointmentValidationRules,
    medical_config: MedicalSchedulingConfig,
}

impl AppointmentBookingService {
    pub fn new(config: &AppConfig) -> Self {
        let supabase = Arc::new(SupabaseClient::new(config));
        let medical_config = MedicalSchedulingConfig::default();

        let conflict_service = ConflictDetectionService::with_config(
            Arc::clone(&supabase),
            medical_config.default_buffer_minutes,
            true, // Enable concurrent appointments
        );
        let lifecycle_service = AppointmentLifecycleService::new();
        let doctor_matching_service = DoctorMatchingService::new(config);
        let telemedicine_service = TelemedicineService::new(Arc::clone(&supabase));

        Self {
            conflict_service,
            lifecycle_service,
            doctor_matching_service,
            telemedicine_service,
            supabase,
            validation_rules: AppointmentValidationRules::default(),
            medical_config,
        }
    }

    pub fn with_medical_config(config: &AppConfig, medical_config: MedicalSchedulingConfig) -> Self {
        let supabase = Arc::new(SupabaseClient::new(config));

        let conflict_service = ConflictDetectionService::with_config(
            Arc::clone(&supabase),
            medical_config.default_buffer_minutes,
            true,
        );
        let lifecycle_service = AppointmentLifecycleService::new();
        let doctor_matching_service = DoctorMatchingService::new(config);
        let telemedicine_service = TelemedicineService::new(Arc::clone(&supabase));

        Self {
            conflict_service,
            lifecycle_service,
            doctor_matching_service,
            telemedicine_service,
            supabase,
            validation_rules: AppointmentValidationRules::default(),
            medical_config,
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
        // FIX: Clone specialty_required to avoid partial move
        let specialty_required_clone = request.specialty_required.clone();
        let booking_request = BookAppointmentRequest {
            patient_id: request.patient_id,
            doctor_id: Some(doctor_match.doctor.id),
            appointment_date: selected_slot.start_time,
            appointment_type: request.appointment_type.clone(),
            duration_minutes: request.duration_minutes,
            timezone: request.timezone.clone(),
            patient_notes: request.patient_notes.clone(),
            preferred_language: None,
            specialty_required: specialty_required_clone,
        };
        
        // **Step 5: Book the Appointment**
        let appointment = self.book_appointment(booking_request, auth_token).await?;
        
        // **Step 6: Generate Prioritized Alternative Slots**
        let alternative_slots = self.generate_prioritized_alternative_slots(
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

        // **Step 1: Enhanced Medical Validation**
        self.validate_enhanced_booking_request(&request, auth_token).await?;
        
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
        
        // **Step 4: Enhanced Conflict Detection with Buffer Times**
        let (duration_minutes, buffer_minutes) = self.get_appointment_timing(&request.appointment_type);
        let actual_duration = if request.duration_minutes > 0 {
            request.duration_minutes
        } else {
            duration_minutes
        };
        
        let end_time = request.appointment_date + ChronoDuration::minutes(actual_duration as i64);
        
        // Use enhanced conflict detection with appointment type awareness
        let conflict_check = self.conflict_service.check_conflicts_with_details(
            selected_doctor_id,
            request.appointment_date,
            end_time,
            None,
            Some(request.appointment_type.clone()),
            Some(buffer_minutes),
            auth_token,
        ).await?;

        if conflict_check.has_conflict {
            warn!("Enhanced appointment conflict detected for doctor {} at {} for type {:?}", 
                  selected_doctor_id, request.appointment_date, request.appointment_type);
            
            // Check if it's a concurrent appointment capacity issue
            if self.supports_concurrent_appointments(&request.appointment_type) {
                let has_capacity = self.conflict_service.check_concurrent_capacity(
                    selected_doctor_id,
                    request.appointment_date,
                    end_time,
                    &request.appointment_type,
                    None,
                    auth_token,
                ).await?;
                
                if !has_capacity {
                    info!("Concurrent appointment capacity exceeded for doctor {}", selected_doctor_id);
                }
            }
            
            return Err(AppointmentError::ConflictDetected);
        }

        // **Step 5: Create Enhanced Appointment Record**
        let mut enhanced_request = request;
        enhanced_request.duration_minutes = actual_duration;
        
        let appointment = self.create_enhanced_appointment_record(
            selected_doctor_id,
            enhanced_request,
            buffer_minutes,
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
            let new_end_time = new_start_time + ChronoDuration::minutes(new_duration as i64);

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
            // Use URL-encoded RFC3339 format for Supabase
            let date_str = from_date.to_rfc3339();
            let encoded_date = urlencoding::encode(&date_str);
            query_parts.push(format!("scheduled_start_time=gte.{}", encoded_date));
        }
        if let Some(to_date) = query.to_date {
            let date_str = to_date.to_rfc3339();
            let encoded_date = urlencoding::encode(&date_str);
            query_parts.push(format!("scheduled_start_time=lte.{}", encoded_date));
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

    /// Get upcoming appointments (configurable hours ahead)
    pub async fn get_upcoming_appointments(
        &self,
        patient_id: Option<Uuid>,
        doctor_id: Option<Uuid>,
        hours_ahead: Option<i32>,
        auth_token: &str,
    ) -> Result<Vec<Appointment>, AppointmentError> {
        let now = Utc::now();
        // Round to nearest second to avoid nanosecond precision issues with PostgreSQL
        let rounded_now = now.with_nanosecond(0).unwrap_or(now);
        let future_time = rounded_now + ChronoDuration::hours(hours_ahead.unwrap_or(24) as i64);

        let query = AppointmentSearchQuery {
            patient_id,
            doctor_id,
            status: None,
            appointment_type: None,
            from_date: Some(rounded_now),
            to_date: Some(future_time),
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

    /// Get appointment statistics with doctor continuity metrics (Production-hardened)
    pub async fn get_appointment_stats(
        &self,
        patient_id: Option<Uuid>,
        doctor_id: Option<Uuid>,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        auth_token: &str,
    ) -> Result<AppointmentStats, AppointmentError> {
        debug!("Calculating appointment statistics with fallback logic");

        // First try with a simplified query to avoid JSON operator issues
        let appointments = match self.get_simplified_appointment_stats_data(
            patient_id, 
            doctor_id, 
            from_date, 
            to_date, 
            auth_token
        ).await {
            Ok(data) => data,
            Err(_) => {
                warn!("Simplified stats query failed, using fallback method");
                // Fallback: try without date filters
                let fallback_query = AppointmentSearchQuery {
                    patient_id,
                    doctor_id,
                    status: None,
                    appointment_type: None,
                    from_date: None,
                    to_date: None,
                    limit: Some(100), // Limit to prevent overwhelming response
                    offset: None,
                };
                self.search_appointments(fallback_query, auth_token).await.unwrap_or_default()
            }
        };

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

        // NEW: Calculate doctor continuity rate with fallback
        let doctor_continuity_rate = if let Some(patient_id) = patient_id {
            // Try to calculate continuity rate, but use fallback on error
            match self.calculate_doctor_continuity_rate_safe(patient_id, auth_token).await {
                Ok(rate) => rate,
                Err(_) => {
                    warn!("Doctor continuity calculation failed, using fallback");
                    0.0 // Safe fallback
                }
            }
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
            doctor_continuity_rate,
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

    /// Public method to start a telemedicine session
    pub async fn start_telemedicine_session(
        &self,
        appointment_id: Uuid,
        participant_type: crate::services::telemedicine::ParticipantType,
        auth_token: &str,
    ) -> Result<crate::services::telemedicine::TelemedicineSessionInfo, AppointmentError> {
        self.telemedicine_service
            .start_telemedicine_appointment(appointment_id, participant_type, auth_token)
            .await
    }

    /// Public method to send pre-appointment telemedicine instructions
    pub async fn send_telemedicine_instructions(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let appointment = self.get_appointment(appointment_id, auth_token).await?;
        self.telemedicine_service
            .send_pre_appointment_instructions(&appointment, auth_token)
            .await
    }

    /// Public method to validate patient telemedicine readiness
    pub async fn check_telemedicine_readiness(
        &self,
        patient_id: Uuid,
        appointment_type: &AppointmentType,
        auth_token: &str,
    ) -> Result<crate::services::telemedicine::TelemedicineReadinessReport, AppointmentError> {
        self.telemedicine_service
            .validate_patient_telemedicine_readiness(patient_id, appointment_type, auth_token)
            .await
    }

    /// Get medical scheduling configuration
    pub fn get_medical_scheduling_config(&self) -> &MedicalSchedulingConfig {
        &self.medical_config
    }

    /// Get enhanced appointment timing with medical scheduling rules
    pub fn get_enhanced_appointment_timing(&self, appointment_type: &AppointmentType) -> (i32, i32, SlotPriority) {
        let (duration, buffer) = self.get_appointment_timing(appointment_type);
        let priority = match appointment_type {
            AppointmentType::Urgent => SlotPriority::Emergency,
            AppointmentType::MentalHealth => SlotPriority::Preferred,
            AppointmentType::WomensHealth => SlotPriority::Preferred,
            _ => SlotPriority::Available,
        };
        
        (duration, buffer, priority)
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
    /// FIX: Add explicit lifetime parameters to resolve lifetime conflict
    async fn select_optimal_slot<'a>(
        &self,
        doctor_match: &'a DoctorMatch,
        request: &SmartBookingRequest,
    ) -> Result<&'a doctor_cell::models::AvailableSlot, AppointmentError> {
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

    /// NEW: Generate prioritized alternative appointment slots
    async fn generate_prioritized_alternative_slots(
        &self,
        request: &SmartBookingRequest,
        exclude_doctor_id: &Uuid,
        auth_token: &str,
    ) -> Result<Vec<AlternativeSlot>, AppointmentError> {
        debug!("Generating prioritized alternative slots for patient {}", request.patient_id);

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
            .find_matching_doctors(matching_request, auth_token, Some(8))
            .await
            .map_err(|e| AppointmentError::DoctorMatchingError(e.to_string()))?;

        let mut alternatives = Vec::new();
        for doctor_match in matches {
            if doctor_match.doctor.id == *exclude_doctor_id {
                continue; // Skip the already selected doctor
            }

            for slot in doctor_match.available_slots.iter().take(3) { // Max 3 slots per doctor
                let has_history = doctor_match.match_reasons.iter()
                    .any(|reason| reason.contains("Previous patient"));

                // Calculate slot priority based on medical scheduling logic
                let slot_priority = self.calculate_slot_priority(
                    slot.start_time,
                    &request.appointment_type,
                    0.5, // Assume moderate availability density
                );

                let mut alternative = AlternativeSlot {
                    doctor_id: doctor_match.doctor.id,
                    doctor_first_name: doctor_match.doctor.first_name.clone(),
                    doctor_last_name: doctor_match.doctor.last_name.clone(),
                    start_time: slot.start_time,
                    end_time: slot.end_time,
                    match_score: doctor_match.match_score,
                    has_patient_history: has_history,
                };

                // Boost match score based on slot priority
                match slot_priority {
                    SlotPriority::Emergency => alternative.match_score += 0.3,
                    SlotPriority::Preferred => alternative.match_score += 0.2,
                    SlotPriority::Available => alternative.match_score += 0.1,
                    SlotPriority::Limited => alternative.match_score += 0.05,
                    SlotPriority::WaitList => alternative.match_score -= 0.1,
                }

                alternatives.push(alternative);
            }
        }

        // Enhanced sorting by medical scheduling priority
        alternatives.sort_by(|a, b| {
            // First priority: patient history
            match (a.has_patient_history, b.has_patient_history) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }
            
            // Second priority: match score (now includes slot priority)
            b.match_score.partial_cmp(&a.match_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        alternatives.truncate(12); // Limit to 12 prioritized alternatives

        info!("Generated {} prioritized alternative slots for patient {}", 
              alternatives.len(), request.patient_id);

        Ok(alternatives)
    }

    /// Legacy method for backward compatibility
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
                    doctor_first_name: doctor_match.doctor.first_name.clone(),
                    doctor_last_name: doctor_match.doctor.last_name.clone(),
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

    /// Production-hardened simplified appointment data retrieval
    async fn get_simplified_appointment_stats_data(
        &self,
        patient_id: Option<Uuid>,
        doctor_id: Option<Uuid>,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        auth_token: &str,
    ) -> Result<Vec<Appointment>, AppointmentError> {
        debug!("Attempting simplified appointment stats data retrieval");

        let query = AppointmentSearchQuery {
            patient_id,
            doctor_id,
            status: None,
            appointment_type: None,
            from_date,
            to_date,
            limit: Some(200), // Reasonable limit to prevent timeout
            offset: None,
        };

        self.search_appointments(query, auth_token).await
    }

    /// Safe doctor continuity rate calculation with error handling
    async fn calculate_doctor_continuity_rate_safe(
        &self,
        patient_id: Uuid,
        auth_token: &str,
    ) -> Result<f32, AppointmentError> {
        debug!("Calculating doctor continuity rate with safe fallback");
        
        // Use a simplified query to avoid potential JSON operator issues
        let query = AppointmentSearchQuery {
            patient_id: Some(patient_id),
            doctor_id: None,
            status: Some(AppointmentStatus::Completed),
            appointment_type: None,
            from_date: None,
            to_date: None,
            limit: Some(50), // Limit to most recent appointments
            offset: None,
        };

        let appointments = match self.search_appointments(query, auth_token).await {
            Ok(apts) => apts,
            Err(_) => {
                // Even more simplified fallback - just return default
                warn!("Simplified continuity query failed, returning default rate");
                return Ok(0.0);
            }
        };
        
        if appointments.len() < 2 {
            return Ok(0.0); // Need at least 2 appointments to calculate continuity
        }

        // Safe calculation without complex grouping that might trigger JSON operators
        let mut doctor_counts = std::collections::HashMap::new();
        for appointment in &appointments {
            *doctor_counts.entry(appointment.doctor_id).or_insert(0) += 1;
        }

        let total_appointments = appointments.len() as f32;
        let repeat_appointments = doctor_counts.values()
            .filter(|&&count| count > 1)
            .map(|&count| count - 1) // Only count repeats
            .sum::<i32>() as f32;

        let continuity_rate = if total_appointments > 0.0 {
            repeat_appointments / total_appointments
        } else {
            0.0
        };

        debug!("Calculated safe continuity rate: {:.2}", continuity_rate);
        Ok(continuity_rate.min(1.0)) // Cap at 100%
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
            preferred_time_end: Some((request.appointment_date + ChronoDuration::hours(2)).time()),
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
            let min_advance = ChronoDuration::hours(self.validation_rules.min_advance_booking_hours as i64);
            let max_advance = ChronoDuration::days(self.validation_rules.max_advance_booking_days as i64);
            
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
        let min_advance = ChronoDuration::hours(self.validation_rules.min_advance_booking_hours as i64);
        if request.appointment_date <= now + min_advance {
            return Err(AppointmentError::InvalidTime(
                format!("Appointment must be booked at least {} hours in advance", 
                       self.validation_rules.min_advance_booking_hours)
            ));
        }

        // Check maximum advance booking time
        let max_advance = ChronoDuration::days(self.validation_rules.max_advance_booking_days as i64);
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
        let (_, buffer_minutes) = self.get_appointment_timing(&request.appointment_type);
        self.create_enhanced_appointment_record(doctor_id, request, buffer_minutes, auth_token).await
    }

    async fn create_enhanced_appointment_record(
        &self,
        doctor_id: Uuid,
        request: BookAppointmentRequest,
        buffer_minutes: i32,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
        let end_time = request.appointment_date + ChronoDuration::minutes(request.duration_minutes as i64);
        let now = Utc::now();

        // Generate video conference link for telemedicine appointments
        let video_conference_link = self.telemedicine_service
            .generate_video_conference_link(
                Uuid::new_v4(), // Temporary ID, will be replaced with actual appointment ID
                &request.appointment_type, 
                request.duration_minutes
            ).await
            .map_err(|e| AppointmentError::ExternalServiceError(e.to_string()))?;

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
            "video_conference_link": video_conference_link,
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

        info!("Enhanced appointment created: {} with type {:?}, duration {} min, buffer {} min", 
              appointment.id, request.appointment_type, request.duration_minutes, buffer_minutes);

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
            let new_end_time = new_start_time + ChronoDuration::minutes(duration as i64);
            
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
        let min_reschedule_notice = ChronoDuration::hours(self.validation_rules.allowed_reschedule_hours as i64);

        if appointment.scheduled_start_time() <= now + min_reschedule_notice {
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
        let min_cancellation_notice = ChronoDuration::hours(self.validation_rules.allowed_cancellation_hours as i64);

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
        if appointment.scheduled_start_time() <= now + min_cancellation_notice {
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

    // ==============================================================================
    // ENHANCED MEDICAL SCHEDULING HELPER METHODS
    // ==============================================================================

    /// Get appointment timing parameters based on type
    fn get_appointment_timing(&self, appointment_type: &AppointmentType) -> (i32, i32) {
        match appointment_type {
            AppointmentType::InitialConsultation => (30, 10),
            AppointmentType::FollowUpConsultation => (20, 10),
            AppointmentType::EmergencyConsultation => (30, 5),
            AppointmentType::PrescriptionRenewal => (15, 5),
            AppointmentType::SpecialtyConsultation => (45, 15),
            AppointmentType::GroupSession => (60, 10),
            AppointmentType::TelehealthCheckIn => (15, 5),
            // Legacy support
            AppointmentType::GeneralConsultation => (30, 10),
            AppointmentType::FollowUp => (20, 10),
            AppointmentType::Prescription => (15, 5),
            AppointmentType::MedicalCertificate => (10, 5),
            AppointmentType::Urgent => (30, 5),
            AppointmentType::MentalHealth => (45, 15),
            AppointmentType::WomensHealth => (30, 10),
        }
    }

    /// Check if appointment type supports concurrent scheduling
    fn supports_concurrent_appointments(&self, appointment_type: &AppointmentType) -> bool {
        matches!(
            appointment_type,
            AppointmentType::InitialConsultation |
            AppointmentType::FollowUpConsultation |
            AppointmentType::SpecialtyConsultation |
            AppointmentType::GroupSession |
            // Legacy support
            AppointmentType::GeneralConsultation |
            AppointmentType::FollowUp |
            AppointmentType::MentalHealth
        )
    }


    /// Get appointment priority score for scheduling optimization
    fn get_appointment_priority(&self, appointment_type: &AppointmentType, has_patient_history: bool) -> i32 {
        let base_priority = match appointment_type {
            AppointmentType::EmergencyConsultation => 100,
            AppointmentType::SpecialtyConsultation => 80,
            AppointmentType::InitialConsultation => 60,
            AppointmentType::FollowUpConsultation => 50,
            AppointmentType::TelehealthCheckIn => 40,
            AppointmentType::PrescriptionRenewal => 20,
            AppointmentType::GroupSession => 30,
            // Legacy support
            AppointmentType::Urgent => 100,
            AppointmentType::MentalHealth => 80,
            AppointmentType::WomensHealth => 70,
            AppointmentType::GeneralConsultation => 60,
            AppointmentType::FollowUp => 50,
            AppointmentType::MedicalCertificate => 30,
            AppointmentType::Prescription => 20,
        };

        // Boost priority for patients with existing doctor relationship
        if has_patient_history {
            base_priority + 20
        } else {
            base_priority
        }
    }

    /// Validate telemedicine appointment requirements using telemedicine service
    async fn validate_telemedicine_requirements(
        &self,
        appointment_type: &AppointmentType,
        patient_id: &Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        // Only validate for telemedicine-capable appointments
        if !self.is_telemedicine_capable(appointment_type) {
            return Ok(());
        }

        // Use telemedicine service for comprehensive validation
        let readiness_report = self.telemedicine_service
            .validate_patient_telemedicine_readiness(*patient_id, appointment_type, auth_token)
            .await?;

        if !readiness_report.is_ready {
            let issues = readiness_report.recommendations.join("; ");
            return Err(AppointmentError::ValidationError(
                format!("Telemedicine readiness issues: {}", issues)
            ));
        }

        if readiness_report.technical_support_needed {
            info!("Patient {} will need technical support for telemedicine appointment", patient_id);
        }

        Ok(())
    }

    /// Check if appointment type is telemedicine capable
    fn is_telemedicine_capable(&self, appointment_type: &AppointmentType) -> bool {
        matches!(
            appointment_type,
            AppointmentType::GeneralConsultation |
            AppointmentType::FollowUp |
            AppointmentType::MentalHealth |
            AppointmentType::WomensHealth |
            AppointmentType::Prescription
        )
    }

    /// Calculate optimal slot priority for scheduling
    fn calculate_slot_priority(
        &self,
        slot_time: DateTime<Utc>,
        appointment_type: &AppointmentType,
        doctor_availability_density: f32,
    ) -> SlotPriority {
        let hour = slot_time.hour();
        
        // Emergency appointments get highest priority
        if matches!(appointment_type, AppointmentType::Urgent) {
            return SlotPriority::Emergency;
        }

        // Consider peak hours and availability density
        let is_peak_hour = (hour >= 9 && hour <= 11) || (hour >= 14 && hour <= 16);
        
        if doctor_availability_density > 0.9 {
            SlotPriority::WaitList
        } else if doctor_availability_density > 0.8 {
            SlotPriority::Limited
        } else if is_peak_hour && doctor_availability_density > 0.6 {
            SlotPriority::Limited
        } else if hour >= 9 && hour <= 17 {
            SlotPriority::Preferred
        } else {
            SlotPriority::Available
        }
    }

    /// Enhanced appointment validation with medical scheduling rules
    async fn validate_enhanced_booking_request(
        &self,
        request: &BookAppointmentRequest,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        // Standard validation
        self.validate_booking_request(request).await?;
        
        // Telemedicine validation
        self.validate_telemedicine_requirements(&request.appointment_type, &request.patient_id, auth_token).await?;
        
        // Medical scheduling specific validation
        let (default_duration, _) = self.get_appointment_timing(&request.appointment_type);
        
        // Validate duration is appropriate for appointment type
        if request.duration_minutes > 0 && request.duration_minutes < default_duration / 2 {
            return Err(AppointmentError::ValidationError(
                format!("Duration too short for {:?} appointment type", request.appointment_type)
            ));
        }
        
        // Validate maximum duration limits
        let max_duration = match request.appointment_type {
            AppointmentType::MentalHealth => 90,
            AppointmentType::WomensHealth => 60,
            AppointmentType::GeneralConsultation => 60,
            _ => default_duration * 2,
        };
        
        if request.duration_minutes > max_duration {
            return Err(AppointmentError::ValidationError(
                format!("Duration exceeds maximum for {:?} appointment type", request.appointment_type)
            ));
        }
        
        Ok(())
    }
}