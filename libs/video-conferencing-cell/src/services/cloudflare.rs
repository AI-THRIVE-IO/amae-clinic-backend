// libs/video-conferencing-cell/src/services/cloudflare.rs
use anyhow::Result;
use reqwest::Client;
use tracing::{debug, error, info, warn};

use shared_config::AppConfig;

use crate::models::{
    CloudflareRenegotiateRequest, CloudflareSessionRequest, CloudflareSessionResponse,
    CloudflareTrackRequest, CloudflareTrackResponse, IceServer, SessionDescription,
    TrackObject, VideoConferencingError,
};

/// Cloudflare Realtime API client for managing WebRTC sessions and tracks
/// Based on: https://developers.cloudflare.com/realtime/
pub struct CloudflareRealtimeClient {
    client: Client,
    app_id: String,
    api_token: String,
    base_url: String,
}

impl CloudflareRealtimeClient {
    pub fn new(config: &AppConfig) -> Result<Self, VideoConferencingError> {
        if !config.is_video_conferencing_configured() {
            return Err(VideoConferencingError::NotConfigured);
        }

        let client = Client::new();

        Ok(Self {
            client,
            app_id: config.cloudflare_realtime_app_id.clone(),
            api_token: config.cloudflare_realtime_api_token.clone(),
            base_url: config.cloudflare_realtime_base_url.clone(),
        })
    }

