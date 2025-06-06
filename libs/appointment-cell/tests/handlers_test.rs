use std::sync::Arc;
use axum::{
    extract::{Extension, State},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, query_param};
use chrono::{DateTime, Utc, NaiveDate};
use uuid::Uuid;

use appointment_cell::handlers::*;
use appointment_cell::models::*;
use shared_config::AppConfig;
use shared_models::{auth::User, error::AppError};
use shared_utils::test_utils::{TestConfig, TestUser, JwtTestUtils};

fn create_test_config() -> AppConfig {
    TestConfig::default().to_app_config()
}

fn create_test_user_extension(role: &str, id: &str) -> Extension<User> {
    Extension(User {
        id: id.to_string(),
        email: Some(format!("{}@example.com", role)),
        role: Some(role.to_string()),
        metadata: None,
        created_at: Some(chrono::Utc::now()),
    })
}

fn create_auth_header(token: &str) -> TypedHeader<Authorization<Bearer>> {
    let auth = Authorization::bearer(token).unwrap();
    TypedHeader(auth)
}

#[tokio::test]
async fn test_book_appointment_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4();
    let appointment_id = Uuid::new_v4();
    
    let book_request = BookAppointmentRequest {
        patient_id: uuid::Uuid::parse_str(&patient_user.id).unwrap(),
        doctor_id: Some(doctor_id),
        appointment_date: DateTime::parse_from_rfc3339("2024-12-25T10:00:00Z").unwrap().with_timezone(&Utc),
        appointment_type: AppointmentType::GeneralConsultation,
        duration_minutes: 30,
        timezone: "UTC".to_string(),
        patient_notes: Some("Regular checkup".to_string()),
        preferred_language: None,
        specialty_required: None,
    };

    // Mock conflict check (no conflicts)
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("doctor_id", format!("eq.{}", doctor_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock_server)
        .await;

    // Mock doctor availability check
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctor_availability"))
        .and(query_param("doctor_id", format!("eq.{}", doctor_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": Uuid::new_v4(),
                "doctor_id": doctor_id,
                "day_of_week": 3, // Wednesday (Dec 25, 2024)
                "start_time": "09:00:00",
                "end_time": "17:00:00",
                "duration_minutes": 30,
                "is_available": true
            }
        ])))
        .mount(&mock_server)
        .await;

    // Mock appointment creation
    Mock::given(method("POST"))
        .and(path("/rest/v1/appointments"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": appointment_id,
            "patient_id": patient_user.id,
            "doctor_id": doctor_id,
            "scheduled_time": "2024-12-25T10:00:00Z",
            "duration_minutes": 30,
            "status": "scheduled",
            "type": "consultation",
            "notes": "Regular checkup",
            "created_at": "2024-01-01T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let result = book_appointment(
        State(Arc::new(config)),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id),
        Json(book_request)
    ).await;

    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response["id"], appointment_id.to_string());
    assert_eq!(response["status"], "scheduled");
}

#[tokio::test]
async fn test_book_appointment_conflict() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4();
    
    let book_request = BookAppointmentRequest {
        patient_id: uuid::Uuid::parse_str(&patient_user.id).unwrap(),
        doctor_id: Some(doctor_id),
        appointment_date: DateTime::parse_from_rfc3339("2024-12-25T10:00:00Z").unwrap().with_timezone(&Utc),
        appointment_type: AppointmentType::GeneralConsultation,
        duration_minutes: 30,
        timezone: "UTC".to_string(),
        patient_notes: None,
        preferred_language: None,
        specialty_required: None,
    };

    // Mock conflict check (existing appointment found)
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("doctor_id", format!("eq.{}", doctor_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": Uuid::new_v4(),
                "doctor_id": doctor_id,
                "scheduled_time": "2024-12-25T10:00:00Z",
                "duration_minutes": 30,
                "status": "scheduled"
            }
        ])))
        .mount(&mock_server)
        .await;

    let result = book_appointment(
        State(Arc::new(config)),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id),
        Json(book_request)
    ).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => assert!(msg.contains("conflict") || msg.contains("unavailable")),
        _ => panic!("Expected BadRequest error for conflict"),
    }
}

#[tokio::test]
async fn test_get_appointment_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    let appointment_id = Uuid::new_v4();

    // Mock get appointment API call
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("id", format!("eq.{}", appointment_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": appointment_id,
                "patient_id": patient_user.id,
                "doctor_id": Uuid::new_v4(),
                "scheduled_time": "2024-12-25T10:00:00Z",
                "duration_minutes": 30,
                "status": "scheduled",
                "type": "consultation",
                "notes": "Test appointment"
            }
        ])))
        .mount(&mock_server)
        .await;

    let result = get_appointment(
        State(Arc::new(config)),
        axum::extract::Path(appointment_id),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id)
    ).await;

    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response["id"], appointment_id.to_string());
    assert_eq!(response["patient_id"], patient_user.id);
}

