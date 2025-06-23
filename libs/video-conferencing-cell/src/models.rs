// libs/video-conferencing-cell/src/models.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ==============================================================================
// VIDEO CONFERENCING DOMAIN MODELS
// ==============================================================================

/// Enhanced VideoSession model with room-based architecture
/// Supports multi-participant medical consultations and enterprise video conferencing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSession {
    pub id: Uuid,
    pub appointment_id: Uuid,
    
    // Enhanced room-based architecture
    pub room_id: String,                    // Logical meeting room identifier
    pub participant_id: Uuid,               // Individual participant in the room
    pub participant_type: ParticipantType,  // Role: patient, doctor, specialist, guardian
    
    // Legacy fields maintained for compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patient_id: Option<Uuid>,           // Deprecated: use participant_id + type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctor_id: Option<Uuid>,            // Deprecated: use participant_id + type
    
    // WebRTC and session management
    pub cloudflare_session_id: Option<String>, // Per-participant WebRTC session
    
    // Enhanced Cloudflare Realtime track support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloudflare_track_id: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_type: Option<TrackType>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<MediaType>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_metadata: Option<serde_json::Value>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_state: Option<ConnectionState>,
    
    // Session lifecycle
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

/// Enhanced video session types for comprehensive medical scenarios
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VideoSessionType {
    #[serde(rename = "consultation")]
    Consultation,           // Standard 1:1 doctor-patient
    #[serde(rename = "follow_up")]
    FollowUp,              // Follow-up appointments
    #[serde(rename = "emergency")]
    Emergency,             // Emergency consultations
    #[serde(rename = "specialist_consult")]
    SpecialistConsult,     // Multi-doctor specialist consultation
    #[serde(rename = "group_therapy")]
    GroupTherapy,          // Group mental health sessions
    #[serde(rename = "family_consult")]
    FamilyConsult,         // Patient + family members
    #[serde(rename = "team_meeting")]
    TeamMeeting,           // Multidisciplinary team conference
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

/// Enhanced participant types for comprehensive medical video conferencing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ParticipantType {
    #[serde(rename = "patient")]
    Patient,
    #[serde(rename = "doctor")]
    Doctor,
    #[serde(rename = "specialist")]
    Specialist,            // Specialist doctors (cardiologist, neurologist, etc.)
    #[serde(rename = "nurse")]
    Nurse,                 // Nursing staff
    #[serde(rename = "guardian")]
    Guardian,              // Parent/guardian for pediatric patients
    #[serde(rename = "therapist")]
    Therapist,             // Mental health professionals
    #[serde(rename = "coordinator")]
    Coordinator,           // Medical coordinators/administrators
    #[serde(rename = "interpreter")]
    Interpreter,           // Language interpreters
    #[serde(rename = "observer")]
    Observer,              // Medical students, observers
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

/// Cloudflare Realtime track types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrackType {
    #[serde(rename = "send")]
    Send,              // Publishing local media (microphone, camera)
    #[serde(rename = "receive")]
    Receive,           // Subscribing to remote media
    #[serde(rename = "bidirectional")]
    Bidirectional,     // Both send and receive
}

/// Media stream types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MediaType {
    #[serde(rename = "audio")]
    Audio,
    #[serde(rename = "video")]
    Video,
    #[serde(rename = "screen")]
    Screen,            // Screen sharing
    #[serde(rename = "application")]
    Application,       // Application sharing
    #[serde(rename = "data")]
    Data,              // Data channels
}

/// WebRTC connection states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionState {
    #[serde(rename = "connecting")]
    Connecting,
    #[serde(rename = "connected")]
    Connected,
    #[serde(rename = "disconnected")]
    Disconnected,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "closed")]
    Closed,
}

/// Dedicated Cloudflare track record - aligned with database schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareTrack {
    pub id: Uuid,
    pub session_id: Uuid,                  // References video_sessions.id
    pub cloudflare_track_id: String,       // Cloudflare's track identifier
    pub cloudflare_session_id: String,     // Cloudflare's session identifier
    
    // Track properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_name: Option<String>,
    pub track_type: TrackType,
    pub media_type: MediaType,
    
    // Media settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,         // e.g., "1920x1080"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<i32>,
    
    // State management
    pub track_state: TrackState,
    
    // Metadata and timing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Track state management
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrackState {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "inactive")]
    Inactive,
    #[serde(rename = "ended")]
    Ended,
    #[serde(rename = "failed")]
    Failed,
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
// ROOM MANAGEMENT MODELS
// ==============================================================================

/// Video Room - Logical meeting space for medical consultations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoRoom {
    pub id: String,                          // room_id
    pub appointment_id: Uuid,
    pub room_type: VideoSessionType,
    pub max_participants: i32,
    pub waiting_room_enabled: bool,
    pub recording_enabled: bool,
    pub room_status: RoomStatus,
    pub security_config: RoomSecurityConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoomStatus {
    #[serde(rename = "scheduled")]
    Scheduled,
    #[serde(rename = "waiting")]
    Waiting,               // Waiting room active, participants can join lobby
    #[serde(rename = "active")]
    Active,                // Meeting in progress
    #[serde(rename = "ended")]
    Ended,
    #[serde(rename = "cancelled")]
    Cancelled,
}

/// Room security and access control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSecurityConfig {
    pub admission_control: AdmissionPolicy,
    pub recording_permissions: RecordingPolicy,
    pub participant_permissions: HashMap<ParticipantType, ParticipantPermissions>,
    pub hipaa_compliance_level: HIPAALevel,
    pub end_to_end_encryption: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdmissionPolicy {
    #[serde(rename = "open")]
    Open,                  // Anyone with room link can join
    #[serde(rename = "waiting_room")]
    WaitingRoom,           // Participants wait for host approval
    #[serde(rename = "restricted")]
    Restricted,            // Only pre-approved participants
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecordingPolicy {
    #[serde(rename = "disabled")]
    Disabled,
    #[serde(rename = "host_only")]
    HostOnly,              // Only doctors can record
    #[serde(rename = "all_participants")]
    AllParticipants,       // Any participant can record
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantPermissions {
    pub can_share_screen: bool,
    pub can_share_audio: bool,
    pub can_share_video: bool,
    pub can_record: bool,
    pub can_admit_participants: bool,
    pub can_remove_participants: bool,
    pub can_end_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HIPAALevel {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "enhanced")]
    Enhanced,
    #[serde(rename = "maximum")]
    Maximum,
}

// ==============================================================================
// API REQUEST/RESPONSE MODELS
// ==============================================================================

/// Enhanced create video session request with room support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVideoSessionRequest {
    #[serde(with = "uuid_serde_flexible")]
    pub appointment_id: Uuid,
    
    // Room configuration (optional - will generate if not provided)
    pub room_id: Option<String>,
    pub room_type: Option<VideoSessionType>,
    pub max_participants: Option<i32>,
    
    // Participant information
    pub participant_type: ParticipantType,
    pub session_type: VideoSessionType,
    pub scheduled_start_time: DateTime<Utc>,
}

// Flexible UUID deserializer that handles both string UUIDs and validates them properly
mod uuid_serde_flexible {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use uuid::Uuid;
    
    pub fn serialize<S>(uuid: &Uuid, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        uuid.serialize(serializer)
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Uuid, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Uuid::parse_str(&s).map_err(|e| serde::de::Error::custom(format!("Invalid UUID format '{}': {}", s, e)))
    }
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