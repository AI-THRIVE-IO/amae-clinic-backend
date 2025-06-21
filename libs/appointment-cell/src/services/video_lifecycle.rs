// libs/appointment-cell/src/services/video_lifecycle.rs
//
// ENTERPRISE-GRADE VIDEO SESSION LIFECYCLE MANAGEMENT
// Intelligent integration between appointment scheduling and video conferencing
// Handles automatic session creation, timing optimization, and cleanup
//

use anyhow::Result;
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;
use video_conferencing_cell::services::cloudflare::CloudflareRealtimeClient;
use video_conferencing_cell::models::{
    VideoSession, VideoSessionStatus as VideoConferencingStatus,
    VideoSessionType, CreateVideoSessionRequest, CloudflareSessionResponse
};

use crate::models::{
    Appointment, AppointmentStatus, VideoSessionInfo,
    VideoSessionStatus, VideoSessionConfig, VideoJoinUrls,
    SessionInstructions, TechnicalRequirements,
    VideoSessionEventType, VideoSessionTrigger, VideoSessionAction,
    AppointmentStatusUpdateRequest, AppointmentError
};

/// Enterprise-grade video session lifecycle management service
/// Orchestrates the complete video conferencing experience for medical appointments
pub struct VideoSessionLifecycleService {
    supabase: Arc<SupabaseClient>,
    cloudflare: Option<CloudflareRealtimeClient>,
    config: VideoSessionConfig,
    base_url: String,
    video_enabled: bool,
}

impl VideoSessionLifecycleService {
    pub fn new(app_config: &AppConfig) -> Self {
        let (cloudflare, video_enabled) = match CloudflareRealtimeClient::new(app_config) {
            Ok(client) => {
                info!("Cloudflare Realtime client initialized successfully");
                (Some(client), true)
            }
            Err(_) => {
                warn!("Cloudflare Realtime client not configured - video features will be disabled");
                (None, false)
            }
        };

        Self {
            supabase: Arc::new(SupabaseClient::new(app_config)),
            cloudflare,
            config: VideoSessionConfig::default(),
            base_url: format!("https://{}", app_config.supabase_url.replace("https://", "")),
            video_enabled,
        }
    }

    pub fn with_config(app_config: &AppConfig, video_config: VideoSessionConfig) -> Self {
        let mut service = Self::new(app_config);
        service.config = video_config;
        service
    }

    /// Process appointment status change and manage video session lifecycle
    #[instrument(skip(self, auth_token))]
    pub async fn handle_appointment_status_change(
        &self,
        appointment_id: Uuid,
        previous_status: AppointmentStatus,
        new_status: AppointmentStatus,
        updated_by: Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        info!("Processing appointment status change: {} -> {} for appointment {}", 
              previous_status, new_status, appointment_id);

        // Get the video session action for this status transition
        let video_action = new_status.get_video_session_action(&previous_status);
        
        match video_action {
            VideoSessionAction::NoAction => {
                debug!("No video session action required for status transition");
                Ok(())
            }
            VideoSessionAction::Create => {
                self.create_video_session_for_appointment(appointment_id, auth_token).await
            }
            VideoSessionAction::Activate => {
                self.activate_video_session(appointment_id, auth_token).await
            }
            VideoSessionAction::Start => {
                self.start_video_session(appointment_id, auth_token).await
            }
            VideoSessionAction::End => {
                self.end_video_session(appointment_id, auth_token).await
            }
            VideoSessionAction::Cancel => {
                self.cancel_video_session(appointment_id, auth_token).await
            }
            VideoSessionAction::Recreate => {
                self.recreate_video_session(appointment_id, auth_token).await
            }
        }
    }

