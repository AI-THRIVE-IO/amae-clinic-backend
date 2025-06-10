// libs/video-conferencing-cell/src/models.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ==============================================================================
// VIDEO CONFERENCING DOMAIN MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSession {
    pub id: Uuid,
    pub appointment_id: Uuid,
    pub patient_id: Uuid,
    pub doctor_id: Uuid,
    pub cloudflare_session_id: Option<String>,
    pub status: VideoSessionStatus,
    pub session_type: VideoSessionType,
    pub scheduled_start_time: DateTime<Utc>,
    pub actual_start_time: Option<DateTime<Utc>>,
    pub actual_end_time: Option<DateTime<Utc>>,
    pub session_duration_minutes: Option<i32>,
    pub quality_rating: Option<i32>, // 1-5 stars
    pub connection_issues: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VideoSessionStatus {
    #[serde(rename = "scheduled")]
    Scheduled,
    #[serde(rename = "ready")]
    Ready,           // Cloudflare session created, waiting for participants
    #[serde(rename = "in_progress")]
    InProgress,      // At least one participant joined
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VideoSessionType {
    #[serde(rename = "consultation")]
    Consultation,
    #[serde(rename = "follow_up")]
    FollowUp,
    #[serde(rename = "emergency")]
    Emergency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionParticipant {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub user_type: ParticipantType,
    pub joined_at: Option<DateTime<Utc>>,
    pub left_at: Option<DateTime<Utc>>,
    pub connection_quality: Option<ConnectionQuality>,
    pub audio_enabled: bool,
    pub video_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticipantType {
    #[serde(rename = "patient")]
    Patient,
    #[serde(rename = "doctor")]
    Doctor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionQuality {
    #[serde(rename = "excellent")]
    Excellent,
    #[serde(rename = "good")]
    Good,
    #[serde(rename = "fair")]
    Fair,
    #[serde(rename = "poor")]
    Poor,
}

// ==============================================================================
// CLOUDFLARE REALTIME API MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareSessionRequest {
    #[serde(rename = "sessionDescription")]
    pub session_description: SessionDescription,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareSessionResponse {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "sessionDescription")]
    pub session_description: SessionDescription,
    #[serde(rename = "errorCode", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(rename = "errorDescription", skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescription {
    #[serde(rename = "type")]
    pub sdp_type: String, // "offer" or "answer"
    pub sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareTrackRequest {
    #[serde(rename = "sessionDescription", skip_serializing_if = "Option::is_none")]
    pub session_description: Option<SessionDescription>,
    pub tracks: Vec<TrackObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareTrackResponse {
    #[serde(rename = "sessionDescription", skip_serializing_if = "Option::is_none")]
    pub session_description: Option<SessionDescription>,
    #[serde(rename = "requiresImmediateRenegotiation", skip_serializing_if = "Option::is_none")]
    pub requires_immediate_renegotiation: Option<bool>,
    pub tracks: Vec<TrackResult>,
    #[serde(rename = "errorCode", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(rename = "errorDescription", skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackObject {
    pub location: String, // "local" or "remote"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid: Option<String>,
    #[serde(rename = "trackName", skip_serializing_if = "Option::is_none")]
    pub track_name: Option<String>,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackResult {
    pub mid: String,
    #[serde(rename = "trackName")]
    pub track_name: String,
    #[serde(rename = "errorCode", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(rename = "errorDescription", skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareRenegotiateRequest {
    #[serde(rename = "sessionDescription")]
    pub session_description: SessionDescription,
}

// ==============================================================================
// API REQUEST/RESPONSE MODELS
// ==============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVideoSessionRequest {
    pub appointment_id: Uuid,
    pub session_type: VideoSessionType,
    pub scheduled_start_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CreateVideoSessionResponse {
    pub success: bool,
    pub session: VideoSession,
    pub join_urls: HashMap<String, String>, // participant_type -> join_url
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct JoinSessionRequest {
    pub user_type: ParticipantType,
    #[serde(rename = "sessionDescription", skip_serializing_if = "Option::is_none")]
    pub session_description: Option<SessionDescription>,
}

#[derive(Debug, Serialize)]
pub struct JoinSessionResponse {
    pub success: bool,
    pub cloudflare_session_id: String,
    #[serde(rename = "sessionDescription", skip_serializing_if = "Option::is_none")]
    pub session_description: Option<SessionDescription>,
    pub ice_servers: Vec<IceServer>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddTracksRequest {
    pub tracks: Vec<TrackObject>,
    #[serde(rename = "sessionDescription", skip_serializing_if = "Option::is_none")]
    pub session_description: Option<SessionDescription>,
}

#[derive(Debug, Serialize)]
pub struct AddTracksResponse {
    pub success: bool,
    pub tracks: Vec<TrackResult>,
    #[serde(rename = "sessionDescription", skip_serializing_if = "Option::is_none")]
    pub session_description: Option<SessionDescription>,
    #[serde(rename = "requiresImmediateRenegotiation", skip_serializing_if = "Option::is_none")]
    pub requires_immediate_renegotiation: Option<bool>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionRequest {
    pub status: Option<VideoSessionStatus>,
    pub quality_rating: Option<i32>,
    pub connection_issues: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct VideoSessionStatsResponse {
    pub session: VideoSession,
    pub participants: Vec<SessionParticipant>,
    pub total_duration_minutes: Option<i32>,
    pub connection_quality_summary: HashMap<String, i32>,
}

// ==============================================================================
// ERROR HANDLING
// ==============================================================================

#[derive(Debug, thiserror::Error)]
pub enum VideoConferencingError {
    #[error("Video session not found")]
    SessionNotFound,
    
    #[error("Appointment not found or invalid")]
    InvalidAppointment,
    
    #[error("User not authorized for this video session")]
    Unauthorized,
    
    #[error("Video session is not in a state that allows this operation: {status}")]
    InvalidSessionState { status: String },
    
    #[error("Cloudflare API error: {message}")]
    CloudflareApiError { message: String },
    
    #[error("WebRTC configuration error: {message}")]
    WebRTCError { message: String },
    
    #[error("Session capacity exceeded")]
    SessionCapacityExceeded,
    
    #[error("Video conferencing not configured")]
    NotConfigured,
    
    #[error("Database error: {message}")]
    DatabaseError { message: String },
    
    #[error("Validation error: {message}")]
    ValidationError { message: String },
    
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl From<anyhow::Error> for VideoConferencingError {
    fn from(err: anyhow::Error) -> Self {
        VideoConferencingError::Internal {
            message: err.to_string(),
        }
    }
}

impl From<reqwest::Error> for VideoConferencingError {
    fn from(err: reqwest::Error) -> Self {
        VideoConferencingError::CloudflareApiError {
            message: err.to_string(),
        }
    }
}