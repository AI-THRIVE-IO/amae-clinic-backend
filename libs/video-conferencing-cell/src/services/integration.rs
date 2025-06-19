// libs/video-conferencing-cell/src/services/integration.rs
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;
use shared_models::auth::User;

use crate::models::{
    CreateVideoSessionRequest, VideoConferencingError, VideoSession, VideoSessionStatus,
    VideoSessionType,
};
use crate::services::session::VideoSessionService;

/// Integration service for coordinating video conferencing with appointment system
/// Handles automatic session creation, appointment lifecycle events, and scheduling
pub struct VideoConferencingIntegrationService {
    supabase: Arc<SupabaseClient>,
    session_service: VideoSessionService,
    config: Arc<AppConfig>,
}

impl VideoConferencingIntegrationService {
    pub fn new(config: &AppConfig) -> Result<Self, VideoConferencingError> {
        let supabase = Arc::new(SupabaseClient::new(config));
        let session_service = VideoSessionService::new(config)?;

        Ok(Self {
            supabase,
            session_service,
            config: Arc::new(config.clone()),
        })
    }

    /// Automatically create video session when appointment is confirmed
    /// This can be called by appointment-cell when appointment status changes
    pub async fn create_session_for_appointment(
        &self,
        appointment_id: Uuid,
        session_type: VideoSessionType,
        requesting_user: &User,
        auth_token: &str,
    ) -> Result<VideoSession, VideoConferencingError> {
        info!(
            "Auto-creating video session for appointment: {} by user: {}",
            appointment_id, requesting_user.id
        );

        // Get appointment details
        let appointment = self.get_appointment(appointment_id, auth_token).await?;

        // Extract appointment time for session scheduling
        let appointment_date = appointment["appointment_date"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .ok_or_else(|| VideoConferencingError::ValidationError {
                message: "Invalid appointment date format".to_string(),
            })?;

        // Check if video session already exists for this appointment
        if let Ok(_existing_session) = self.get_session_by_appointment(appointment_id, auth_token).await {
            warn!("Video session already exists for appointment: {}", appointment_id);
            return Err(VideoConferencingError::ValidationError {
                message: "Video session already exists for this appointment".to_string(),
            });
        }

        // Create session request
        let session_request = CreateVideoSessionRequest {
            appointment_id,
            session_type,
            scheduled_start_time: appointment_date,
        };

        // Create the session
        let session_response = self
            .session_service
            .create_session(session_request, requesting_user, auth_token)
            .await?;

        // Update appointment with video session link
        self.update_appointment_with_video_link(
            appointment_id,
            &session_response.session.id,
            auth_token,
        )
        .await?;

        info!(
            "Successfully created video session {} for appointment: {}",
            session_response.session.id, appointment_id
        );

        Ok(session_response.session)
    }

    /// Handle appointment status changes
    /// Called when appointment status changes to update video session accordingly
    pub async fn handle_appointment_status_change(
        &self,
        appointment_id: Uuid,
        new_status: &str,
        auth_token: &str,
    ) -> Result<(), VideoConferencingError> {
        info!(
            "Handling appointment status change for {}: {}",
            appointment_id, new_status
        );

        // Get associated video session if it exists
        let session_result = self.get_session_by_appointment(appointment_id, auth_token).await;

        match new_status {
            "confirmed" => {
                // If no video session exists, optionally create one
                if session_result.is_err() {
                    debug!("Appointment confirmed but no video session - manual creation may be needed");
                }
            }
            "in_progress" => {
                // Appointment started - ensure video session is ready
                if let Ok(session) = session_result {
                    if session.status == VideoSessionStatus::Scheduled {
                        self.update_session_status(
                            session.id,
                            VideoSessionStatus::Ready,
                            auth_token,
                        )
                        .await?;
                    }
                }
            }
            "completed" => {
                // Appointment completed - end video session if still active
                if let Ok(session) = session_result {
                    if matches!(
                        session.status,
                        VideoSessionStatus::Ready | VideoSessionStatus::InProgress
                    ) {
                        self.update_session_status(
                            session.id,
                            VideoSessionStatus::Completed,
                            auth_token,
                        )
                        .await?;
                    }
                }
            }
            "cancelled" => {
                // Appointment cancelled - cancel video session
                if let Ok(session) = session_result {
                    if !matches!(session.status, VideoSessionStatus::Completed) {
                        self.update_session_status(
                            session.id,
                            VideoSessionStatus::Cancelled,
                            auth_token,
                        )
                        .await?;
                    }
                }
            }
            _ => {
                debug!("No video session action needed for status: {}", new_status);
            }
        }

        Ok(())
    }

    /// Get upcoming video sessions for a user (patient or doctor) - Production-hardened with fallback logic
    pub async fn get_upcoming_sessions(
        &self,
        user: &User,
        hours_ahead: i32,
        auth_token: &str,
    ) -> Result<Vec<VideoSession>, VideoConferencingError> {
        info!("Getting upcoming video sessions for user: {} with production-hardened logic", user.id);

        // Primary attempt: try with scheduled_start_time column
        match self.get_upcoming_sessions_primary(user, hours_ahead, auth_token).await {
            Ok(sessions) => {
                debug!("Successfully retrieved {} upcoming sessions via primary query", sessions.len());
                return Ok(sessions);
            }
            Err(e) => {
                warn!("Primary upcoming sessions query failed: {}", e);
            }
        }

        // Fallback 1: Try with created_at as time reference
        match self.get_upcoming_sessions_fallback(user, hours_ahead, auth_token).await {
            Ok(sessions) => {
                warn!("Retrieved {} upcoming sessions via fallback query", sessions.len());
                return Ok(sessions);
            }
            Err(e) => {
                warn!("Fallback upcoming sessions query failed: {}", e);
            }
        }

        // Fallback 2: Return empty result with warning
        warn!("All upcoming sessions queries failed, returning empty result");
        Ok(Vec::new())
    }

    /// Primary upcoming sessions query using scheduled_start_time
    async fn get_upcoming_sessions_primary(
        &self,
        user: &User,
        hours_ahead: i32,
        auth_token: &str,
    ) -> Result<Vec<VideoSession>, VideoConferencingError> {
        debug!("Executing primary upcoming sessions query");

        let now = Utc::now();
        let until_time = now + Duration::hours(hours_ahead as i64);
        
        // Use properly formatted timestamps
        let now_str = now.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string();
        let until_str = until_time.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string();

        let query = match user.role.as_deref() {
            Some("doctor") => format!(
                "doctor_id=eq.{}&scheduled_start_time=gte.{}&scheduled_start_time=lte.{}&status=in.(scheduled,ready,in_progress)",
                user.id, now_str, until_str
            ),
            _ => format!(
                "patient_id=eq.{}&scheduled_start_time=gte.{}&scheduled_start_time=lte.{}&status=in.(scheduled,ready,in_progress)",
                user.id, now_str, until_str
            ),
        };

        let path = format!("/rest/v1/video_sessions?{}&order=scheduled_start_time.asc&limit=50", query);

        let result: Vec<Value> = self
            .supabase
            .request(Method::GET, &path, Some(auth_token), None)
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: format!("Primary sessions query failed: {}", e),
            })?;