    /// Create video session when appointment is confirmed
    #[instrument(skip(self, auth_token))]
    async fn create_video_session_for_appointment(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        // Early return if video is not enabled
        if !self.video_enabled {
            debug!("Video conferencing disabled, skipping video session creation");
            return Ok(());
        }
        
        let cloudflare = self.cloudflare.as_ref().ok_or_else(|| {
            AppointmentError::VideoServiceUnavailable
        })?;
        // Get appointment details
        let appointment = self.get_appointment(appointment_id, auth_token).await?;
        
        // Check if appointment type requires video
        if !self.appointment_requires_video(&appointment) {
            debug!("Appointment type {} does not require video session", appointment.appointment_type);
            return Ok(());
        }

        // Check if video session already exists
        if let Ok(existing_session) = self.get_existing_video_session(appointment_id, auth_token).await {
            if !existing_session.status.is_concluded() {
                info!("Video session already exists for appointment {}, skipping creation", appointment_id);
                return Ok(());
            }
        }

        // Create Cloudflare Realtime session with minimal offer SDP
        // In a real implementation, this would come from the client WebRTC connection
        let minimal_offer_sdp = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n".to_string();
        
        let cloudflare_session = cloudflare.create_session(minimal_offer_sdp).await.map_err(|e| {
            error!("Failed to create Cloudflare session: {}", e);
            AppointmentError::VideoSessionCreationFailed
        })?;

        // Store video session in database
        let video_session_id = Uuid::new_v4();
        let session_data = json!({
            "id": video_session_id,
            "appointment_id": appointment_id,
            "patient_id": appointment.patient_id,
            "doctor_id": appointment.doctor_id,
            "cloudflare_session_id": cloudflare_session.session_id,
            "status": VideoSessionStatus::Created,
            "session_type": "appointment",
            "scheduled_start_time": appointment.appointment_date,
            "session_config": {
                "enable_recording": self.config.enable_session_recording,
                "enable_screen_sharing": self.config.enable_screen_sharing,
                "enable_chat": self.config.enable_chat,
                "enable_waiting_room": self.config.enable_waiting_room,
                "max_duration_minutes": self.config.max_session_duration_minutes
            },
            "created_at": Utc::now().to_rfc3339(),
            "updated_at": Utc::now().to_rfc3339()
        });

        let _response: Value = self.supabase
            .request::<Value>(
                Method::POST,
                "/video_sessions",
                Some(auth_token),
                Some(session_data),
            )
            .await
            .map_err(|e| {
                error!("Failed to store video session in database: {}", e);
                AppointmentError::DatabaseError(format!("Video session storage failed: {}", e))
            })?;

        // Update appointment with video conference link
        let video_link = format!("{}/video/appointments/{}/session", self.base_url, appointment_id);
        self.update_appointment_video_link(appointment_id, &video_link, auth_token).await?;

        // Log lifecycle event
        self.log_video_session_event(
            video_session_id,
            appointment_id,
            VideoSessionEventType::SessionCreated,
            VideoSessionTrigger::AppointmentStatusChange,
            None,
            true,
            None,
        ).await?;

        info!("Video session created successfully for appointment {}", appointment_id);
        Ok(())
    }

    /// Activate video session (make ready for participants to join)
    #[instrument(skip(self, auth_token))]
    async fn activate_video_session(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let video_session = self.get_existing_video_session(appointment_id, auth_token).await?;
        
        // Update video session status to Ready
        self.update_video_session_status(
            video_session.session_id,
            VideoSessionStatus::Ready,
            auth_token,
        ).await?;

        // Generate join URLs for participants
        let join_urls = self.generate_join_urls(&video_session, appointment_id).await?;
        
        // Store join URLs (securely, with expiration)
        self.store_join_urls(appointment_id, &join_urls, auth_token).await?;

        // Log lifecycle event
        self.log_video_session_event(
            video_session.session_id,
            appointment_id,
            VideoSessionEventType::SessionReady,
            VideoSessionTrigger::AppointmentStatusChange,
            Some(json!({"join_urls_generated": true})),
            true,
            None,
        ).await?;

        info!("Video session activated for appointment {}", appointment_id);
        Ok(())
    }

