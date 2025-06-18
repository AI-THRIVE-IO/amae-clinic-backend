// libs/appointment-cell/src/services/telemedicine.rs
use anyhow::Result;
use chrono::{DateTime, Utc, Duration};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, info};
use uuid::Uuid;
use std::sync::Arc;

use shared_database::supabase::SupabaseClient;
use crate::models::{
    Appointment, AppointmentType, AppointmentError
};

pub struct TelemedicineService {
    supabase: Arc<SupabaseClient>,
    video_service_base_url: String,
    enable_pre_appointment_checks: bool,
}

impl TelemedicineService {
    pub fn new(supabase: Arc<SupabaseClient>) -> Self {
        Self {
            supabase,
            video_service_base_url: "https://amae-clinic.video".to_string(),
            enable_pre_appointment_checks: true,
        }
    }

    pub fn with_config(
        supabase: Arc<SupabaseClient>, 
        video_service_url: String,
        enable_checks: bool
    ) -> Self {
        Self {
            supabase,
            video_service_base_url: video_service_url,
            enable_pre_appointment_checks: enable_checks,
        }
    }

    /// Generate video conference link for telemedicine appointment
    pub async fn generate_video_conference_link(
        &self,
        appointment_id: Uuid,
        appointment_type: &AppointmentType,
        duration_minutes: i32,
    ) -> Result<Option<String>, AppointmentError> {
        debug!("Generating video conference link for appointment {}", appointment_id);

        if !self.is_telemedicine_capable(appointment_type) {
            return Ok(None);
        }

        // In production, this would integrate with video conferencing service (Zoom, WebRTC, etc.)
        let conference_id = Uuid::new_v4();
        let room_name = format!("appointment-{}", appointment_id);
        
        let video_link = format!("{}/room/{}?duration={}&type={}", 
                                self.video_service_base_url,
                                conference_id,
                                duration_minutes,
                                appointment_type.to_string());

        info!("Video conference link generated for appointment {}: {}", appointment_id, video_link);

        // Store video conference details
        self.store_video_conference_details(appointment_id, &video_link, &room_name).await?;

        Ok(Some(video_link))
    }

    /// Validate patient telemedicine readiness
    pub async fn validate_patient_telemedicine_readiness(
        &self,
        patient_id: Uuid,
        appointment_type: &AppointmentType,
        auth_token: &str,
    ) -> Result<TelemedicineReadinessReport, AppointmentError> {
        debug!("Validating telemedicine readiness for patient {}", patient_id);

        if !self.enable_pre_appointment_checks {
            return Ok(TelemedicineReadinessReport::default_ready());
        }

        // Get patient telemedicine profile
        let patient_profile = self.get_patient_telemedicine_profile(patient_id, auth_token).await?;

        let mut readiness_report = TelemedicineReadinessReport {
            is_ready: true,
            consent_provided: patient_profile.telemedicine_consent,
            device_compatible: patient_profile.device_compatibility_verified,
            network_adequate: patient_profile.network_speed_adequate,
            privacy_setup: patient_profile.privacy_environment_confirmed,
            technical_support_needed: false,
            recommendations: Vec::new(),
        };

        // Validate consent
        if !readiness_report.consent_provided {
            readiness_report.is_ready = false;
            readiness_report.recommendations.push(
                "Please complete telemedicine consent form before your appointment".to_string()
            );
        }

        // Check device compatibility
        if !readiness_report.device_compatible {
            readiness_report.is_ready = false;
            readiness_report.technical_support_needed = true;
            readiness_report.recommendations.push(
                "Please test your device compatibility using our system check tool".to_string()
            );
        }

        // Validate network connectivity
        if !readiness_report.network_adequate {
            readiness_report.recommendations.push(
                "For best experience, ensure stable internet connection (minimum 1 Mbps upload/download)".to_string()
            );
        }

        // Privacy environment check
        if !readiness_report.privacy_setup {
            readiness_report.recommendations.push(
                "Please ensure you have a private, quiet space for your consultation".to_string()
            );
        }

        // Type-specific recommendations
        match appointment_type {
            AppointmentType::MentalHealth => {
                readiness_report.recommendations.push(
                    "Mental health consultations require extra privacy and uninterrupted time".to_string()
                );
            },
            AppointmentType::WomensHealth => {
                readiness_report.recommendations.push(
                    "Please ensure you have adequate lighting and privacy for your consultation".to_string()
                );
            },
            _ => {}
        }

        info!("Telemedicine readiness validated for patient {}: ready={}", 
              patient_id, readiness_report.is_ready);

        Ok(readiness_report)
    }