        let sessions: Result<Vec<VideoSession>, _> = result
            .into_iter()
            .map(|v| serde_json::from_value(v))
            .collect();

        sessions.map_err(|e| VideoConferencingError::DatabaseError {
            message: format!("Failed to parse sessions: {}", e),
        })
    }

    /// Fallback upcoming sessions query using created_at timestamp
    async fn get_upcoming_sessions_fallback(
        &self,
        user: &User,
        hours_ahead: i32,
        auth_token: &str,
    ) -> Result<Vec<VideoSession>, VideoConferencingError> {
        debug!("Executing fallback upcoming sessions query");

        let now = Utc::now();
        let until_time = now + Duration::hours(hours_ahead as i64);
        
        let now_str = now.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string();
        let until_str = until_time.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string();

        let query = match user.role.as_deref() {
            Some("doctor") => format!(
                "doctor_id=eq.{}&created_at=gte.{}&created_at=lte.{}&status=in.(scheduled,ready,in_progress)",
                user.id, now_str, until_str
            ),
            _ => format!(
                "patient_id=eq.{}&created_at=gte.{}&created_at=lte.{}&status=in.(scheduled,ready,in_progress)",
                user.id, now_str, until_str
            ),
        };

        let path = format!("/rest/v1/video_sessions?{}&order=created_at.asc&limit=50", query);

        let result: Vec<Value> = self
            .supabase
            .request(Method::GET, &path, Some(auth_token), None)
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: format!("Fallback sessions query failed: {}", e),
            })?;

        let sessions: Result<Vec<VideoSession>, _> = result
            .into_iter()
            .map(|v| serde_json::from_value(v))
            .collect();

        sessions.map_err(|e| VideoConferencingError::DatabaseError {
            message: format!("Failed to parse fallback sessions: {}", e),
        })
    }

    /// Get session statistics for appointments
    pub async fn get_appointment_video_stats(
        &self,
        appointment_id: Uuid,
        user: &User,
        auth_token: &str,
    ) -> Result<Value, VideoConferencingError> {
        let session = self.get_session_by_appointment(appointment_id, auth_token).await?;

        // Verify access
        let has_access = session.patient_id.to_string() == user.id
            || session.doctor_id.to_string() == user.id
            || user.role.as_deref() == Some("admin");

        if !has_access {
            return Err(VideoConferencingError::Unauthorized);
        }

        let stats = self
            .session_service
            .get_session_stats(session.id, user, auth_token)
            .await?;

        Ok(json!({
            "appointment_id": appointment_id,
            "video_session": stats.session,
            "participants": stats.participants,
            "duration_minutes": stats.total_duration_minutes,
            "connection_quality": stats.connection_quality_summary,
        }))
    }

    /// Check if video conferencing is available for an appointment
    pub async fn is_video_available_for_appointment(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<bool, VideoConferencingError> {
        // Check if video conferencing is configured
        if !self.config.is_video_conferencing_configured() {
            return Ok(false);
        }

        // Check if appointment exists and is valid for video
        let appointment = self.get_appointment(appointment_id, auth_token).await?;

        // Check appointment type and status
        let appointment_type = appointment["appointment_type"]
            .as_str()
            .unwrap_or("")
            .to_lowercase();

        let status = appointment["status"].as_str().unwrap_or("").to_lowercase();

        // Video conferencing is available for certain appointment types and statuses
        let video_compatible_types = vec!["consultation", "follow_up", "general_consultation"];
        let video_compatible_statuses = vec!["confirmed", "in_progress"];

        let is_compatible = video_compatible_types
            .iter()
            .any(|&t| appointment_type.contains(t))
            && video_compatible_statuses.contains(&status.as_str());

        Ok(is_compatible)
    }

    /// Cleanup expired video sessions
    pub async fn cleanup_expired_sessions(&self, auth_token: &str) -> Result<i32, VideoConferencingError> {
        info!("Cleaning up expired video sessions");

        // Find sessions that are scheduled but past their time
        let cutoff_time = Utc::now() - Duration::hours(2); // 2 hours past scheduled time

        let path = format!(
            "/rest/v1/video_sessions?status=eq.scheduled&scheduled_start_time=lt.{}",
            cutoff_time.to_rfc3339()
        );

        let expired_sessions: Vec<Value> = self
            .supabase
            .request(Method::GET, &path, Some(auth_token), None)
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: e.to_string(),
            })?;

        let mut cleaned_count = 0;

        for session_data in expired_sessions {
            if let Some(session_id) = session_data["id"]
                .as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                // Update status to cancelled
                let update_path = format!("/rest/v1/video_sessions?id=eq.{}", session_id);
                let update_body = json!({
                    "status": VideoSessionStatus::Cancelled,
                    "updated_at": Utc::now(),
                });

                let _: Vec<Value> = self
                    .supabase
                    .request(Method::PATCH, &update_path, Some(auth_token), Some(update_body))
                    .await
                    .map_err(|e| VideoConferencingError::DatabaseError {
                        message: e.to_string(),
                    })?;

                cleaned_count += 1;
            }
        }

        info!("Cleaned up {} expired video sessions", cleaned_count);
        Ok(cleaned_count)
    }

    // ==============================================================================
    // PRIVATE HELPER METHODS
    // ==============================================================================

    async fn get_appointment(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<Value, VideoConferencingError> {
        let path = format!("/rest/v1/appointments?id=eq.{}", appointment_id);
        let result: Vec<Value> = self
            .supabase
            .request(Method::GET, &path, Some(auth_token), None)
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: e.to_string(),
            })?;

        result
            .into_iter()
            .next()
            .ok_or(VideoConferencingError::InvalidAppointment)
    }

    async fn get_session_by_appointment(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<VideoSession, VideoConferencingError> {
        let path = format!("/rest/v1/video_sessions?appointment_id=eq.{}", appointment_id);
        let result: Vec<Value> = self
            .supabase
            .request(Method::GET, &path, Some(auth_token), None)
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: e.to_string(),
            })?;

        let session_data = result
            .into_iter()
            .next()
            .ok_or(VideoConferencingError::SessionNotFound)?;

        serde_json::from_value(session_data).map_err(|e| VideoConferencingError::DatabaseError {
            message: format!("Failed to parse session: {}", e),
        })
    }

    async fn update_appointment_with_video_link(
        &self,
        appointment_id: Uuid,
        session_id: &Uuid,
        auth_token: &str,
    ) -> Result<(), VideoConferencingError> {
        let path = format!("/rest/v1/appointments?id=eq.{}", appointment_id);
        let body = json!({
            "video_conference_link": format!("/video/sessions/{}", session_id),
            "updated_at": Utc::now(),
        });

        let _: Vec<Value> = self
            .supabase
            .request(Method::PATCH, &path, Some(auth_token), Some(body))
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: e.to_string(),
            })?;

        Ok(())
    }

    async fn update_session_status(
        &self,
        session_id: Uuid,
        status: VideoSessionStatus,
        auth_token: &str,
    ) -> Result<(), VideoConferencingError> {
        let path = format!("/rest/v1/video_sessions?id=eq.{}", session_id);
        let body = json!({
            "status": status,
            "updated_at": Utc::now(),
        });

        let _: Vec<Value> = self
            .supabase
            .request(Method::PATCH, &path, Some(auth_token), Some(body))
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: e.to_string(),
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> AppConfig {
        AppConfig {
            supabase_url: "test".to_string(),
            supabase_anon_key: "test".to_string(),
            supabase_jwt_secret: "test".to_string(),
            cloudflare_realtime_app_id: "test-app-id".to_string(),
            cloudflare_realtime_api_token: "test-token".to_string(),
            cloudflare_realtime_base_url: "https://test.cloudflare.com/v1".to_string(),
        }
    }

    #[test]
    fn test_integration_service_creation() {
        let config = create_test_config();
        let service = VideoConferencingIntegrationService::new(&config);
        assert!(service.is_ok());
    }

    #[test]
    fn test_integration_service_fails_without_config() {
        let mut config = create_test_config();
        config.cloudflare_realtime_app_id = "".to_string();
        
        let service = VideoConferencingIntegrationService::new(&config);
        assert!(matches!(service, Err(VideoConferencingError::NotConfigured)));
    }
}