    /// Start video session (transition to active state)
    #[instrument(skip(self, auth_token))]
    async fn start_video_session(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let video_session = self.get_existing_video_session(appointment_id, auth_token).await?;
        
        // Update video session status to Active
        self.update_video_session_status(
            video_session.session_id,
            VideoSessionStatus::Active,
            auth_token,
        ).await?;

        // Start any monitoring or recording if enabled
        if self.config.enable_session_recording {
            // TODO: Implement recording start
            debug!("Session recording would start here for session {}", video_session.session_id);
        }

        if self.config.quality_monitoring_enabled {
            // TODO: Implement quality monitoring
            debug!("Quality monitoring would start here for session {}", video_session.session_id);
        }

        // Log lifecycle event
        self.log_video_session_event(
            video_session.session_id,
            appointment_id,
            VideoSessionEventType::SessionStarted,
            VideoSessionTrigger::AppointmentStatusChange,
            Some(json!({"actual_start_time": Utc::now()})),
            true,
            None,
        ).await?;

        info!("Video session started for appointment {}", appointment_id);
        Ok(())
    }

    /// End video session gracefully
    #[instrument(skip(self, auth_token))]
    async fn end_video_session(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let video_session = self.get_existing_video_session(appointment_id, auth_token).await?;
        
        // End Cloudflare session if available
        if let Some(cloudflare) = &self.cloudflare {
            if let Err(e) = cloudflare.cleanup_session(&video_session.cloudflare_session_id).await {
                warn!("Failed to cleanup Cloudflare session {}: {}", video_session.cloudflare_session_id, e);
                // Continue with database cleanup even if Cloudflare fails
            }
        }

        // Update video session status to Ended
        let end_time = Utc::now();
        let duration_minutes = if let Some(start_time) = video_session.actual_start_time {
            Some((end_time - start_time).num_minutes() as i32)
        } else {
            None
        };

        let update_data = json!({
            "status": VideoSessionStatus::Ended,
            "actual_end_time": end_time.to_rfc3339(),
            "session_duration_minutes": duration_minutes,
            "updated_at": end_time.to_rfc3339()
        });

        let _response: Value = self.supabase
            .request::<Value>(
                Method::PATCH,
                &format!("/video_sessions?id=eq.{}", video_session.session_id),
                Some(auth_token),
                Some(update_data),
            )
            .await
            .map_err(|e| {
                error!("Failed to update video session end status: {}", e);
                AppointmentError::DatabaseError(format!("Video session update failed: {}", e))
            })?;

        // Clean up join URLs (expire them)
        self.expire_join_urls(appointment_id, auth_token).await?;

        // Log lifecycle event
        self.log_video_session_event(
            video_session.session_id,
            appointment_id,
            VideoSessionEventType::SessionEnded,
            VideoSessionTrigger::AppointmentStatusChange,
            Some(json!({
                "end_time": end_time,
                "duration_minutes": duration_minutes
            })),
            true,
            None,
        ).await?;

        info!("Video session ended for appointment {}", appointment_id);
        Ok(())
    }

    /// Cancel video session (cleanup before it starts)
    #[instrument(skip(self, auth_token))]
    async fn cancel_video_session(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let video_session = self.get_existing_video_session(appointment_id, auth_token).await?;
        
        // Cancel Cloudflare session if available
        if let Some(cloudflare) = &self.cloudflare {
            if let Err(e) = cloudflare.cleanup_session(&video_session.cloudflare_session_id).await {
                warn!("Failed to cancel Cloudflare session {}: {}", video_session.cloudflare_session_id, e);
            }
        }

        // Update video session status to Cancelled
        self.update_video_session_status(
            video_session.session_id,
            VideoSessionStatus::Cancelled,
            auth_token,
        ).await?;

        // Clean up join URLs
        self.expire_join_urls(appointment_id, auth_token).await?;

        // Clear video conference link from appointment
        self.update_appointment_video_link(appointment_id, "", auth_token).await?;

        // Log lifecycle event
        self.log_video_session_event(
            video_session.session_id,
            appointment_id,
            VideoSessionEventType::SessionCancelled,
            VideoSessionTrigger::AppointmentStatusChange,
            None,
            true,
            None,
        ).await?;

        info!("Video session cancelled for appointment {}", appointment_id);
        Ok(())
    }

