// libs/video-conferencing-cell/src/handlers.rs
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State, Extension},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::{json, Value};
use serde::Deserialize;
use uuid::Uuid;

use shared_config::AppConfig;
use shared_models::auth::User;
use shared_models::error::AppError;

use crate::models::{
    AddTracksRequest, CreateVideoSessionRequest, JoinSessionRequest, 
    VideoConferencingError, VideoSessionType
};
use crate::services::{VideoSessionService, VideoConferencingIntegrationService};

// ==============================================================================
// QUERY PARAMETER STRUCTS
// ==============================================================================

#[derive(Debug, Deserialize)]
pub struct UpcomingSessionsQuery {
    pub hours_ahead: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SessionStatsQuery {
    pub include_participants: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct RenegotiateRequest {
    pub answer_sdp: String,
}

// ==============================================================================
// VIDEO SESSION MANAGEMENT HANDLERS
// ==============================================================================

/// Create a new video session for an appointment
#[axum::debug_handler]
pub async fn create_video_session(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<CreateVideoSessionRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let session_service = VideoSessionService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let response = session_service
        .create_session(request, &user, token)
        .await
        .map_err(|e| match e {
            VideoConferencingError::InvalidAppointment => {
                AppError::NotFound("Appointment not found".to_string())
            }
            VideoConferencingError::Unauthorized => {
                AppError::Auth("Not authorized for this appointment".to_string())
            }
            VideoConferencingError::ValidationError { message } => {
                AppError::BadRequest(message)
            }
            VideoConferencingError::NotConfigured => {
                AppError::Internal("Video conferencing not configured".to_string())
            }
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": response.success,
        "session": response.session,
        "join_urls": response.join_urls,
        "message": response.message
    })))
}

/// Join a video session
#[axum::debug_handler]
pub async fn join_video_session(
    State(state): State<Arc<AppConfig>>,
    Path(session_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<JoinSessionRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let session_service = VideoSessionService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let response = session_service
        .join_session(session_id, request, &user, token)
        .await
        .map_err(|e| match e {
            VideoConferencingError::SessionNotFound => {
                AppError::NotFound("Video session not found".to_string())
            }
            VideoConferencingError::Unauthorized => {
                AppError::Auth("Not authorized for this session".to_string())
            }
            VideoConferencingError::InvalidSessionState { status } => {
                AppError::BadRequest(format!("Session not available: {}", status))
            }
            VideoConferencingError::WebRTCError { message } => {
                AppError::BadRequest(format!("WebRTC error: {}", message))
            }
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": response.success,
        "cloudflare_session_id": response.cloudflare_session_id,
        "session_description": response.session_description,
        "ice_servers": response.ice_servers,
        "message": response.message
    })))
}

/// Add tracks to a video session
#[axum::debug_handler]
pub async fn add_session_tracks(
    State(state): State<Arc<AppConfig>>,
    Path(session_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<AddTracksRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let session_service = VideoSessionService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let response = session_service
        .add_tracks(
            session_id,
            request.tracks,
            request.session_description.map(|sd| sd.sdp),
            &user,
            token,
        )
        .await
        .map_err(|e| match e {
            VideoConferencingError::SessionNotFound => {
                AppError::NotFound("Video session not found".to_string())
            }
            VideoConferencingError::InvalidSessionState { status } => {
                AppError::BadRequest(format!("Cannot add tracks: {}", status))
            }
            VideoConferencingError::CloudflareApiError { message } => {
                AppError::Internal(format!("Cloudflare error: {}", message))
            }
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(response))
}

/// Handle WebRTC renegotiation
#[axum::debug_handler]
pub async fn renegotiate_session(
    State(state): State<Arc<AppConfig>>,
    Path(session_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<RenegotiateRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let session_service = VideoSessionService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    session_service
        .renegotiate_session(session_id, request.answer_sdp, &user, token)
        .await
        .map_err(|e| match e {
            VideoConferencingError::SessionNotFound => {
                AppError::NotFound("Video session not found".to_string())
            }
            VideoConferencingError::InvalidSessionState { status } => {
                AppError::BadRequest(format!("Cannot renegotiate: {}", status))
            }
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": true,
        "message": "Session renegotiated successfully"
    })))
}

/// End a video session
#[axum::debug_handler]
pub async fn end_video_session(
    State(state): State<Arc<AppConfig>>,
    Path(session_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let session_service = VideoSessionService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let session = session_service
        .end_session(session_id, &user, token)
        .await
        .map_err(|e| match e {
            VideoConferencingError::SessionNotFound => {
                AppError::NotFound("Video session not found".to_string())
            }
            VideoConferencingError::Unauthorized => {
                AppError::Auth("Not authorized to end this session".to_string())
            }
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": true,
        "session": session,
        "message": "Video session ended successfully"
    })))
}