    /// Create a new WebRTC session with initial offer SDP
    /// POST /v1/apps/{appId}/sessions/new
    pub async fn create_session(
        &self,
        offer_sdp: String,
    ) -> Result<CloudflareSessionResponse, VideoConferencingError> {
        info!("Creating new Cloudflare Realtime session");

        let url = format!("{}/apps/{}/sessions/new", self.base_url, self.app_id);

        let request_body = CloudflareSessionRequest {
            session_description: SessionDescription {
                sdp_type: "offer".to_string(),
                sdp: offer_sdp,
            },
        };

        debug!("Sending session creation request to: {}", url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        let response_text = response.text().await?;

        debug!("Cloudflare session creation response: {} - {}", status, response_text);

        if !status.is_success() {
            error!("Cloudflare session creation failed: {} - {}", status, response_text);
            return Err(VideoConferencingError::CloudflareApiError {
                message: format!("HTTP {}: {}", status, response_text),
            });
        }

        let session_response: CloudflareSessionResponse = serde_json::from_str(&response_text)
            .map_err(|e| VideoConferencingError::CloudflareApiError {
                message: format!("Failed to parse session response: {}", e),
            })?;

        self.check_session_errors(&session_response)?;

        info!("Successfully created Cloudflare session: {}", session_response.session_id);
        Ok(session_response)
    }

    /// Add new tracks to an existing session (publish local tracks or request remote tracks)
    /// POST /v1/apps/{appId}/sessions/{sessionId}/tracks/new
    pub async fn add_tracks(
        &self,
        session_id: &str,
        tracks: Vec<TrackObject>,
        offer_sdp: Option<String>,
    ) -> Result<CloudflareTrackResponse, VideoConferencingError> {
        info!("Adding {} tracks to session: {}", tracks.len(), session_id);

        let url = format!(
            "{}/apps/{}/sessions/{}/tracks/new",
            self.base_url, self.app_id, session_id
        );

        let mut request_body = CloudflareTrackRequest {
            session_description: None,
            tracks,
        };

        if let Some(sdp) = offer_sdp {
            request_body.session_description = Some(SessionDescription {
                sdp_type: "offer".to_string(),
                sdp,
            });
        }

        debug!("Sending add tracks request to: {}", url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        let response_text = response.text().await?;

        debug!("Cloudflare add tracks response: {} - {}", status, response_text);

        if !status.is_success() {
            error!("Cloudflare add tracks failed: {} - {}", status, response_text);
            return Err(VideoConferencingError::CloudflareApiError {
                message: format!("HTTP {}: {}", status, response_text),
            });
        }

        let track_response: CloudflareTrackResponse = serde_json::from_str(&response_text)
            .map_err(|e| VideoConferencingError::CloudflareApiError {
                message: format!("Failed to parse track response: {}", e),
            })?;

        self.check_track_errors(&track_response)?;

        info!("Successfully added tracks to session: {}", session_id);
        Ok(track_response)
    }

    /// Send answer SDP for session renegotiation
    /// PUT /v1/apps/{appId}/sessions/{sessionId}/renegotiate
    pub async fn renegotiate_session(
        &self,
        session_id: &str,
        answer_sdp: String,
    ) -> Result<(), VideoConferencingError> {
        info!("Renegotiating session: {}", session_id);

        let url = format!(
            "{}/apps/{}/sessions/{}/renegotiate",
            self.base_url, self.app_id, session_id
        );

        let request_body = CloudflareRenegotiateRequest {
            session_description: SessionDescription {
                sdp_type: "answer".to_string(),
                sdp: answer_sdp,
            },
        };

        debug!("Sending renegotiation request to: {}", url);

        let response = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let response_text = response.text().await?;
            error!("Cloudflare renegotiation failed: {} - {}", status, response_text);
            return Err(VideoConferencingError::CloudflareApiError {
                message: format!("HTTP {}: {}", status, response_text),
            });
        }

        info!("Successfully renegotiated session: {}", session_id);
        Ok(())
    }

    /// Get ICE servers configuration for WebRTC peer connection
    /// Uses Cloudflare's STUN server as recommended
    pub fn get_ice_servers(&self) -> Vec<IceServer> {
        vec![IceServer {
            urls: vec!["stun:stun.cloudflare.com:3478".to_string()],
            username: None,
            credential: None,
        }]
    }

    /// Generate WebRTC configuration for client-side peer connection
    pub fn get_rtc_configuration(&self) -> serde_json::Value {
        serde_json::json!({
            "iceServers": self.get_ice_servers(),
            "bundlePolicy": "max-bundle"
        })
    }

    /// Delete a session (cleanup)
    /// Note: Cloudflare doesn't have explicit session deletion in their current API
    /// Sessions automatically expire after inactivity
    pub async fn cleanup_session(&self, session_id: &str) -> Result<(), VideoConferencingError> {
        info!("Cleaning up session: {} (automatic expiration)", session_id);
        // Cloudflare sessions auto-expire, so this is just for logging
        Ok(())
    }

    /// Check for errors in session response
    fn check_session_errors(
        &self,
        response: &CloudflareSessionResponse,
    ) -> Result<(), VideoConferencingError> {
        if let Some(error_code) = &response.error_code {
            let message = response
                .error_description
                .as_deref()
                .unwrap_or("Unknown error");
            error!("Cloudflare session error: {} - {}", error_code, message);
            return Err(VideoConferencingError::CloudflareApiError {
                message: format!("{}: {}", error_code, message),
            });
        }
        Ok(())
    }

    /// Check for errors in track response
    fn check_track_errors(
        &self,
        response: &CloudflareTrackResponse,
    ) -> Result<(), VideoConferencingError> {
        // Check global error
        if let Some(error_code) = &response.error_code {
            let message = response
                .error_description
                .as_deref()
                .unwrap_or("Unknown error");
            error!("Cloudflare track error: {} - {}", error_code, message);
            return Err(VideoConferencingError::CloudflareApiError {
                message: format!("{}: {}", error_code, message),
            });
        }

        // Check individual track errors
        for (index, track) in response.tracks.iter().enumerate() {
            if let Some(error_code) = &track.error_code {
                let message = track
                    .error_description
                    .as_deref()
                    .unwrap_or("Unknown track error");
                error!("Track {} error: {} - {}", index, error_code, message);
                return Err(VideoConferencingError::CloudflareApiError {
                    message: format!("Track {}: {} - {}", index, error_code, message),
                });
            }
        }

        Ok(())
    }

    /// Health check for Cloudflare Realtime API
    pub async fn health_check(&self) -> Result<bool, VideoConferencingError> {
        debug!("Performing Cloudflare Realtime API health check");

        // Test API connectivity with a minimal request
        let url = format!("{}/apps/{}", self.base_url, self.app_id);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .send()
            .await?;

        let is_healthy = response.status().is_success() || response.status() == 404; // 404 is expected for app info endpoint
        
        if is_healthy {
            info!("Cloudflare Realtime API health check passed");
        } else {
            warn!("Cloudflare Realtime API health check failed: {}", response.status());
        }

        Ok(is_healthy)
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
            redis_url: Some("redis://localhost:6379".to_string()),
        }
    }

    #[test]
    fn test_client_creation() {
        let config = create_test_config();
        let client = CloudflareRealtimeClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_creation_fails_without_config() {
        let mut config = create_test_config();
        config.cloudflare_realtime_app_id = "".to_string();
        
        let client = CloudflareRealtimeClient::new(&config);
        assert!(matches!(client, Err(VideoConferencingError::NotConfigured)));
    }

    #[test]
    fn test_ice_servers_configuration() {
        let config = create_test_config();
        let client = CloudflareRealtimeClient::new(&config).unwrap();
        let ice_servers = client.get_ice_servers();
        
        assert_eq!(ice_servers.len(), 1);
        assert_eq!(ice_servers[0].urls[0], "stun:stun.cloudflare.com:3478");
    }

    #[test]
    fn test_rtc_configuration() {
        let config = create_test_config();
        let client = CloudflareRealtimeClient::new(&config).unwrap();
        let rtc_config = client.get_rtc_configuration();
        
        assert!(rtc_config["iceServers"].is_array());
        assert_eq!(rtc_config["bundlePolicy"], "max-bundle");
    }
}