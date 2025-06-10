use serde_json::json;
use uuid::Uuid;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

use shared_utils::test_utils::{TestConfig, TestUser, JwtTestUtils};
use video_conferencing_cell::{
    models::{VideoSessionType, CreateVideoSessionRequest},
    services::{VideoConferencingIntegrationService, CloudflareRealtimeClient},
};

fn create_test_config() -> shared_config::AppConfig {
    TestConfig::default().to_app_config()
}

#[tokio::test]
async fn test_integration_service_creation() {
    let config = create_test_config();
    let service = VideoConferencingIntegrationService::new(&config);
    assert!(service.is_ok());
}

#[tokio::test]
async fn test_integration_service_fails_without_config() {
    let mut config = create_test_config();
    config.cloudflare_realtime_app_id = "".to_string();
    
    let service = VideoConferencingIntegrationService::new(&config);
    assert!(service.is_err());
}

#[tokio::test]
async fn test_cloudflare_client_creation() {
    let config = create_test_config();
    let client = CloudflareRealtimeClient::new(&config);
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_cloudflare_client_fails_without_config() {
    let mut config = create_test_config();
    config.cloudflare_realtime_api_token = "".to_string();
    
    let client = CloudflareRealtimeClient::new(&config);
    assert!(client.is_err());
}

#[tokio::test]
async fn test_video_availability_check() {
    let mock_server = MockServer::start().await;
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let patient_id = Uuid::new_v4().to_string();
    let doctor_id = Uuid::new_v4().to_string();
    let appointment_id = Uuid::new_v4().to_string();
    
    // Mock appointment response
    Mock::given(method("GET"))
        .and(path(format!("/rest/v1/appointments")))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![json!({
            "id": appointment_id,
            "patient_id": patient_id,
            "doctor_id": doctor_id,
            "appointment_date": "2024-12-25T10:00:00Z",
            "status": "confirmed",
            "appointment_type": "consultation",
            "duration_minutes": 30
        })]))
        .mount(&mock_server)
        .await;
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    
    let service = VideoConferencingIntegrationService::new(&config).unwrap();
    let appointment_uuid = Uuid::parse_str(&appointment_id).unwrap();
    
    let is_available = service
        .is_video_available_for_appointment(appointment_uuid, &token)
        .await;
    
    assert!(is_available.is_ok());
    assert!(is_available.unwrap()); // Should be available for consultation
}

#[tokio::test]
async fn test_video_session_type_serialization() {
    let consultation = VideoSessionType::Consultation;
    let follow_up = VideoSessionType::FollowUp;
    let emergency = VideoSessionType::Emergency;
    
    let consultation_json = serde_json::to_string(&consultation).unwrap();
    let follow_up_json = serde_json::to_string(&follow_up).unwrap();
    let emergency_json = serde_json::to_string(&emergency).unwrap();
    
    assert_eq!(consultation_json, "\"consultation\"");
    assert_eq!(follow_up_json, "\"follow_up\"");
    assert_eq!(emergency_json, "\"emergency\"");
}

#[tokio::test]
async fn test_create_session_request_validation() {
    let appointment_id = Uuid::new_v4();
    let scheduled_time = chrono::Utc::now() + chrono::Duration::hours(1);
    
    let request = CreateVideoSessionRequest {
        appointment_id,
        session_type: VideoSessionType::Consultation,
        scheduled_start_time: scheduled_time,
    };
    
    // Verify serialization works
    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateVideoSessionRequest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(request.appointment_id, deserialized.appointment_id);
    assert_eq!(request.session_type, deserialized.session_type);
}