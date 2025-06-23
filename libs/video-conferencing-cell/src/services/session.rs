// libs/video-conferencing-cell/src/services/session.rs
use anyhow::Result;
use chrono::Utc;
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;
use shared_models::auth::User;

use crate::models::{
    CreateVideoSessionRequest, CreateVideoSessionResponse, JoinSessionRequest,
    JoinSessionResponse, ParticipantType, SessionParticipant, TrackObject,
    VideoConferencingError, VideoSession, VideoSessionStatsResponse, VideoSessionStatus,
};
use crate::services::cloudflare::CloudflareRealtimeClient;

/// Video session management service
/// Handles session lifecycle, participant management, and Cloudflare integration
pub struct VideoSessionService {
    supabase: Arc<SupabaseClient>,
    cloudflare: CloudflareRealtimeClient,
    config: Arc<AppConfig>,
}

impl VideoSessionService {
    pub fn new(config: &AppConfig) -> Result<Self, VideoConferencingError> {
        let supabase = Arc::new(SupabaseClient::new(config));
        let cloudflare = CloudflareRealtimeClient::new(config)?;

        Ok(Self {
            supabase,
            cloudflare,
            config: Arc::new(config.clone()),
        })
    }

    /// Create a new video session tied to an appointment
    pub async fn create_session(
        &self,
        request: CreateVideoSessionRequest,
        user: &User,
        auth_token: &str,
    ) -> Result<CreateVideoSessionResponse, VideoConferencingError> {
        info!(
            "Creating video session for appointment: {} by user: {}",
            request.appointment_id, user.id
        );

        // Verify the appointment exists and user has access
        let appointment = self
            .get_appointment(request.appointment_id, auth_token)
            .await?;

        // Verify user authorization (patient, doctor, or admin)
        self.verify_appointment_access(&appointment, user)?;

        // Extract appointment details
        let patient_id = appointment["patient_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| VideoConferencingError::ValidationError {
                message: "Invalid patient ID in appointment".to_string(),
            })?;