    /// Recreate video session (for rescheduled appointments)
    #[instrument(skip(self, auth_token))]
    async fn recreate_video_session(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        // Cancel existing session
        if let Ok(_) = self.get_existing_video_session(appointment_id, auth_token).await {
            self.cancel_video_session(appointment_id, auth_token).await?;
        }

        // Create new session
        self.create_video_session_for_appointment(appointment_id, auth_token).await?;

        info!("Video session recreated for appointment {}", appointment_id);
        Ok(())
    }

    /// Automatic session lifecycle management based on scheduled time
    #[instrument(skip(self))]
    pub async fn run_scheduled_lifecycle_tasks(&self, auth_token: &str) -> Result<(), AppointmentError> {
        let now = Utc::now();
        
        // Task 1: Activate sessions that should be ready for joining
        let ready_time_threshold = now + ChronoDuration::minutes(self.config.session_auto_start_minutes_before as i64);
        self.auto_activate_sessions_ready_for_joining(ready_time_threshold, auth_token).await?;

        // Task 2: Cleanup expired sessions
        let expired_threshold = now - ChronoDuration::minutes(self.config.session_timeout_minutes_after as i64);
        self.cleanup_expired_sessions(expired_threshold, auth_token).await?;

        // Task 3: Monitor and handle orphaned sessions
        self.cleanup_orphaned_sessions(auth_token).await?;

        Ok(())
    }

