use serde_json::json;
use uuid::Uuid;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

use shared_utils::test_utils::{TestConfig, TestUser, JwtTestUtils};
use video_conferencing_cell::{
    models::{VideoSessionType, CreateVideoSessionRequest, ParticipantType},
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
        room_id: None,
        room_type: Some(VideoSessionType::Consultation),
        max_participants: Some(4),
        participant_type: ParticipantType::Patient,
        session_type: VideoSessionType::Consultation,
        scheduled_start_time: scheduled_time,
    };
    
    // Verify serialization works
    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateVideoSessionRequest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(request.appointment_id, deserialized.appointment_id);
    assert_eq!(request.session_type, deserialized.session_type);
    assert_eq!(request.participant_type, deserialized.participant_type);
    assert_eq!(request.max_participants, deserialized.max_participants);
}

// ==============================================================================
// ROOM-BASED ARCHITECTURE TESTS
// ==============================================================================

#[tokio::test]
async fn test_participant_type_serialization() {
    use video_conferencing_cell::models::ParticipantType;
    
    let patient = ParticipantType::Patient;
    let doctor = ParticipantType::Doctor;
    let specialist = ParticipantType::Specialist;
    let nurse = ParticipantType::Nurse;
    let guardian = ParticipantType::Guardian;
    let therapist = ParticipantType::Therapist;
    let coordinator = ParticipantType::Coordinator;
    let interpreter = ParticipantType::Interpreter;
    let observer = ParticipantType::Observer;
    
    // Test serialization
    assert_eq!(serde_json::to_string(&patient).unwrap(), "\"patient\"");
    assert_eq!(serde_json::to_string(&doctor).unwrap(), "\"doctor\"");
    assert_eq!(serde_json::to_string(&specialist).unwrap(), "\"specialist\"");
    assert_eq!(serde_json::to_string(&nurse).unwrap(), "\"nurse\"");
    assert_eq!(serde_json::to_string(&guardian).unwrap(), "\"guardian\"");
    assert_eq!(serde_json::to_string(&therapist).unwrap(), "\"therapist\"");
    assert_eq!(serde_json::to_string(&coordinator).unwrap(), "\"coordinator\"");
    assert_eq!(serde_json::to_string(&interpreter).unwrap(), "\"interpreter\"");
    assert_eq!(serde_json::to_string(&observer).unwrap(), "\"observer\"");
    
    // Test deserialization
    assert_eq!(serde_json::from_str::<ParticipantType>("\"patient\"").unwrap(), patient);
    assert_eq!(serde_json::from_str::<ParticipantType>("\"doctor\"").unwrap(), doctor);
    assert_eq!(serde_json::from_str::<ParticipantType>("\"specialist\"").unwrap(), specialist);
}

#[tokio::test]
async fn test_enhanced_session_types() {
    use video_conferencing_cell::models::VideoSessionType;
    
    let specialist_consult = VideoSessionType::SpecialistConsult;
    let group_therapy = VideoSessionType::GroupTherapy;
    let family_consult = VideoSessionType::FamilyConsult;
    let team_meeting = VideoSessionType::TeamMeeting;
    
    // Test serialization
    assert_eq!(serde_json::to_string(&specialist_consult).unwrap(), "\"specialist_consult\"");
    assert_eq!(serde_json::to_string(&group_therapy).unwrap(), "\"group_therapy\"");
    assert_eq!(serde_json::to_string(&family_consult).unwrap(), "\"family_consult\"");
    assert_eq!(serde_json::to_string(&team_meeting).unwrap(), "\"team_meeting\"");
    
    // Test deserialization
    assert_eq!(serde_json::from_str::<VideoSessionType>("\"specialist_consult\"").unwrap(), specialist_consult);
    assert_eq!(serde_json::from_str::<VideoSessionType>("\"group_therapy\"").unwrap(), group_therapy);
    assert_eq!(serde_json::from_str::<VideoSessionType>("\"family_consult\"").unwrap(), family_consult);
    assert_eq!(serde_json::from_str::<VideoSessionType>("\"team_meeting\"").unwrap(), team_meeting);
}

