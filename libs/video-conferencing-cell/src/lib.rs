// libs/video-conferencing-cell/src/lib.rs
//! # Video Conferencing Cell
//! 
//! This cell provides video conferencing functionality using Cloudflare's Realtime API
//! for WebRTC-based video calls between patients and doctors.
//! 
//! ## Features
//! 
//! - **WebRTC Video Sessions**: Direct patient-doctor video calls
//! - **Cloudflare Realtime Integration**: Serverless SFU for reliable connections  
//! - **Appointment Integration**: Automatic session creation tied to appointments
//! - **Session Management**: Join, leave, track management, renegotiation
//! - **Quality Monitoring**: Connection quality tracking and statistics
//! - **Access Control**: Role-based authorization for sessions
//! 
//! ## Architecture
//! 
//! The video conferencing cell follows the established cell architecture pattern:
//! 
//! ```text
//! +-----------------------------------------------------+
//! |                   Video Cell                        |
//! +-----------------------------------------------------+
//! |  handlers.rs    |  HTTP endpoint handlers           |
//! |  router.rs      |  Route definitions                |
//! |  models.rs      |  Data structures & DTOs           |
//! |  services/      |  Business logic layer             |
//! |    cloudflare.rs|  Cloudflare Realtime API client   |
//! |    session.rs   |  Video session management         |
//! |    integration.rs| Appointment system integration   |
//! +-----------------------------------------------------+
//! ```
//! 
//! ## API Endpoints
//! 
//! ### Video Session Management
//! - `POST /video/sessions` - Create new video session
//! - `GET /video/sessions/{id}` - Get session details
//! - `POST /video/sessions/{id}/join` - Join session
//! - `POST /video/sessions/{id}/tracks` - Add audio/video tracks
//! - `PUT /video/sessions/{id}/renegotiate` - Handle WebRTC renegotiation
//! - `DELETE /video/sessions/{id}/end` - End session
//! 
//! ### Appointment Integration
//! - `POST /video/appointments/{id}/session` - Create session for appointment
//! - `GET /video/appointments/{id}/availability` - Check video availability
//! - `GET /video/appointments/{id}/stats` - Get video statistics
//! 
//! ### User Management
//! - `GET /video/upcoming` - Get upcoming sessions
//! 
//! ### System Administration
//! - `GET /video/health` - Health check
//! - `POST /video/admin/cleanup` - Cleanup expired sessions
//! 
//! ## Usage Example
//! 
//! ```rust
//! use video_conferencing_cell::router::video_conferencing_routes;
//! use shared_config::AppConfig;
//! use std::sync::Arc;
//! 
//! let config = Arc::new(AppConfig::from_env());
//! let video_routes = video_conferencing_routes(config);
//! ```
//! 
//! ## Configuration
//! 
//! Required environment variables:
//! - `CLOUDFLARE_REALTIME_APP_ID` - Cloudflare app identifier
//! - `CLOUDFLARE_REALTIME_API_TOKEN` - API authentication token
//! - `CLOUDFLARE_REALTIME_BASE_URL` - API base URL (optional, defaults to production)
//! 
//! ## Integration with Appointment Cell
//! 
//! The video conferencing cell integrates seamlessly with the appointment system:
//! 
//! ```rust,no_run
//! use video_conferencing_cell::services::VideoConferencingIntegrationService;
//! use shared_config::AppConfig;
//! use std::sync::Arc;
//! use uuid::Uuid;
//! 
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = Arc::new(AppConfig::from_env());
//! let appointment_id = Uuid::new_v4();
//! let token = "token";
//! 
//! // Automatically create video session when appointment is confirmed
//! let integration = VideoConferencingIntegrationService::new(&config)?;
//! integration.handle_appointment_status_change(appointment_id, "confirmed", token).await?;
//! # Ok(())
//! # }
//! ```

pub mod handlers;
pub mod models;
pub mod router;
pub mod services;

// Re-export commonly used types
pub use models::{
    VideoSession, VideoSessionStatus, VideoSessionType, ParticipantType,
    CreateVideoSessionRequest, CreateVideoSessionResponse,
    JoinSessionRequest, JoinSessionResponse,
    VideoConferencingError
};

pub use services::{
    CloudflareRealtimeClient,
    VideoSessionService,
    VideoConferencingIntegrationService
};

pub use router::video_conferencing_routes;