    /// Send pre-appointment telemedicine instructions
    pub async fn send_pre_appointment_instructions(
        &self,
        appointment: &Appointment,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        debug!("Sending pre-appointment telemedicine instructions for appointment {}", appointment.id);

        if !self.is_telemedicine_capable(&appointment.appointment_type) {
            return Ok(()); // No instructions needed for in-person appointments
        }

        let readiness_report = self.validate_patient_telemedicine_readiness(
            appointment.patient_id,
            &appointment.appointment_type,
            auth_token,
        ).await?;

        // Generate personalized instructions
        let instructions = self.generate_personalized_instructions(&appointment, &readiness_report);

        // In production, this would send email/SMS with instructions
        self.store_telemedicine_instructions(appointment.id, &instructions).await?;

        info!("Pre-appointment telemedicine instructions sent for appointment {}", appointment.id);

        Ok(())
    }

    /// Handle telemedicine appointment start
    pub async fn start_telemedicine_appointment(
        &self,
        appointment_id: Uuid,
        participant_type: ParticipantType,
        auth_token: &str,
    ) -> Result<TelemedicineSessionInfo, AppointmentError> {
        debug!("Starting telemedicine appointment {} for {:?}", appointment_id, participant_type);

        // Get appointment details
        let appointment = self.get_appointment_details(appointment_id, auth_token).await?;

        if appointment.video_conference_link.is_none() {
            return Err(AppointmentError::ValidationError(
                "No video conference link available for this appointment".to_string()
            ));
        }

        // Validate appointment timing
        let now = Utc::now();
        let can_start = self.can_start_telemedicine_session(&appointment, now);

        if !can_start {
            return Err(AppointmentError::InvalidTime(
                "Appointment cannot be started at this time".to_string()
            ));
        }

        // Generate session token and room access
        let room_url = appointment.video_conference_link.clone().unwrap();
        let session_token = self.generate_session_token(appointment_id, participant_type).await?;
        let wait_time = self.calculate_wait_time(&appointment, participant_type).await;

        let session_info = TelemedicineSessionInfo {
            room_url,
            session_token,
            participant_type,
            appointment_id,
            estimated_wait_time_minutes: wait_time,
            technical_support_available: true,
        };

        // Log session start
        self.log_session_event(appointment_id, participant_type, "session_started", auth_token).await?;

        info!("Telemedicine session started for appointment {} by {:?}", appointment_id, participant_type);

        Ok(session_info)
    }

    /// Handle telemedicine appointment completion
    pub async fn complete_telemedicine_appointment(
        &self,
        appointment_id: Uuid,
        session_duration_minutes: i32,
        quality_metrics: Option<SessionQualityMetrics>,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        debug!("Completing telemedicine appointment {}", appointment_id);

        // Store session metrics
        if let Some(metrics) = quality_metrics {
            self.store_session_quality_metrics(appointment_id, &metrics, auth_token).await?;
        }

        // Log session completion
        self.log_session_event(appointment_id, ParticipantType::System, "session_completed", auth_token).await?;

        // Generate session summary
        self.generate_session_summary(appointment_id, session_duration_minutes, auth_token).await?;

        info!("Telemedicine appointment {} completed successfully", appointment_id);

        Ok(())
    }

    // ==============================================================================
    // PRIVATE HELPER METHODS
    // ==============================================================================

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