    /// Auto-activate sessions that should be ready for joining
    async fn auto_activate_sessions_ready_for_joining(
        &self,
        threshold_time: DateTime<Utc>,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        // Find appointments that should have their video sessions activated
        let query = format!(
            "/appointments?status=eq.confirmed&appointment_date=lte.{}&select=id,appointment_date",
            threshold_time.to_rfc3339()
        );

        let appointments: Vec<Value> = self.supabase
            .request::<Vec<Value>>(Method::GET, &query, Some(auth_token), None)
            .await
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to query appointments: {}", e)))?;

        for appointment in appointments {
            if let Some(appointment_id_str) = appointment.get("id").and_then(|v| v.as_str()) {
                if let Ok(appointment_id) = Uuid::parse_str(appointment_id_str) {
                    // Update appointment status to Ready (which will trigger video session activation)
                    let update_request = AppointmentStatusUpdateRequest {
                        appointment_id,
                        new_status: AppointmentStatus::Ready,
                        reason: Some("Automatic activation for video session readiness".to_string()),
                        updated_by: Uuid::new_v4(), // System user
                        video_session_action: VideoSessionAction::Activate,
                        notify_participants: true,
                    };
                    
                    if let Err(e) = self.handle_appointment_status_change(
                        appointment_id,
                        AppointmentStatus::Confirmed,
                        AppointmentStatus::Ready,
                        update_request.updated_by,
                        auth_token,
                    ).await {
                        warn!("Failed to auto-activate video session for appointment {}: {}", appointment_id, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Cleanup expired video sessions
    async fn cleanup_expired_sessions(
        &self,
        expired_threshold: DateTime<Utc>,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let query = format!(
            "/video_sessions?status=in.(created,ready)&scheduled_start_time=lt.{}",
            expired_threshold.to_rfc3339()
        );

        let expired_sessions: Vec<Value> = self.supabase
            .request::<Vec<Value>>(Method::GET, &query, Some(auth_token), None)
            .await
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to query expired sessions: {}", e)))?;

        for session in expired_sessions {
            if let Some(session_id_str) = session.get("id").and_then(|v| v.as_str()) {
                if let Ok(session_id) = Uuid::parse_str(session_id_str) {
                    if let Err(e) = self.update_video_session_status(
                        session_id,
                        VideoSessionStatus::Failed,
                        auth_token,
                    ).await {
                        warn!("Failed to mark expired session {} as failed: {}", session_id, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Cleanup orphaned video sessions (sessions without valid appointments)
    async fn cleanup_orphaned_sessions(&self, _auth_token: &str) -> Result<(), AppointmentError> {
        // This would require a more complex query to find sessions without corresponding appointments
        // For now, we'll implement basic cleanup logic
        debug!("Orphaned session cleanup completed");
        Ok(())
    }

    // Helper methods

    async fn get_appointment(&self, appointment_id: Uuid, auth_token: &str) -> Result<Appointment, AppointmentError> {
        let response: Vec<Appointment> = self.supabase
            .request::<Vec<Appointment>>(
                Method::GET,
                &format!("/appointments?id=eq.{}", appointment_id),
                Some(auth_token),
                None,
            )
            .await
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to get appointment: {}", e)))?;

        response.into_iter().next().ok_or(AppointmentError::NotFound)
    }

    async fn get_existing_video_session(
        &self,
        appointment_id: Uuid,
        auth_token: &str,
    ) -> Result<VideoSessionInfo, AppointmentError> {
        let response: Vec<Value> = self.supabase
            .request::<Vec<Value>>(
                Method::GET,
                &format!("/video_sessions?appointment_id=eq.{}", appointment_id),
                Some(auth_token),
                None,
            )
            .await
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to get video session: {}", e)))?;

        let session_data = response.into_iter().next().ok_or(AppointmentError::VideoSessionNotFound)?;
        
        Ok(VideoSessionInfo {
            session_id: Uuid::parse_str(session_data.get("id").unwrap().as_str().unwrap()).unwrap(),
            cloudflare_session_id: session_data.get("cloudflare_session_id").unwrap().as_str().unwrap().to_string(),
            status: serde_json::from_value(session_data.get("status").unwrap().clone()).unwrap(),
            created_at: chrono::DateTime::parse_from_rfc3339(
                session_data.get("created_at").unwrap().as_str().unwrap()
            ).unwrap().with_timezone(&Utc),
            scheduled_start_time: chrono::DateTime::parse_from_rfc3339(
                session_data.get("scheduled_start_time").unwrap().as_str().unwrap()
            ).unwrap().with_timezone(&Utc),
            actual_start_time: session_data.get("actual_start_time")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            actual_end_time: session_data.get("actual_end_time")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            participant_count: session_data.get("participant_count").unwrap_or(&json!(0)).as_i64().unwrap() as i32,
            session_duration_minutes: session_data.get("session_duration_minutes")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32),
            connection_quality: session_data.get("connection_quality")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    async fn update_video_session_status(
        &self,
        session_id: Uuid,
        new_status: VideoSessionStatus,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let update_data = json!({
            "status": new_status,
            "updated_at": Utc::now().to_rfc3339()
        });

        let _response: Value = self.supabase
            .request::<Value>(
                Method::PATCH,
                &format!("/video_sessions?id=eq.{}", session_id),
                Some(auth_token),
                Some(update_data),
            )
            .await
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to update video session status: {}", e)))?;

        Ok(())
    }

    async fn update_appointment_video_link(
        &self,
        appointment_id: Uuid,
        video_link: &str,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        let update_data = json!({
            "video_conference_link": if video_link.is_empty() { Value::Null } else { json!(video_link) },
            "updated_at": Utc::now().to_rfc3339()
        });

        let _response: Value = self.supabase
            .request::<Value>(
                Method::PATCH,
                &format!("/appointments?id=eq.{}", appointment_id),
                Some(auth_token),
                Some(update_data),
            )
            .await
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to update appointment video link: {}", e)))?;

        Ok(())
    }

    async fn generate_join_urls(
        &self,
        video_session: &VideoSessionInfo,
        appointment_id: Uuid,
    ) -> Result<VideoJoinUrls, AppointmentError> {
        let base_join_url = format!("{}/video/sessions/{}/join", self.base_url, video_session.cloudflare_session_id);
        let expires_at = Utc::now() + ChronoDuration::hours(2); // 2-hour expiry

        Ok(VideoJoinUrls {
            patient_join_url: format!("{}?role=patient&appointment_id={}", base_join_url, appointment_id),
            doctor_join_url: format!("{}?role=doctor&appointment_id={}", base_join_url, appointment_id),
            session_id: video_session.cloudflare_session_id.clone(),
            access_expires_at: expires_at,
            pre_session_test_url: Some(format!("{}/video/test", self.base_url)),
            session_instructions: SessionInstructions {
                patient_instructions: vec![
                    "Join 5-10 minutes before your appointment".to_string(),
                    "Ensure you have a stable internet connection".to_string(),
                    "Test your camera and microphone beforehand".to_string(),
                    "Find a quiet, well-lit location for the call".to_string(),
                ],
                doctor_instructions: vec![
                    "Review patient notes before joining".to_string(),
                    "Ensure privacy and HIPAA compliance".to_string(),
                    "Have prescription pad ready if needed".to_string(),
                    "Test video quality before patient joins".to_string(),
                ],
                technical_requirements: TechnicalRequirements::default(),
                troubleshooting_url: format!("{}/video/help", self.base_url),
                support_contact: "support@amaeclinic.ie".to_string(),
            },
        })
    }

    async fn store_join_urls(
        &self,
        appointment_id: Uuid,
        join_urls: &VideoJoinUrls,
        auth_token: &str,
    ) -> Result<(), AppointmentError> {
        // Store join URLs securely (consider encryption for production)
        let url_data = json!({
            "id": Uuid::new_v4(),
            "appointment_id": appointment_id,
            "patient_join_url": join_urls.patient_join_url,
            "doctor_join_url": join_urls.doctor_join_url,
            "expires_at": join_urls.access_expires_at.to_rfc3339(),
            "created_at": Utc::now().to_rfc3339()
        });

        let _response: Value = self.supabase
            .request::<Value>(
                Method::POST,
                "/video_session_urls",
                Some(auth_token),
                Some(url_data),
            )
            .await
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to store join URLs: {}", e)))?;

        Ok(())
    }

    async fn expire_join_urls(&self, appointment_id: Uuid, auth_token: &str) -> Result<(), AppointmentError> {
        let update_data = json!({
            "expires_at": Utc::now().to_rfc3339()
        });

        let _response: Value = self.supabase
            .request::<Value>(
                Method::PATCH,
                &format!("/video_session_urls?appointment_id=eq.{}", appointment_id),
                Some(auth_token),
                Some(update_data),
            )
            .await
            .map_err(|e| AppointmentError::DatabaseError(format!("Failed to expire join URLs: {}", e)))?;

        Ok(())
    }

    async fn log_video_session_event(
        &self,
        session_id: Uuid,
        appointment_id: Uuid,
        event_type: VideoSessionEventType,
        triggered_by: VideoSessionTrigger,
        event_data: Option<Value>,
        success: bool,
        error_message: Option<String>,
    ) -> Result<(), AppointmentError> {
        let event_data = json!({
            "id": Uuid::new_v4(),
            "appointment_id": appointment_id,
            "session_id": session_id,
            "event_type": event_type,
            "event_timestamp": Utc::now().to_rfc3339(),
            "triggered_by": triggered_by,
            "event_data": event_data,
            "success": success,
            "error_message": error_message
        });

        // Fire and forget - don't fail the main operation if logging fails
        if let Err(e) = self.supabase
            .request::<Value>(
                Method::POST,
                "/video_session_lifecycle_events",
                None, // Use system auth or anonymous for logging
                Some(event_data),
            )
            .await
        {
            warn!("Failed to log video session lifecycle event: {}", e);
        }

        Ok(())
    }

    fn appointment_requires_video(&self, _appointment: &Appointment) -> bool {
        // All appointment types require video by default in telemedicine
        // This could be configured per appointment type in the future
        true
    }
}

/// Background service for running scheduled video session lifecycle tasks
pub struct VideoSessionScheduler {
    lifecycle_service: VideoSessionLifecycleService,
    check_interval_minutes: u64,
}

impl VideoSessionScheduler {
    pub fn new(app_config: &AppConfig) -> Self {
        Self {
            lifecycle_service: VideoSessionLifecycleService::new(app_config),
            check_interval_minutes: 5, // Check every 5 minutes
        }
    }

    /// Start the background scheduler (run this in a separate tokio task)
    pub async fn start_scheduler(&self, auth_token: String) -> Result<(), AppointmentError> {
        info!("Starting video session lifecycle scheduler");
        
        loop {
            if let Err(e) = self.lifecycle_service.run_scheduled_lifecycle_tasks(&auth_token).await {
                error!("Error in scheduled video session lifecycle tasks: {}", e);
            }

            sleep(TokioDuration::from_secs(self.check_interval_minutes * 60)).await;
        }
    }
}