        let doctor_id = appointment["doctor_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| VideoConferencingError::ValidationError {
                message: "Invalid doctor ID in appointment".to_string(),
            })?;

        // Generate room_id for this appointment (hotfix: bypass room creation) | TODO: FIX FOR PRODUCTION!
        let room_id = format!("room_{}_{}", 
            request.appointment_id.to_string().chars().take(8).collect::<String>(),
            chrono::Utc::now().timestamp()
        );
        
        // Determine participant details based on user role
        let (participant_id, participant_type) = match user.role.as_deref() {
            Some("doctor") => (user.id.parse().unwrap_or(doctor_id), crate::models::ParticipantType::Doctor),
            Some("specialist") => (user.id.parse().unwrap_or(doctor_id), crate::models::ParticipantType::Specialist),
            Some("nurse") => (user.id.parse().unwrap_or(doctor_id), crate::models::ParticipantType::Nurse),
            _ => (user.id.parse().unwrap_or(patient_id), crate::models::ParticipantType::Patient),
        };

        // Create enhanced video session record with room support
        let session_id = Uuid::new_v4();
        let video_session = VideoSession {
            id: session_id,
            appointment_id: request.appointment_id,
            
            // Enhanced room-based architecture
            room_id: room_id.clone(),
            participant_id,
            participant_type,
            
            // Legacy fields for backward compatibility
            patient_id: Some(patient_id),
            doctor_id: Some(doctor_id),
            
            // Session management
            cloudflare_session_id: None, // Will be set when participant joins
            status: VideoSessionStatus::Scheduled,
            session_type: request.session_type,
            scheduled_start_time: request.scheduled_start_time,
            actual_start_time: None,
            actual_end_time: None,
            session_duration_minutes: None,
            quality_rating: None,
            connection_issues: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Store session in database
        self.store_session(&video_session, auth_token).await?;

        // Generate join URLs (for frontend integration)
        let mut join_urls = HashMap::new();
        join_urls.insert(
            "patient".to_string(),
            format!("/video/sessions/{}/join?type=patient", session_id),
        );
        join_urls.insert(
            "doctor".to_string(),
            format!("/video/sessions/{}/join?type=doctor", session_id),
        );

        info!("Successfully created video session: {}", session_id);

        Ok(CreateVideoSessionResponse {
            success: true,
            session: video_session,
            join_urls,
            message: "Video session created successfully".to_string(),
        })
    }

    /// Join a video session (patient or doctor)
    pub async fn join_session(
        &self,
        session_id: Uuid,
        request: JoinSessionRequest,
        user: &User,
        auth_token: &str,
    ) -> Result<JoinSessionResponse, VideoConferencingError> {
        info!(
            "User {} joining video session {} as {:?}",
            user.id, session_id, request.user_type
        );

        // Get session from database
        let mut session = self.get_session(session_id, auth_token).await?;

        // Verify user authorization for this session
        self.verify_session_access(&session, user, &request.user_type)?;

        // Check session state
        if !matches!(
            session.status,
            VideoSessionStatus::Scheduled | VideoSessionStatus::Ready | VideoSessionStatus::InProgress
        ) {
            return Err(VideoConferencingError::InvalidSessionState {
                status: format!("{:?}", session.status),
            });
        }

        // Create or get Cloudflare session
        let cloudflare_session_id = if let Some(cf_session_id) = &session.cloudflare_session_id {
            cf_session_id.clone()
        } else {
            // First participant - create Cloudflare session
            if let Some(session_desc) = &request.session_description {
                let cf_response = self
                    .cloudflare
                    .create_session(session_desc.sdp.clone())
                    .await?;

                // Update session with Cloudflare session ID
                session.cloudflare_session_id = Some(cf_response.session_id.clone());
                session.status = VideoSessionStatus::Ready;
                session.updated_at = Utc::now();

                self.update_session_record(&session, auth_token).await?;

                cf_response.session_id
            } else {
                return Err(VideoConferencingError::WebRTCError {
                    message: "Session description required for first participant".to_string(),
                });
            }
        };

        // Record participant joining
        self.record_participant_join(&session, user, &request.user_type, auth_token)
            .await?;

        // Update session status if this is first active participant
        if session.status == VideoSessionStatus::Ready {
            session.status = VideoSessionStatus::InProgress;
            session.actual_start_time = Some(Utc::now());
            session.updated_at = Utc::now();
            self.update_session_record(&session, auth_token).await?;
        }

        info!(
            "Successfully joined session {} - Cloudflare session: {}",
            session_id, cloudflare_session_id
        );

        Ok(JoinSessionResponse {
            success: true,
            cloudflare_session_id,
            session_description: None, // Will be set by subsequent WebRTC negotiations
            ice_servers: self.cloudflare.get_ice_servers(),
            message: "Successfully joined video session".to_string(),
        })
    }

    /// Add tracks to a session (publish local audio/video or request remote tracks)
    pub async fn add_tracks(
        &self,
        session_id: Uuid,
        tracks: Vec<TrackObject>,
        offer_sdp: Option<String>,
        user: &User,
        auth_token: &str,
    ) -> Result<Value, VideoConferencingError> {
        info!(
            "User {} adding {} tracks to session {}",
            user.id,
            tracks.len(),
            session_id
        );

        let session = self.get_session(session_id, auth_token).await?;

        // Verify session has Cloudflare session ID
        let cloudflare_session_id = session
            .cloudflare_session_id
            .as_ref()
            .ok_or_else(|| VideoConferencingError::InvalidSessionState {
                status: "No Cloudflare session initialized".to_string(),
            })?;

        // Add tracks via Cloudflare API
        let track_response = self
            .cloudflare
            .add_tracks(cloudflare_session_id, tracks, offer_sdp)
            .await?;

        info!("Successfully added tracks to session: {}", session_id);

        Ok(json!({
            "success": true,
            "tracks": track_response.tracks,
            "sessionDescription": track_response.session_description,
            "requiresImmediateRenegotiation": track_response.requires_immediate_renegotiation,
            "message": "Tracks added successfully"
        }))
    }

    /// Handle WebRTC renegotiation
    pub async fn renegotiate_session(
        &self,
        session_id: Uuid,
        answer_sdp: String,
        user: &User,
        auth_token: &str,
    ) -> Result<(), VideoConferencingError> {
        info!("Renegotiating session {} for user {}", session_id, user.id);

        let session = self.get_session(session_id, auth_token).await?;

        let cloudflare_session_id = session
            .cloudflare_session_id
            .as_ref()
            .ok_or_else(|| VideoConferencingError::InvalidSessionState {
                status: "No Cloudflare session initialized".to_string(),
            })?;

        self.cloudflare
            .renegotiate_session(cloudflare_session_id, answer_sdp)
            .await?;

        info!("Successfully renegotiated session: {}", session_id);
        Ok(())
    }

    /// End a video session
    pub async fn end_session(
        &self,
        session_id: Uuid,
        user: &User,
        auth_token: &str,
    ) -> Result<VideoSession, VideoConferencingError> {
        info!("Ending video session {} by user {}", session_id, user.id);

        let mut session = self.get_session(session_id, auth_token).await?;

        // Verify user can end session (participant or admin)
        let user_uuid = user.id.parse::<uuid::Uuid>().map_err(|_| {
            VideoConferencingError::ValidationError {
                message: "Invalid user ID format".to_string(),
            }
        })?;
        
        let can_end = session.participant_id == user_uuid
            || session.patient_id.map_or(false, |id| id == user_uuid)
            || session.doctor_id.map_or(false, |id| id == user_uuid)
            || user.role.as_deref() == Some("admin");

        if !can_end {
            return Err(VideoConferencingError::Unauthorized);
        }

        // Update session status
        session.status = VideoSessionStatus::Completed;
        session.actual_end_time = Some(Utc::now());

        // Calculate duration if session was started
        if let Some(start_time) = session.actual_start_time {
            let duration = Utc::now().signed_duration_since(start_time);
            session.session_duration_minutes = Some(duration.num_minutes() as i32);
        }

        session.updated_at = Utc::now();

        // Update in database
        self.update_session_record(&session, auth_token).await?;

        // Record participant leaving
        self.record_participant_leave(&session, user, auth_token)
            .await?;

        // Cleanup Cloudflare session
        if let Some(cf_session_id) = &session.cloudflare_session_id {
            let _ = self.cloudflare.cleanup_session(cf_session_id).await;
        }

        info!("Successfully ended video session: {}", session_id);
        Ok(session)
    }

    /// Get session statistics
    pub async fn get_session_stats(
        &self,
        session_id: Uuid,
        user: &User,
        auth_token: &str,
    ) -> Result<VideoSessionStatsResponse, VideoConferencingError> {
        let session = self.get_session(session_id, auth_token).await?;

        // Verify access
        let user_uuid = user.id.parse::<uuid::Uuid>().map_err(|_| {
            VideoConferencingError::ValidationError {
                message: "Invalid user ID format".to_string(),
            }
        })?;
        
        let has_access = session.participant_id == user_uuid
            || session.patient_id.map_or(false, |id| id == user_uuid)
            || session.doctor_id.map_or(false, |id| id == user_uuid)
            || user.role.as_deref() == Some("admin");

        if !has_access {
            return Err(VideoConferencingError::Unauthorized);
        }

        let participants = self.get_session_participants(session_id, auth_token).await?;

        let mut connection_quality_summary = HashMap::new();
        for participant in &participants {
            if let Some(quality) = &participant.connection_quality {
                let quality_str = format!("{:?}", quality);
                *connection_quality_summary.entry(quality_str).or_insert(0) += 1;
            }
        }

        let total_duration_minutes = session.session_duration_minutes;

        Ok(VideoSessionStatsResponse {
            session,
            participants,
            total_duration_minutes,
            connection_quality_summary,
        })
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

    fn verify_appointment_access(
        &self,
        appointment: &Value,
        user: &User,
    ) -> Result<(), VideoConferencingError> {
        let patient_id = appointment["patient_id"].as_str().unwrap_or("");
        let doctor_id = appointment["doctor_id"].as_str().unwrap_or("");

        let has_access = user.id == patient_id
            || user.id == doctor_id
            || user.role.as_deref() == Some("admin");

        if !has_access {
            return Err(VideoConferencingError::Unauthorized);
        }

        Ok(())
    }

    fn verify_session_access(
        &self,
        session: &VideoSession,
        user: &User,
        user_type: &ParticipantType,
    ) -> Result<(), VideoConferencingError> {
        // Enhanced room-based access verification
        let user_uuid = user.id.parse::<uuid::Uuid>().map_err(|_| {
            VideoConferencingError::ValidationError {
                message: "Invalid user ID format".to_string(),
            }
        })?;
        
        // Check if user is the participant for this session
        if session.participant_id == user_uuid {
            return Ok(());
        }
        
        // Legacy compatibility checks
        match user_type {
            ParticipantType::Patient => {
                if let Some(patient_id) = session.patient_id {
                    if patient_id == user_uuid {
                        return Ok(());
                    }
                }
            }
            ParticipantType::Doctor | ParticipantType::Specialist | ParticipantType::Nurse => {
                if let Some(doctor_id) = session.doctor_id {
                    if doctor_id == user_uuid {
                        return Ok(());
                    }
                }
            }
            _ => {
                // Other participant types (Guardian, Therapist, etc.) use participant_id check above
            }
        }

        Err(VideoConferencingError::Unauthorized)
    }

    /// Store video session with enhanced room-based architecture
    async fn store_session(
        &self,
        session: &VideoSession,
        auth_token: &str,
    ) -> Result<(), VideoConferencingError> {
        let path = "/rest/v1/video_sessions";
        let body = json!({
            "id": session.id,
            "appointment_id": session.appointment_id,
            
            // Enhanced room-based fields
            "room_id": session.room_id,
            "participant_id": session.participant_id,
            "participant_type": session.participant_type,
            
            // Legacy fields for backward compatibility
            "patient_id": session.patient_id,
            "doctor_id": session.doctor_id,
            
            // Session management
            "cloudflare_session_id": session.cloudflare_session_id,
            "status": session.status,
            "session_type": session.session_type,
            "scheduled_start_time": session.scheduled_start_time,
            "actual_start_time": session.actual_start_time,
            "actual_end_time": session.actual_end_time,
            "session_duration_minutes": session.session_duration_minutes,
            "quality_rating": session.quality_rating,
            "connection_issues": session.connection_issues,
            "created_at": session.created_at,
            "updated_at": session.updated_at,
        });

        let _: Vec<Value> = self
            .supabase
            .request(Method::POST, path, Some(auth_token), Some(body))
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: format!("Failed to store video session: {}", e),
            })?;

        Ok(())
    }
    
    /// Create or get existing room for appointment
    async fn ensure_room_exists(
        &self,
        appointment_id: Uuid,
        session_type: &crate::models::VideoSessionType,
        auth_token: &str,
    ) -> Result<String, VideoConferencingError> {
        // Check if room already exists for this appointment
        let check_path = format!("/rest/v1/video_rooms?appointment_id=eq.{}", appointment_id);
        let existing_rooms: Vec<Value> = self
            .supabase
            .request(Method::GET, &check_path, Some(auth_token), None)
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: format!("Failed to check existing rooms: {}", e),
            })?;
            
        if let Some(room) = existing_rooms.first() {
            // Room exists, return its ID
            room["id"].as_str()
                .ok_or_else(|| VideoConferencingError::DatabaseError {
                    message: "Room ID not found in database response".to_string(),
                })
                .map(|s| s.to_string())
        } else {
            // Create new room using database function
            let create_path = "/rest/v1/rpc/create_default_room_for_appointment";
            let create_body = json!({
                "appointment_uuid": appointment_id,
                "session_type": session_type
            });
            
            let room_id_response: String = self
                .supabase
                .request(Method::POST, create_path, Some(auth_token), Some(create_body))
                .await
                .map_err(|e| VideoConferencingError::DatabaseError {
                    message: format!("Failed to create room: {}", e),
                })?;
                
            Ok(room_id_response)
        }
    }

    async fn get_session(
        &self,
        session_id: Uuid,
        auth_token: &str,
    ) -> Result<VideoSession, VideoConferencingError> {
        let path = format!("/rest/v1/video_sessions?id=eq.{}", session_id);
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

    async fn update_session_record(
        &self,
        session: &VideoSession,
        auth_token: &str,
    ) -> Result<(), VideoConferencingError> {
        let path = format!("/rest/v1/video_sessions?id=eq.{}", session.id);
        let body = json!({
            "cloudflare_session_id": session.cloudflare_session_id,
            "status": session.status,
            "actual_start_time": session.actual_start_time,
            "actual_end_time": session.actual_end_time,
            "session_duration_minutes": session.session_duration_minutes,
            "quality_rating": session.quality_rating,
            "connection_issues": session.connection_issues,
            "updated_at": session.updated_at,
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

    async fn record_participant_join(
        &self,
        session: &VideoSession,
        user: &User,
        user_type: &ParticipantType,
        auth_token: &str,
    ) -> Result<(), VideoConferencingError> {
        let path = "/rest/v1/video_session_participants";
        let body = json!({
            "session_id": session.id,
            "user_id": user.id,
            "user_type": user_type,
            "joined_at": Utc::now(),
            "audio_enabled": true,
            "video_enabled": true,
        });

        let _: Vec<Value> = self
            .supabase
            .request(Method::POST, path, Some(auth_token), Some(body))
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: e.to_string(),
            })?;

        Ok(())
    }

    async fn record_participant_leave(
        &self,
        session: &VideoSession,
        user: &User,
        auth_token: &str,
    ) -> Result<(), VideoConferencingError> {
        let path = format!(
            "/rest/v1/video_session_participants?session_id=eq.{}&user_id=eq.{}",
            session.id, user.id
        );
        let body = json!({
            "left_at": Utc::now(),
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

    async fn get_session_participants(
        &self,
        session_id: Uuid,
        auth_token: &str,
    ) -> Result<Vec<SessionParticipant>, VideoConferencingError> {
        let path = format!("/rest/v1/video_session_participants?session_id=eq.{}", session_id);
        let result: Vec<Value> = self
            .supabase
            .request(Method::GET, &path, Some(auth_token), None)
            .await
            .map_err(|e| VideoConferencingError::DatabaseError {
                message: e.to_string(),
            })?;

        result
            .into_iter()
            .map(|p| {
                serde_json::from_value(p).map_err(|e| VideoConferencingError::DatabaseError {
                    message: format!("Failed to parse participant: {}", e),
                })
            })
            .collect()
    }
}