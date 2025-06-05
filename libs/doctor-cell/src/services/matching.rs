// libs/doctor-cell/src/services/matching.rs
use anyhow::{Result, anyhow};
use chrono::{NaiveDate, NaiveTime};
use reqwest::Method;
use serde_json::{Value};
use tracing::{debug, info, warn, error};

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{
    Doctor, DoctorMatch, DoctorMatchingRequest, AvailableSlot,
    DoctorSearchFilters, AvailabilityQueryRequest, DoctorError
};
use crate::services::doctor::DoctorService;
use crate::services::availability::AvailabilityService;

pub struct DoctorMatchingService {
    supabase: SupabaseClient,
    doctor_service: DoctorService,
    availability_service: AvailabilityService,
}

impl DoctorMatchingService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
            doctor_service: DoctorService::new(config),
            availability_service: AvailabilityService::new(config),
        }
    }

    /// Find best matching doctors for a patient's requirements with history prioritization
    pub async fn find_matching_doctors(
        &self,
        request: DoctorMatchingRequest,
        auth_token: &str,
        max_results: Option<usize>,
    ) -> Result<Vec<DoctorMatch>, DoctorError> {
        debug!("Finding matching doctors for patient: {}", request.patient_id);

        // **CRITICAL: Validate specialty availability first**
        if let Some(ref required_specialty) = request.specialty_required {
            self.validate_specialty_availability(required_specialty, auth_token).await?;
        }

        // Get patient information and appointment history
        let patient_info = self.get_patient_info(&request.patient_id.to_string(), auth_token).await?;
        let patient_history = self.get_patient_appointment_history(&request.patient_id.to_string(), auth_token).await?;

        // Build search filters based on request
        let search_filters = DoctorSearchFilters {
            specialty: request.specialty_required.clone(),
            sub_specialty: None,
            min_experience: None,
            min_rating: Some(3.0),
            available_date: request.preferred_date,
            available_time_start: request.preferred_time_start,
            available_time_end: request.preferred_time_end,
            timezone: Some(request.timezone.clone()),
            appointment_type: Some(request.appointment_type.clone()),
            is_verified_only: Some(true),
        };

        // Search for potentially matching doctors
        let candidate_doctors = self.doctor_service.search_doctors(
            search_filters,
            auth_token,
            Some(50),
            None,
        ).await.map_err(|e| DoctorError::ValidationError(e.to_string()))?;

        if candidate_doctors.is_empty() {
            if let Some(ref specialty) = request.specialty_required {
                return Err(DoctorError::NotAvailable);
            }
        }

        debug!("Found {} candidate doctors", candidate_doctors.len());

        let mut doctor_matches = Vec::new();

        // Evaluate each candidate doctor with history prioritization
        for doctor in candidate_doctors {
            if let Ok(doctor_match) = self.evaluate_doctor_match_with_history(
                &doctor,
                &request,
                &patient_info,
                &patient_history,
                auth_token,
            ).await {
                doctor_matches.push(doctor_match);
            }
        }

        // **CRITICAL: Check if we have any matches with required specialty**
        if let Some(ref required_specialty) = request.specialty_required {
            let specialty_matches = doctor_matches.iter()
                .filter(|m| m.doctor.specialty.to_lowercase().contains(&required_specialty.to_lowercase()))
                .count();
            
            if specialty_matches == 0 {
                error!("No {} doctors available at this time", required_specialty);
                return Err(DoctorError::NotAvailable);
            }
        }

        // Sort by match score (highest first) - previously seen doctors will have highest scores
        doctor_matches.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap());

        // Apply limit
        if let Some(limit) = max_results {
            doctor_matches.truncate(limit);
        }

        let avg_score = if !doctor_matches.is_empty() {
            doctor_matches.iter().map(|m| m.match_score).sum::<f32>() / doctor_matches.len() as f32
        } else {
            0.0
        };

        info!("Found {} matching doctors with average score: {:.2}", 
              doctor_matches.len(), avg_score);

        Ok(doctor_matches)
    }

    /// Find the single best matching doctor with history prioritization
    pub async fn find_best_doctor(
        &self,
        request: DoctorMatchingRequest,
        auth_token: &str,
    ) -> Result<Option<DoctorMatch>, DoctorError> {
        let matches = self.find_matching_doctors(request, auth_token, Some(1)).await?;
        Ok(matches.into_iter().next())
    }

    /// Find doctors with theoretical availability at a specific time
    pub async fn find_theoretically_available_doctors(
        &self,
        date: NaiveDate,
        preferred_time_start: Option<NaiveTime>,
        preferred_time_end: Option<NaiveTime>,
        appointment_type: String,
        duration_minutes: i32,
        timezone: String,
        specialty_filter: Option<String>,
        auth_token: &str,
    ) -> Result<Vec<DoctorMatch>, DoctorError> {
        debug!("Finding theoretically available doctors for {} at time range {:?}-{:?}", 
               date, preferred_time_start, preferred_time_end);

        // **CRITICAL: Validate specialty if provided**
        if let Some(ref specialty) = specialty_filter {
            self.validate_specialty_availability(specialty, auth_token).await?;
        }

        let search_filters = DoctorSearchFilters {
            specialty: specialty_filter.clone(),
            sub_specialty: None,
            min_experience: None,
            min_rating: Some(3.0),
            available_date: Some(date),
            available_time_start: preferred_time_start,
            available_time_end: preferred_time_end,
            timezone: Some(timezone.clone()),
            appointment_type: Some(appointment_type.clone()),
            is_verified_only: Some(true),
        };

        let doctors = self.doctor_service.search_doctors(
            search_filters,
            auth_token,
            None,
            None,
        ).await.map_err(|e| DoctorError::ValidationError(e.to_string()))?;

        if doctors.is_empty() && specialty_filter.is_some() {
            return Err(DoctorError::NotAvailable);
        }

        let mut available_doctors = Vec::new();

        for doctor in doctors {
            let availability_query = AvailabilityQueryRequest {
                date,
                timezone: Some(timezone.clone()),
                appointment_type: Some(appointment_type.clone()),
                duration_minutes: Some(duration_minutes),
            };

            let theoretical_slots = self.availability_service.get_available_slots(
                &doctor.id.to_string(),
                availability_query,
                auth_token,
            ).await.map_err(|e| DoctorError::ValidationError(e.to_string()))?;

            let filtered_slots = if let (Some(start), Some(end)) = (preferred_time_start, preferred_time_end) {
                theoretical_slots.into_iter()
                    .filter(|slot| {
                        let slot_time = slot.start_time.time();
                        slot_time >= start && slot_time <= end
                    })
                    .collect()
            } else {
                theoretical_slots
            };

            if !filtered_slots.is_empty() {
                let match_score = self.calculate_availability_score(&doctor, &filtered_slots);
                
                available_doctors.push(DoctorMatch {
                    doctor,
                    available_slots: filtered_slots,
                    match_score,
                    match_reasons: vec!["Theoretically available at requested time (verify with appointment-cell)".to_string()],
                });
            }
        }

        available_doctors.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap());
        Ok(available_doctors)
    }

    /// Get recommended doctors based on patient history and preferences
    pub async fn get_recommended_doctors(
        &self,
        patient_id: &str,
        specialty: Option<String>,
        auth_token: &str,
        limit: Option<usize>,
    ) -> Result<Vec<DoctorMatch>, DoctorError> {
        debug!("Getting recommended doctors for patient: {}", patient_id);

        // **CRITICAL: Validate specialty if provided**
        if let Some(ref specialty_name) = specialty {
            self.validate_specialty_availability(specialty_name, auth_token).await?;
        }

        let patient_history = self.get_patient_appointment_history(patient_id, auth_token).await?;
        let patient_info = self.get_patient_info(patient_id, auth_token).await?;

        let search_filters = DoctorSearchFilters {
            specialty: specialty.clone(),
            sub_specialty: None,
            min_experience: Some(2),
            min_rating: Some(4.0),
            available_date: None,
            available_time_start: None,
            available_time_end: None,
            timezone: patient_info.get("timezone").and_then(|v| v.as_str()).map(String::from),
            appointment_type: None,
            is_verified_only: Some(true),
        };

        let candidate_doctors = self.doctor_service.search_doctors(
            search_filters,
            auth_token,
            Some(20),
            None,
        ).await.map_err(|e| DoctorError::ValidationError(e.to_string()))?;

        if candidate_doctors.is_empty() && specialty.is_some() {
            return Err(DoctorError::NotAvailable);
        }

        let mut recommendations = Vec::new();

        for doctor in candidate_doctors {
            let recommendation_score = self.calculate_recommendation_score_with_history(
                &doctor,
                &patient_history,
                &patient_info,
            );

            if recommendation_score > 0.5 {
                recommendations.push(DoctorMatch {
                    doctor,
                    available_slots: vec![],
                    match_score: recommendation_score,
                    match_reasons: self.generate_recommendation_reasons_with_history(&patient_history),
                });
            }
        }

        recommendations.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap());

        if let Some(limit_val) = limit {
            recommendations.truncate(limit_val);
        }

        Ok(recommendations)
    }

    // ==============================================================================
    // PRIVATE HELPER METHODS
    // ==============================================================================

    /// **NEW: Validate that doctors with the required specialty are available**
    async fn validate_specialty_availability(
        &self,
        required_specialty: &str,
        auth_token: &str,
    ) -> Result<(), DoctorError> {
        debug!("Validating specialty availability: {}", required_specialty);

        let specialty_check_filters = DoctorSearchFilters {
            specialty: Some(required_specialty.to_string()),
            sub_specialty: None,
            min_experience: None,
            min_rating: None,
            available_date: None,
            available_time_start: None,
            available_time_end: None,
            timezone: None,
            appointment_type: None,
            is_verified_only: Some(true),
        };

        let specialty_doctors = self.doctor_service.search_doctors(
            specialty_check_filters,
            auth_token,
            Some(1), // Just need to know if any exist
            None,
        ).await.map_err(|e| DoctorError::ValidationError(e.to_string()))?;

        if specialty_doctors.is_empty() {
            error!("No {} doctors available at this time", required_specialty);
            return Err(DoctorError::NotAvailable);
        }

        debug!("Specialty {} validation passed", required_specialty);
        Ok(())
    }

    /// **ENHANCED: Evaluate doctor match with patient history prioritization**
    async fn evaluate_doctor_match_with_history(
        &self,
        doctor: &Doctor,
        request: &DoctorMatchingRequest,
        patient_info: &Value,
        patient_history: &[Value],
        auth_token: &str,
    ) -> Result<DoctorMatch, DoctorError> {
        let theoretical_slots = if let Some(date) = request.preferred_date {
            let availability_query = AvailabilityQueryRequest {
                date,
                timezone: Some(request.timezone.clone()),
                appointment_type: Some(request.appointment_type.clone()),
                duration_minutes: Some(request.duration_minutes),
            };

            self.availability_service.get_available_slots(
                &doctor.id.to_string(),
                availability_query,
                auth_token,
            ).await.unwrap_or_default()
        } else {
            vec![]
        };

        let match_score = self.calculate_match_score_with_history(
            doctor, 
            request, 
            patient_info, 
            patient_history, 
            &theoretical_slots
        );
        
        let match_reasons = self.generate_match_reasons_with_history(
            doctor, 
            request, 
            patient_history, 
            &theoretical_slots
        );

        Ok(DoctorMatch {
            doctor: doctor.clone(),
            available_slots: theoretical_slots,
            match_score,
            match_reasons,
        })
    }

    /// **ENHANCED: Calculate match score with heavy history weighting**
    fn calculate_match_score_with_history(
        &self,
        doctor: &Doctor,
        request: &DoctorMatchingRequest,
        _patient_info: &Value,
        patient_history: &[Value],
        theoretical_slots: &[AvailableSlot],
    ) -> f32 {
        let mut score = 0.0;
        let mut max_score = 0.0;

        // **CRITICAL: Patient history match (50% weight - highest priority)**
        let history_weight = 0.5;
        let has_seen_doctor = patient_history.iter().any(|appointment| {
            appointment.get("doctor_id")
                .and_then(|id| id.as_str())
                .map(|id| id == doctor.id.to_string())
                .unwrap_or(false)
        });

        if has_seen_doctor {
            score += history_weight; // Full points for previous relationship
            debug!("Doctor {} has treated patient before - prioritizing", doctor.id);
        }
        max_score += history_weight;

        // Specialty match (25% weight)
        let specialty_weight = 0.25;
        if let Some(ref required_specialty) = request.specialty_required {
            if doctor.specialty.to_lowercase().contains(&required_specialty.to_lowercase()) {
                score += specialty_weight;
            }
        } else {
            score += specialty_weight * 0.8;
        }
        max_score += specialty_weight;

        // Theoretical availability match (15% weight)
        let availability_weight = 0.15;
        if !theoretical_slots.is_empty() {
            let availability_score = if let (Some(start), Some(end)) = 
                (request.preferred_time_start, request.preferred_time_end) {
                let matching_slots = theoretical_slots.iter()
                    .filter(|slot| {
                        let slot_time = slot.start_time.time();
                        slot_time >= start && slot_time <= end
                    })
                    .count();
                
                if matching_slots > 0 { 1.0 } else { 0.5 }
            } else {
                1.0
            };
            
            score += availability_weight * availability_score;
        }
        max_score += availability_weight;

        // Doctor rating (10% weight)
        let rating_weight = 0.1;
        let rating_score = (doctor.rating / 5.0).min(1.0);
        score += rating_weight * rating_score as f64;
        max_score += rating_weight;

        // Normalize and ensure previously seen doctors get high scores
        let normalized_score = if max_score > 0.0 {
            (score / max_score) as f32
        } else {
            0.0
        };

        // **BOOST: Ensure previously seen doctors get minimum 0.8 score**
        if has_seen_doctor {
            normalized_score.max(0.8)
        } else {
            normalized_score
        }
    }

    /// **ENHANCED: Generate match reasons with history context**
    fn generate_match_reasons_with_history(
        &self,
        doctor: &Doctor,
        request: &DoctorMatchingRequest,
        patient_history: &[Value],
        theoretical_slots: &[AvailableSlot],
    ) -> Vec<String> {
        let mut reasons = Vec::new();

        // **PRIORITY: Previous consultation history**
        let has_seen_doctor = patient_history.iter().any(|appointment| {
            appointment.get("doctor_id")
                .and_then(|id| id.as_str())
                .map(|id| id == doctor.id.to_string())
                .unwrap_or(false)
        });

        if has_seen_doctor {
            let consultation_count = patient_history.iter()
                .filter(|appointment| {
                    appointment.get("doctor_id")
                        .and_then(|id| id.as_str())
                        .map(|id| id == doctor.id.to_string())
                        .unwrap_or(false)
                })
                .count();
            
            reasons.push(format!("Previous patient - {} consultation(s) with this doctor", consultation_count));
        }

        // Specialty match
        if let Some(ref required_specialty) = request.specialty_required {
            if doctor.specialty.to_lowercase().contains(&required_specialty.to_lowercase()) {
                reasons.push(format!("Specializes in {}", required_specialty));
            }
        }

        // Theoretical availability
        if !theoretical_slots.is_empty() {
            if let Some(preferred_date) = request.preferred_date {
                reasons.push(format!("Theoretically available on {} (verify with appointment-cell)", preferred_date));
            } else {
                reasons.push("Has theoretical availability slots (verify with appointment-cell)".to_string());
            }
        }

        // Quality indicators
        if doctor.rating >= 4.0 {
            reasons.push(format!("Highly rated ({:.1}/5.0)", doctor.rating));
        }

        if let Some(years_exp) = doctor.years_experience {
            if years_exp >= 5 {
                reasons.push(format!("{} years of experience", years_exp));
            }
        }

        if doctor.is_verified {
            reasons.push("Verified doctor".to_string());
        }

        reasons
    }

    /// **ENHANCED: Get actual patient appointment history**
    async fn get_patient_appointment_history(
        &self,
        patient_id: &str,
        auth_token: &str,
    ) -> Result<Vec<Value>, DoctorError> {
        debug!("Retrieving appointment history for patient: {}", patient_id);
        
        let path = format!(
            "/rest/v1/appointments?patient_id=eq.{}&status=eq.completed&order=created_at.desc&limit=50", 
            patient_id
        );

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await.map_err(|e| {
            error!("Failed to retrieve patient history: {}", e);
            DoctorError::ValidationError(format!("Failed to retrieve patient history: {}", e))
        })?;

        debug!("Found {} completed appointments in patient history", result.len());
        Ok(result)
    }

    /// **ENHANCED: Calculate recommendation score with history weighting**
    fn calculate_recommendation_score_with_history(
        &self,
        doctor: &Doctor,
        appointment_history: &[Value],
        _patient_info: &Value,
    ) -> f32 {
        let mut score = 0.0;

        // **CRITICAL: Previous consultation bonus (huge weight)**
        let has_seen_doctor = appointment_history.iter().any(|appointment| {
            appointment.get("doctor_id")
                .and_then(|id| id.as_str())
                .map(|id| id == doctor.id.to_string())
                .unwrap_or(false)
        });

        if has_seen_doctor {
            score += 0.6; // Major boost for previous relationship
        }

        // Base quality scores
        score += (doctor.rating / 5.0) * 0.2;
        
        if let Some(years_exp) = doctor.years_experience {
            score += (years_exp as f32 / 20.0).min(0.1);
        }

        if doctor.is_verified {
            score += 0.05;
        }

        if doctor.total_consultations >= 50 {
            score += 0.05;
        }

        score.min(1.0)
    }

    /// **ENHANCED: Generate recommendation reasons with history context**
    fn generate_recommendation_reasons_with_history(&self, appointment_history: &[Value]) -> Vec<String> {
        let mut reasons = Vec::new();

        if !appointment_history.is_empty() {
            reasons.push("Based on your consultation history".to_string());
            reasons.push("Previously treated by this doctor".to_string());
            reasons.push("Continuity of care recommended".to_string());
        } else {
            reasons.push("Highly rated doctor".to_string());
            reasons.push("Verified and experienced".to_string());
            reasons.push("Strong professional credentials".to_string());
        }

        reasons
    }

    async fn get_patient_info(&self, patient_id: &str, auth_token: &str) -> Result<Value, DoctorError> {
        let path = format!("/rest/v1/patients?id=eq.{}", patient_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await.map_err(|e| DoctorError::ValidationError(e.to_string()))?;

        if result.is_empty() {
            return Err(DoctorError::NotFound);
        }

        Ok(result[0].clone())
    }

    fn calculate_availability_score(&self, doctor: &Doctor, theoretical_slots: &[AvailableSlot]) -> f32 {
        let mut score = 0.6;
        score += (theoretical_slots.len() as f32 * 0.05).min(0.2);
        score += (doctor.rating / 5.0) * 0.15;
        
        if let Some(years_exp) = doctor.years_experience {
            score += (years_exp as f32 / 20.0).min(0.05);
        }

        score.min(1.0)
    }
}