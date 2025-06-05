use anyhow::{Result, anyhow};
use chrono::{NaiveDate, NaiveTime};
use reqwest::Method;
use serde_json::{Value};
use tracing::{debug, info};

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{
    Doctor, DoctorMatch, DoctorMatchingRequest, AvailableSlot,
    DoctorSearchFilters, AvailabilityQueryRequest
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

    /// Find best matching doctors for a patient's requirements
    /// NOTE: This provides theoretical matches based on doctor availability schedules.
    /// The appointment-cell should verify actual availability against booked appointments.
    pub async fn find_matching_doctors(
        &self,
        request: DoctorMatchingRequest,
        auth_token: &str,
        max_results: Option<usize>,
    ) -> Result<Vec<DoctorMatch>> {
        debug!("Finding matching doctors for patient: {}", request.patient_id);

        // Get patient information to help with matching
        let patient_info = self.get_patient_info(&request.patient_id.to_string(), auth_token).await?;

        // Build search filters based on request
        let search_filters = DoctorSearchFilters {
            specialty: request.specialty_required.clone(),
            sub_specialty: None,
            min_experience: None,
            min_rating: Some(3.0), // Minimum acceptable rating
            available_date: request.preferred_date,
            available_time_start: request.preferred_time_start,
            available_time_end: request.preferred_time_end,
            timezone: Some(request.timezone.clone()),
            appointment_type: Some(request.appointment_type.clone()),
            is_verified_only: Some(true), // Only verified doctors
        };

        // Search for potentially matching doctors
        let candidate_doctors = self.doctor_service.search_doctors(
            search_filters,
            auth_token,
            Some(50), // Get more candidates for better matching
            None,
        ).await?;

        debug!("Found {} candidate doctors", candidate_doctors.len());

        let mut doctor_matches = Vec::new();

        // Evaluate each candidate doctor
        for doctor in candidate_doctors {
            if let Ok(doctor_match) = self.evaluate_doctor_match(
                &doctor,
                &request,
                &patient_info,
                auth_token,
            ).await {
                doctor_matches.push(doctor_match);
            }
        }

        // Sort by match score (highest first)
        doctor_matches.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap());

        // Apply limit
        if let Some(limit) = max_results {
            doctor_matches.truncate(limit);
        }

        info!("Found {} matching doctors with average score: {:.2}", 
              doctor_matches.len(),
              if !doctor_matches.is_empty() {
                  doctor_matches.iter().map(|m| m.match_score as f32).sum::<f32>() / doctor_matches.len() as f32
              } else {
                  0.0
              });

        Ok(doctor_matches)
    }

    /// Find the single best matching doctor
    pub async fn find_best_doctor(
        &self,
        request: DoctorMatchingRequest,
        auth_token: &str,
    ) -> Result<Option<DoctorMatch>> {
        let matches = self.find_matching_doctors(request, auth_token, Some(1)).await?;
        Ok(matches.into_iter().next())
    }

    /// Find doctors with theoretical availability at a specific time
    /// NOTE: This returns doctors based on their availability schedules.
    /// The appointment-cell should verify no conflicts with actual bookings.
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
    ) -> Result<Vec<DoctorMatch>> {
        debug!("Finding theoretically available doctors for {} at time range {:?}-{:?}", 
               date, preferred_time_start, preferred_time_end);

        // Get all active, verified doctors
        let search_filters = DoctorSearchFilters {
            specialty: specialty_filter,
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
        ).await?;

        let mut available_doctors = Vec::new();

        for doctor in doctors {
            // Get theoretical available slots for this doctor
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
            ).await?;

            // Filter slots by preferred time if specified
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
                // Calculate a simple availability score
                let match_score = self.calculate_availability_score(&doctor, &filtered_slots);
                
                available_doctors.push(DoctorMatch {
                    doctor,
                    available_slots: filtered_slots,
                    match_score,
                    match_reasons: vec!["Theoretically available at requested time (verify with appointment-cell)".to_string()],
                });
            }
        }

        // Sort by match score
        available_doctors.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap());

        Ok(available_doctors)
    }

    /// Get recommended doctors based on patient history and preferences
    /// NOTE: If the patient has already seen that doctor, they should be prioritized. || If they are available.
    pub async fn get_recommended_doctors(
        &self,
        patient_id: &str,
        specialty: Option<String>,
        auth_token: &str,
        limit: Option<usize>,
    ) -> Result<Vec<DoctorMatch>> {
        debug!("Getting recommended doctors for patient: {}", patient_id);

        // Get patient's appointment history (if accessible)
        let patient_history = self.get_patient_basic_history(patient_id, auth_token).await.unwrap_or_default();
        
        // Get patient info
        let patient_info = self.get_patient_info(patient_id, auth_token).await?;

        // Build recommendation filters
        let search_filters = DoctorSearchFilters {
            specialty: specialty.clone(),
            sub_specialty: None,
            min_experience: Some(2), // Prefer experienced doctors
            min_rating: Some(4.0), // Higher rating for recommendations
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
        ).await?;

        let mut recommendations = Vec::new();

        for doctor in candidate_doctors {
            let recommendation_score = self.calculate_recommendation_score(
                &doctor,
                &patient_history,
                &patient_info,
            );

            if recommendation_score > 0.5 { // Only include good recommendations
                recommendations.push(DoctorMatch {
                    doctor,
                    available_slots: vec![], // Will be filled when needed by appointment-cell
                    match_score: recommendation_score,
                    match_reasons: self.generate_recommendation_reasons(&patient_history),
                });
            }
        }

        // Sort by recommendation score
        recommendations.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap());

        if let Some(limit_val) = limit {
            recommendations.truncate(limit_val);
        }

        Ok(recommendations)
    }

    // Private helper methods

    async fn evaluate_doctor_match(
        &self,
        doctor: &Doctor,
        request: &DoctorMatchingRequest,
        patient_info: &Value,
        auth_token: &str,
    ) -> Result<DoctorMatch> {
        // Get theoretical available slots for the doctor
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

        // Calculate match score
        let match_score = self.calculate_match_score(doctor, request, patient_info, &theoretical_slots);
        
        // Generate match reasons
        let match_reasons = self.generate_match_reasons(doctor, request, &theoretical_slots);

        Ok(DoctorMatch {
            doctor: doctor.clone(),
            available_slots: theoretical_slots,
            match_score,
            match_reasons,
        })
    }

    // TODO's
    // If theres no available specialty match, throw error (Not available)
    // Prioritize previous consultations with the doctor
    fn calculate_match_score(
        &self,
        doctor: &Doctor,
        request: &DoctorMatchingRequest,
        _patient_info: &Value,
        theoretical_slots: &[AvailableSlot],
    ) -> f32 {
        let mut score = 0.0;
        let mut max_score = 0.0;

        // Specialty match (40% weight - increased since no pricing)
        let specialty_weight = 0.4;
        if let Some(ref required_specialty) = request.specialty_required {
            if doctor.specialty.to_lowercase().contains(&required_specialty.to_lowercase()) {
                score += specialty_weight;
            }
        } else {
            score += specialty_weight * 0.8; // Some points for no specific requirement
        }
        max_score += specialty_weight;

        // Theoretical availability match (30% weight)
        let availability_weight = 0.3;
        if !theoretical_slots.is_empty() {
            let availability_score = if let (Some(start), Some(end)) = 
                (request.preferred_time_start, request.preferred_time_end) {
                // Check if any slots match preferred time
                let matching_slots = theoretical_slots.iter()
                    .filter(|slot| {
                        let slot_time = slot.start_time.time();
                        slot_time >= start && slot_time <= end
                    })
                    .count();
                
                if matching_slots > 0 {
                    1.0
                } else {
                    0.5 // Has theoretical availability but not at preferred time
                }
            } else {
                1.0 // Any theoretical availability is good if no preference
            };
            
            score += availability_weight * availability_score;
        }
        max_score += availability_weight;

        // Doctor rating (20% weight)
        let rating_weight = 0.2;
        let rating_score = (doctor.rating / 5.0).min(1.0);
        score += rating_weight * rating_score as f64;
        max_score += rating_weight;

        // Experience (10% weight)
        let experience_weight = 0.1;
        if let Some(years_exp) = doctor.years_experience {
            let experience_score = (years_exp as f32 / 20.0).min(1.0); // Max out at 20 years
            score += experience_weight * experience_score as f64;
        }
        max_score += experience_weight;

        // Normalize score to 0-1 range
        if max_score > 0.0 {
            (score / max_score) as f32
        } else {
            0.0
        }
    }

    fn generate_match_reasons(
        &self,
        doctor: &Doctor,
        request: &DoctorMatchingRequest,
        theoretical_slots: &[AvailableSlot],
    ) -> Vec<String> {
        let mut reasons = Vec::new();

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

        // Rating
        if doctor.rating >= 4.0 {
            reasons.push(format!("Highly rated ({:.1}/5.0)", doctor.rating));
        }

        // Experience
        if let Some(years_exp) = doctor.years_experience {
            if years_exp >= 5 {
                reasons.push(format!("{} years of experience", years_exp));
            }
        }

        // Verification
        if doctor.is_verified {
            reasons.push("Verified doctor".to_string());
        }

        reasons
    }

    fn calculate_availability_score(&self, doctor: &Doctor, theoretical_slots: &[AvailableSlot]) -> f32 {
        let mut score = 0.6; // Base score for having theoretical availability

        // More slots = higher score
        score += (theoretical_slots.len() as f32 * 0.05).min(0.2);

        // Doctor rating contributes
        score += (doctor.rating / 5.0) * 0.15;

        // Experience contributes
        if let Some(years_exp) = doctor.years_experience {
            score += (years_exp as f32 / 20.0).min(0.05);
        }

        score.min(1.0)
    }

    async fn get_patient_info(&self, patient_id: &str, auth_token: &str) -> Result<Value> {
        let path = format!("/rest/v1/patients?id=eq.{}", patient_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Patient not found"));
        }

        Ok(result[0].clone())
    }

    async fn get_patient_basic_history(
        &self,
        patient_id: &str,
        auth_token: &str,
    ) -> Result<Vec<Value>> {
        // This is a simplified version that doesn't rely on appointment-cell data
        // In a real implementation, this might call the appointment-cell API
        // or use a shared patient history service
        
        debug!("Getting basic patient history for: {}", patient_id);
        
        // For now, return empty - the appointment-cell would provide this data
        Ok(vec![])
    }

    fn calculate_recommendation_score(
        &self,
        doctor: &Doctor,
        appointment_history: &[Value],
        _patient_info: &Value,
    ) -> f32 {
        let mut score = 0.0;

        // Base score from doctor rating and experience
        score += (doctor.rating / 5.0) * 0.4;
        
        if let Some(years_exp) = doctor.years_experience {
            score += (years_exp as f32 / 20.0).min(0.2);
        }

        // Bonus if doctor has worked with similar patients (if history available)
        if !appointment_history.is_empty() {
            let specialty_matches = appointment_history.iter()
                .filter(|apt| {
                    apt.get("doctor_specialty")
                        .and_then(|s| s.as_str())
                        .map(|s| s == doctor.specialty)
                        .unwrap_or(false)
                })
                .count();

            if specialty_matches > 0 {
                score += 0.2;
            }
        }

        // Verification bonus
        if doctor.is_verified {
            score += 0.1;
        }

        // High consultation count indicates popularity
        if doctor.total_consultations >= 50 {
            score += 0.1;
        }

        // Strong experience bonus (since no cost factors)
        if let Some(years_exp) = doctor.years_experience {
            if years_exp >= 10 {
                score += 0.1;
            }
        }

        score.min(1.0)
    }

    fn generate_recommendation_reasons(&self, appointment_history: &[Value]) -> Vec<String> {
        let mut reasons = Vec::new();

        if appointment_history.is_empty() {
            reasons.push("Highly rated doctor".to_string());
            reasons.push("Verified and experienced".to_string());
            reasons.push("Strong professional credentials".to_string());
        } else {
            reasons.push("Based on available patient history".to_string());
            reasons.push("Matches previous preferences".to_string());
        }

        reasons
    }
}