#[tokio::test]
async fn test_room_based_session_request() {
    use video_conferencing_cell::models::{CreateVideoSessionRequest, VideoSessionType, ParticipantType};
    
    let appointment_id = Uuid::new_v4();
    let scheduled_time = chrono::Utc::now() + chrono::Duration::hours(1);
    
    // Test multi-participant session request
    let specialist_consult_request = CreateVideoSessionRequest {
        appointment_id,
        room_id: Some("room_specialist_consult_2025_123".to_string()),
        room_type: Some(VideoSessionType::SpecialistConsult),
        max_participants: Some(6), // Patient + Primary Doctor + Specialist + Nurse + Observer + Coordinator
        participant_type: ParticipantType::Specialist,
        session_type: VideoSessionType::SpecialistConsult,
        scheduled_start_time: scheduled_time,
    };
    
    // Verify serialization/deserialization
    let json = serde_json::to_string(&specialist_consult_request).unwrap();
    let deserialized: CreateVideoSessionRequest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(specialist_consult_request.appointment_id, deserialized.appointment_id);
    assert_eq!(specialist_consult_request.room_id, deserialized.room_id);
    assert_eq!(specialist_consult_request.room_type, deserialized.room_type);
    assert_eq!(specialist_consult_request.max_participants, deserialized.max_participants);
    assert_eq!(specialist_consult_request.participant_type, deserialized.participant_type);
    assert_eq!(specialist_consult_request.session_type, deserialized.session_type);
    
    // Test group therapy session
    let group_therapy_request = CreateVideoSessionRequest {
        appointment_id,
        room_id: Some("room_group_therapy_anxiety_2025_456".to_string()),
        room_type: Some(VideoSessionType::GroupTherapy),
        max_participants: Some(12), // 1 Therapist + up to 11 Patients
        participant_type: ParticipantType::Therapist,
        session_type: VideoSessionType::GroupTherapy,
        scheduled_start_time: scheduled_time,
    };
    
    let json = serde_json::to_string(&group_therapy_request).unwrap();
    let deserialized: CreateVideoSessionRequest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(group_therapy_request.max_participants, Some(12));
    assert_eq!(group_therapy_request.participant_type, ParticipantType::Therapist);
    assert_eq!(group_therapy_request.session_type, VideoSessionType::GroupTherapy);
}

#[tokio::test]
async fn test_room_security_models() {
    use video_conferencing_cell::models::{
        RoomSecurityConfig, AdmissionPolicy, RecordingPolicy, 
        ParticipantPermissions, HIPAALevel, ParticipantType
    };
    use std::collections::HashMap;
    
    // Test security configuration serialization
    let mut participant_permissions = HashMap::new();
    participant_permissions.insert(ParticipantType::Doctor, ParticipantPermissions {
        can_share_screen: true,
        can_share_audio: true,
        can_share_video: true,
        can_record: true,
        can_admit_participants: true,
        can_remove_participants: true,
        can_end_session: true,
    });
    
    participant_permissions.insert(ParticipantType::Patient, ParticipantPermissions {
        can_share_screen: false,
        can_share_audio: true,
        can_share_video: true,
        can_record: false,
        can_admit_participants: false,
        can_remove_participants: false,
        can_end_session: false,
    });
    
    let security_config = RoomSecurityConfig {
        admission_control: AdmissionPolicy::WaitingRoom,
        recording_permissions: RecordingPolicy::HostOnly,
        participant_permissions,
        hipaa_compliance_level: HIPAALevel::Enhanced,
        end_to_end_encryption: true,
    };
    
    // Test serialization
    let json = serde_json::to_string(&security_config).unwrap();
    let deserialized: RoomSecurityConfig = serde_json::from_str(&json).unwrap();
    
    assert!(matches!(deserialized.admission_control, AdmissionPolicy::WaitingRoom));
    assert!(matches!(deserialized.recording_permissions, RecordingPolicy::HostOnly));
    assert!(matches!(deserialized.hipaa_compliance_level, HIPAALevel::Enhanced));
    assert!(deserialized.end_to_end_encryption);
    assert_eq!(deserialized.participant_permissions.len(), 2);
}

#[tokio::test]
async fn test_video_room_model() {
    use video_conferencing_cell::models::{
        VideoRoom, VideoSessionType, RoomStatus, RoomSecurityConfig, 
        AdmissionPolicy, RecordingPolicy, HIPAALevel, ParticipantType, ParticipantPermissions
    };
    use std::collections::HashMap;
    use chrono::Utc;
    
    let room_id = "room_cardiology_consult_2025_789".to_string();
    let appointment_id = Uuid::new_v4();
    
    let mut participant_permissions = HashMap::new();
    participant_permissions.insert(ParticipantType::Doctor, ParticipantPermissions {
        can_share_screen: true,
        can_share_audio: true,
        can_share_video: true,
        can_record: true,
        can_admit_participants: true,
        can_remove_participants: true,
        can_end_session: true,
    });
    
    let security_config = RoomSecurityConfig {
        admission_control: AdmissionPolicy::WaitingRoom,
        recording_permissions: RecordingPolicy::HostOnly,
        participant_permissions,
        hipaa_compliance_level: HIPAALevel::Maximum,
        end_to_end_encryption: true,
    };
    
    let video_room = VideoRoom {
        id: room_id.clone(),
        appointment_id,
        room_type: VideoSessionType::SpecialistConsult,
        max_participants: 6,
        waiting_room_enabled: true,
        recording_enabled: false,
        room_status: RoomStatus::Scheduled,
        security_config,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    // Test serialization
    let json = serde_json::to_string(&video_room).unwrap();
    let deserialized: VideoRoom = serde_json::from_str(&json).unwrap();
    
    assert_eq!(deserialized.id, room_id);
    assert_eq!(deserialized.appointment_id, appointment_id);
    assert!(matches!(deserialized.room_type, VideoSessionType::SpecialistConsult));
    assert_eq!(deserialized.max_participants, 6);
    assert!(deserialized.waiting_room_enabled);
    assert!(!deserialized.recording_enabled);
    assert!(matches!(deserialized.room_status, RoomStatus::Scheduled));
    assert!(matches!(deserialized.security_config.hipaa_compliance_level, HIPAALevel::Maximum));
}