#[tokio::test]
async fn test_cancel_appointment_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    let appointment_id = Uuid::new_v4();

    let cancel_request = CancelAppointmentRequest {
        reason: "Schedule conflict".to_string(),
        cancelled_by: appointment_cell::models::CancelledBy::Patient,
    };

    // Mock get appointment for authorization check
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("id", format!("eq.{}", appointment_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": appointment_id,
                "patient_id": patient_user.id,
                "doctor_id": Uuid::new_v4(),
                "status": "scheduled"
            }
        ])))
        .mount(&mock_server)
        .await;

    // Mock appointment cancellation
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("id", format!("eq.{}", appointment_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": appointment_id,
                "status": "cancelled",
                "cancelled_at": "2024-01-01T12:00:00Z",
                "cancellation_reason": "Schedule conflict"
            }
        ])))
        .mount(&mock_server)
        .await;

    let result = cancel_appointment(
        State(Arc::new(config)),
        axum::extract::Path(appointment_id),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id),
        Json(cancel_request)
    ).await;

    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response["status"], "cancelled");
}

#[tokio::test]
async fn test_reschedule_appointment_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    let appointment_id = Uuid::new_v4();
    let doctor_id = Uuid::new_v4();

    let reschedule_request = RescheduleAppointmentRequest {
        new_start_time: DateTime::parse_from_rfc3339("2024-12-26T14:00:00Z").unwrap().with_timezone(&Utc),
        new_duration_minutes: Some(30),
        reason: Some("Better time slot available".to_string()),
    };

    // Mock get appointment for authorization check
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("id", format!("eq.{}", appointment_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": appointment_id,
                "patient_id": patient_user.id,
                "doctor_id": doctor_id,
                "status": "scheduled",
                "duration_minutes": 30
            }
        ])))
        .mount(&mock_server)
        .await;

    // Mock conflict check for new time
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("doctor_id", format!("eq.{}", doctor_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock_server)
        .await;

    // Mock appointment update
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("id", format!("eq.{}", appointment_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": appointment_id,
                "scheduled_time": "2024-12-26T14:00:00Z",
                "status": "scheduled",
                "updated_at": "2024-01-01T12:00:00Z"
            }
        ])))
        .mount(&mock_server)
        .await;

    let result = reschedule_appointment(
        State(Arc::new(config)),
        axum::extract::Path(appointment_id),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id),
        Json(reschedule_request)
    ).await;

    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response[0]["scheduled_time"], "2024-12-26T14:00:00Z");
}

#[tokio::test]
async fn test_search_appointments_by_patient() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));

    // Mock search appointments API call
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("patient_id", format!("eq.{}", patient_user.id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": Uuid::new_v4(),
                "patient_id": patient_user.id,
                "doctor_id": Uuid::new_v4(),
                "scheduled_time": "2024-12-25T10:00:00Z",
                "status": "scheduled",
                "type": "consultation"
            },
            {
                "id": Uuid::new_v4(),
                "patient_id": patient_user.id,
                "doctor_id": Uuid::new_v4(),
                "scheduled_time": "2024-12-20T14:00:00Z",
                "status": "completed",
                "type": "followup"
            }
        ])))
        .mount(&mock_server)
        .await;

    let query = AppointmentQueryParams {
        patient_id: Some(uuid::Uuid::parse_str(&patient_user.id).unwrap()),
        doctor_id: None,
        status: None,
        appointment_type: None,
        from_date: None,
        to_date: None,
        limit: Some(10),
        offset: Some(0),
    };

    let result = search_appointments(
        State(Arc::new(config)),
        axum::extract::Query(query),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id)
    ).await;

    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert_eq!(response["appointments"].as_array().unwrap().len(), 2);
    assert_eq!(response["total"], 2);
}

#[tokio::test]
async fn test_smart_booking_request() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));

    let smart_request = SmartBookingRequest {
        patient_id: uuid::Uuid::parse_str(&patient_user.id).unwrap(),
        specialty_required: Some("Cardiology".to_string()),
        preferred_date: Some(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap()),
        preferred_time_start: None,
        preferred_time_end: None,
        appointment_type: AppointmentType::GeneralConsultation,
        duration_minutes: 30,
        timezone: "UTC".to_string(),
        patient_notes: Some("Regular checkup".to_string()),
        allow_history_prioritization: Some(true),
    };

    // Mock doctor matching API calls
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": Uuid::new_v4(),
                "specialty": "Cardiology",
                "is_available": true,
                "rating": 4.8
            }
        ])))
        .mount(&mock_server)
        .await;

    let result = smart_book_appointment(
        State(Arc::new(config)),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id),
        Json(smart_request)
    ).await;

    assert!(result.is_ok());
    let response = result.unwrap().0;
    assert!(response["recommended_slots"].is_array());
    assert!(response["best_match"].is_object());
}

#[tokio::test]
async fn test_unauthorized_access_to_other_patient_appointment() {
    let config = Arc::new(create_test_config());
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    let other_patient_id = Uuid::new_v4().to_string();
    let appointment_id = Uuid::new_v4();

    // This should fail because the user is trying to access another patient's appointment
    let query = AppointmentQueryParams {
        patient_id: Some(uuid::Uuid::parse_str(&other_patient_id).unwrap()),
        doctor_id: None,
        status: None,
        appointment_type: None,
        from_date: None,
        to_date: None,
        limit: None,
        offset: None,
    };

    let result = search_appointments(
        State(config),
        axum::extract::Query(query),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id)
    ).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(_) => {}, // Expected
        _ => panic!("Expected Auth error"),
    }
}