    async fn get_patient_telemedicine_profile(
        &self,
        patient_id: Uuid,
        auth_token: &str,
    ) -> Result<PatientTelemedicineProfile, AppointmentError> {
        let path = format!("/rest/v1/patient_telemedicine_profiles?patient_id=eq.{}", patient_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        if result.is_empty() {
            // Return default profile if none exists
            return Ok(PatientTelemedicineProfile::default());
        }

        let profile: PatientTelemedicineProfile = serde_json::from_value(result[0].clone())
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to parse telemedicine profile: {}", e)))?;

        Ok(profile)
    }

    async fn store_video_conference_details(
        &self,
        appointment_id: Uuid,
        video_link: &str,
        room_name: &str,
    ) -> Result<(), AppointmentError> {
        let conference_data = json!({
            "appointment_id": appointment_id,
            "video_link": video_link,
            "room_name": room_name,
            "created_at": Utc::now().to_rfc3339(),
            "status": "active"
        });

        let _result: Vec<Value> = self.supabase.request(
            Method::POST,
            "/rest/v1/telemedicine_sessions",
            None,
            Some(conference_data),
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    fn generate_personalized_instructions(
        &self,
        appointment: &Appointment,
        readiness_report: &TelemedicineReadinessReport,
    ) -> Vec<String> {
        let mut instructions = vec![
            format!("Your telemedicine appointment is scheduled for {}", 
                   appointment.scheduled_start_time().format("%Y-%m-%d at %H:%M UTC")),
            "Please join the video call 5 minutes before your scheduled time".to_string(),
        ];

        // Add readiness-specific instructions
        instructions.extend(readiness_report.recommendations.clone());

        // Type-specific instructions
        match appointment.appointment_type {
            AppointmentType::MentalHealth => {
                instructions.push("Ensure you have a comfortable, private space for your mental health consultation".to_string());
            },
            AppointmentType::WomensHealth => {
                instructions.push("Please have any relevant documents or test results ready to share".to_string());
            },
            _ => {}
        }

        instructions.push("Technical support is available during your appointment if needed".to_string());

        instructions
    }

    async fn store_telemedicine_instructions(
        &self,
        appointment_id: Uuid,
        instructions: &[String],
    ) -> Result<(), AppointmentError> {
        let instructions_data = json!({
            "appointment_id": appointment_id,
            "instructions": instructions,
            "sent_at": Utc::now().to_rfc3339()
        });

        let _result: Vec<Value> = self.supabase.request(
            Method::POST,
            "/rest/v1/telemedicine_instructions",
            None,
            Some(instructions_data),
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn get_appointment_details(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<Appointment, AppointmentError> {
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

    fn can_start_telemedicine_session(&self, appointment: &Appointment, current_time: DateTime<Utc>) -> bool {
        let early_start_buffer = Duration::minutes(10); // Can start 10 minutes early
        let late_start_limit = Duration::minutes(15);   // Can start up to 15 minutes late

        let earliest_start = appointment.scheduled_start_time() - early_start_buffer;
        let latest_start = appointment.scheduled_start_time() + late_start_limit;

        current_time >= earliest_start && current_time <= latest_start
    }

    async fn generate_session_token(
        &self,
        appointment_id: Uuid,
        participant_type: ParticipantType,
    ) -> Result<String, AppointmentError> {
        // In production, this would generate a secure JWT token for video service
        let token = format!("session_{}_{:?}_{}", 
                           appointment_id, 
                           participant_type, 
                           Utc::now().timestamp());
        Ok(token)
    }

    async fn calculate_wait_time(
        &self,
        _appointment: &Appointment,
        participant_type: ParticipantType,
    ) -> Option<i32> {
        match participant_type {
            ParticipantType::Patient => {
                // Patients typically wait if doctor hasn't joined yet
                Some(2) // Estimated 2 minutes
            },
            ParticipantType::Doctor => {
                // Doctors usually join on time
                None
            },
            ParticipantType::System => None,
        }
    }

    async fn log_session_event(
        &self,
        appointment_id: Uuid,
        participant_type: ParticipantType,
        event_type: &str,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let event_data = json!({
            "appointment_id": appointment_id,
            "participant_type": format!("{:?}", participant_type),
            "event_type": event_type,
            "timestamp": Utc::now().to_rfc3339()
        });

        let _result: Vec<Value> = self.supabase.request(
            Method::POST,
            "/rest/v1/telemedicine_session_events",
            Some(auth_token),
            Some(event_data),
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn store_session_quality_metrics(
        &self,
        appointment_id: Uuid,
        metrics: &SessionQualityMetrics,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let metrics_data = json!({
            "appointment_id": appointment_id,
            "video_quality_score": metrics.video_quality_score,
            "audio_quality_score": metrics.audio_quality_score,
            "connection_stability_score": metrics.connection_stability_score,
            "overall_satisfaction": metrics.overall_satisfaction,
            "technical_issues_reported": metrics.technical_issues_reported,
            "recorded_at": Utc::now().to_rfc3339()
        });

        let _result: Vec<Value> = self.supabase.request(
            Method::POST,
            "/rest/v1/telemedicine_quality_metrics",
            Some(auth_token),
            Some(metrics_data),
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn generate_session_summary(
        &self,
        appointment_id: Uuid,
        session_duration_minutes: i32,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let summary_data = json!({
            "appointment_id": appointment_id,
            "session_duration_minutes": session_duration_minutes,
            "completion_status": "completed",
            "generated_at": Utc::now().to_rfc3339()
        });

        let _result: Vec<Value> = self.supabase.request(
            Method::POST,
            "/rest/v1/telemedicine_session_summaries",
            Some(auth_token),
            Some(summary_data),
        ).await.map_err(|e| AppointmentError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

// ==============================================================================
// TELEMEDICINE DATA STRUCTURES
// ==============================================================================

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PatientTelemedicineProfile {
    pub patient_id: Uuid,
    pub telemedicine_consent: bool,
    pub device_compatibility_verified: bool,
    pub network_speed_adequate: bool,
    pub privacy_environment_confirmed: bool,
    pub preferred_communication_method: String,
    pub technical_assistance_needed: bool,
}

impl Default for PatientTelemedicineProfile {
    fn default() -> Self {
        Self {
            patient_id: Uuid::new_v4(),
            telemedicine_consent: false,
            device_compatibility_verified: false,
            network_speed_adequate: false,
            privacy_environment_confirmed: false,
            preferred_communication_method: "video".to_string(),
            technical_assistance_needed: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TelemedicineReadinessReport {
    pub is_ready: bool,
    pub consent_provided: bool,
    pub device_compatible: bool,
    pub network_adequate: bool,
    pub privacy_setup: bool,
    pub technical_support_needed: bool,
    pub recommendations: Vec<String>,
}

impl TelemedicineReadinessReport {
    pub fn default_ready() -> Self {
        Self {
            is_ready: true,
            consent_provided: true,
            device_compatible: true,
            network_adequate: true,
            privacy_setup: true,
            technical_support_needed: false,
            recommendations: vec!["Your telemedicine setup is ready!".to_string()],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ParticipantType {
    Patient,
    Doctor,
    System,
}

#[derive(Debug, Clone)]
pub struct TelemedicineSessionInfo {
    pub room_url: String,
    pub session_token: String,
    pub participant_type: ParticipantType,
    pub appointment_id: Uuid,
    pub estimated_wait_time_minutes: Option<i32>,
    pub technical_support_available: bool,
}

#[derive(Debug, Clone)]
pub struct SessionQualityMetrics {
    pub video_quality_score: f32,      // 0.0 to 1.0
    pub audio_quality_score: f32,      // 0.0 to 1.0
    pub connection_stability_score: f32, // 0.0 to 1.0
    pub overall_satisfaction: i32,     // 1 to 5 rating
    pub technical_issues_reported: bool,
}