/// Get video session details
#[axum::debug_handler]
pub async fn get_video_session(
    State(state): State<Arc<AppConfig>>,
    Path(session_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Query(query): Query<SessionStatsQuery>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let session_service = VideoSessionService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    if query.include_participants.unwrap_or(false) {
        let stats = session_service
            .get_session_stats(session_id, &user, token)
            .await
            .map_err(|e| match e {
                VideoConferencingError::SessionNotFound => {
                    AppError::NotFound("Video session not found".to_string())
                }
                VideoConferencingError::Unauthorized => {
                    AppError::Auth("Not authorized to view this session".to_string())
                }
                _ => AppError::Internal(e.to_string()),
            })?;
        
        Ok(Json(json!(stats)))
    } else {
        // Just return basic session info - implement basic get_session method
        Err(AppError::Internal("Basic session retrieval not yet implemented".to_string()))
    }
}

// ==============================================================================
// APPOINTMENT INTEGRATION HANDLERS
// ==============================================================================

/// Create video session for existing appointment
#[axum::debug_handler]
pub async fn create_session_for_appointment(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(session_type): Json<VideoSessionType>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let integration_service = VideoConferencingIntegrationService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let session = integration_service
        .create_session_for_appointment(appointment_id, session_type, &user, token)
        .await
        .map_err(|e| match e {
            VideoConferencingError::InvalidAppointment => {
                AppError::NotFound("Appointment not found".to_string())
            }
            VideoConferencingError::ValidationError { message } => {
                AppError::BadRequest(message)
            }
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": true,
        "session": session,
        "message": "Video session created for appointment"
    })))
}

/// Get upcoming video sessions
#[axum::debug_handler]
pub async fn get_upcoming_sessions(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Query(query): Query<UpcomingSessionsQuery>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let hours_ahead = query.hours_ahead.unwrap_or(24);
    
    let integration_service = VideoConferencingIntegrationService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let sessions = integration_service
        .get_upcoming_sessions(&user, hours_ahead, token)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "upcoming_sessions": sessions,
        "total": sessions.len(),
        "hours_ahead": hours_ahead
    })))
}

/// Check if video conferencing is available for an appointment
#[axum::debug_handler]
pub async fn check_video_availability(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(_user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let integration_service = VideoConferencingIntegrationService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let is_available = integration_service
        .is_video_available_for_appointment(appointment_id, token)
        .await
        .map_err(|e| match e {
            VideoConferencingError::InvalidAppointment => {
                AppError::NotFound("Appointment not found".to_string())
            }
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "video_available": is_available,
        "appointment_id": appointment_id,
        "configured": state.is_video_conferencing_configured()
    })))
}

/// Get video stats for an appointment
#[axum::debug_handler]
pub async fn get_appointment_video_stats(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let integration_service = VideoConferencingIntegrationService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let stats = integration_service
        .get_appointment_video_stats(appointment_id, &user, token)
        .await
        .map_err(|e| match e {
            VideoConferencingError::SessionNotFound => {
                AppError::NotFound("No video session found for this appointment".to_string())
            }
            VideoConferencingError::Unauthorized => {
                AppError::Auth("Not authorized to view video stats".to_string())
            }
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(stats))
}

// ==============================================================================
// SYSTEM ADMINISTRATION HANDLERS
// ==============================================================================

/// Health check for video conferencing system
#[axum::debug_handler]
pub async fn video_health_check(
    State(state): State<Arc<AppConfig>>,
) -> Result<Json<Value>, AppError> {
    if !state.is_video_conferencing_configured() {
        return Ok(Json(json!({
            "status": "not_configured",
            "video_configured": false,
            "message": "Video conferencing not configured"
        })));
    }
    
    let cloudflare_client = crate::services::CloudflareRealtimeClient::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let cloudflare_healthy = cloudflare_client
        .health_check()
        .await
        .unwrap_or(false);
    
    Ok(Json(json!({
        "status": if cloudflare_healthy { "healthy" } else { "unhealthy" },
        "video_configured": true,
        "cloudflare_status": if cloudflare_healthy { "connected" } else { "error" },
        "message": if cloudflare_healthy {
            "Video conferencing system is operational"
        } else {
            "Video conferencing system has connectivity issues"
        }
    })))
}

/// Admin: Cleanup expired sessions
#[axum::debug_handler]
pub async fn cleanup_expired_sessions(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    // Verify admin access
    if user.role.as_deref() != Some("admin") {
        return Err(AppError::Auth("Admin access required".to_string()));
    }
    
    let token = auth.token();
    
    let integration_service = VideoConferencingIntegrationService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let cleaned_count = integration_service
        .cleanup_expired_sessions(token)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "success": true,
        "cleaned_sessions": cleaned_count,
        "message": format!("Cleaned up {} expired sessions", cleaned_count)
    })))
}