// libs/doctor-cell/tests/handlers_test.rs
// 🔑 PRODUCTION-READY DOCTOR CELL TESTS - COMPREHENSIVE ENDPOINT COVERAGE

use std::sync::Arc;
use axum::{
    extract::{Extension, State},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header, query_param};
use chrono::{NaiveDate, NaiveTime, Utc};
use uuid::Uuid;

use doctor_cell::handlers::*;
use doctor_cell::models::*;
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

// Helper function to create a standardized doctor response with ALL required fields
fn create_doctor_response(id: &str, email: &str, full_name: &str, specialty: &str) -> serde_json::Value {
    json!({
        "id": id,
        "full_name": full_name,
        "email": email,
        "specialty": specialty,
        "bio": "Experienced physician",
        "license_number": "MD123456",
        "years_experience": 10,
        "timezone": "UTC",
        "is_verified": false,
        "is_available": true,
        "rating": 0.0,
        "total_consultations": 0,
        "created_at": Utc::now().to_rfc3339(),
        "updated_at": Utc::now().to_rfc3339()
    })
}

// Helper function to create availability response
fn create_availability_response(id: &str, doctor_id: &str, day_of_week: i32) -> serde_json::Value {
    json!({
        "id": id,
        "doctor_id": doctor_id,
        "day_of_week": day_of_week,
        "start_time": "09:00:00",
        "end_time": "17:00:00",
        "duration_minutes": 30,
        "timezone": "UTC",
        "appointment_type": "consultation",
        "is_available": true,
        "created_at": Utc::now().to_rfc3339(),
        "updated_at": Utc::now().to_rfc3339()
    })
}

// COMPREHENSIVE MOCK SETUP - Covers ALL possible HTTP endpoints
async fn setup_comprehensive_mocks(mock_server: &MockServer, token: &str, user_id: &str) {
    // === DOCTOR ENDPOINTS ===
    // Any doctor search/query
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            create_doctor_response(&Uuid::new_v4().to_string(), "doctor@example.com", "Dr. Test", "General Practice")
        ])))
        .mount(mock_server)
        .await;

    // === AVAILABILITY ENDPOINTS ===
    // Any availability query
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctor_availability"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            create_availability_response(&Uuid::new_v4().to_string(), &Uuid::new_v4().to_string(), 1)
        ])))
        .mount(mock_server)
        .await;

    // Create availability
    Mock::given(method("POST"))
        .and(path("/rest/v1/doctor_availability"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            create_availability_response(&Uuid::new_v4().to_string(), user_id, 1)
        ])))
        .mount(mock_server)
        .await;

    // === AVAILABILITY OVERRIDES ===
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctor_availability_overrides"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(mock_server)
        .await;

    // === AUTH ENDPOINTS ===
    Mock::given(method("GET"))
        .and(path("/auth/v1/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": user_id,
            "email": "test@example.com",
            "metadata": {"timezone": "UTC"}
        })))
        .mount(mock_server)
        .await;

    // === APPOINTMENT ENDPOINTS ===
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(mock_server)
        .await;

    // === HEALTH PROFILE ENDPOINTS ===
    Mock::given(method("GET"))
        .and(path("/rest/v1/health_profiles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": Uuid::new_v4(),
            "patient_id": user_id,
            "created_at": Utc::now().to_rfc3339()
        }])))
        .mount(mock_server)
        .await;

    // === CATCH-ALL MOCKS FOR ANY OTHER ENDPOINTS ===
    // Any other GET request
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(mock_server)
        .await;

    // Any other POST request
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([{"id": Uuid::new_v4()}])))
        .mount(mock_server)
        .await;

    // Any other PATCH request
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": Uuid::new_v4()}])))
        .mount(mock_server)
        .await;
}

#[tokio::test]
async fn test_create_doctor_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let admin_user = TestUser::admin("admin@example.com");
    let token = JwtTestUtils::create_test_token(&admin_user, &config.supabase_jwt_secret, Some(24));
    
    let doctor_id = Uuid::new_v4().to_string();
    let request = CreateDoctorRequest {
        full_name: "Dr. John Smith".to_string(),
        email: "dr.smith@example.com".to_string(),
        specialty: "Cardiology".to_string(),
        bio: Some("Experienced cardiologist".to_string()),
        license_number: Some("MD123456".to_string()),
        years_experience: Some(10),
        timezone: "UTC".to_string(),
    };

    // Setup comprehensive mocks
    setup_comprehensive_mocks(&mock_server, &token, &admin_user.id).await;

    // Specific mocks for create doctor
    Mock::given(method("POST"))
        .and(path("/rest/v1/doctors"))
        .and(header("Prefer", "return=representation"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            create_doctor_response(&doctor_id, &request.email, &request.full_name, &request.specialty)
        ])))
        .mount(&mock_server)
        .await;

    let result = create_doctor(
        State(Arc::new(config)),
        create_auth_header(&token),
        create_test_user_extension("admin", &admin_user.id),
        Json(request.clone())
    ).await;

    assert!(result.is_ok(), "Expected create_doctor to succeed, but got error: {:?}", result.err());
    let response = result.unwrap().0;
    assert_eq!(response["full_name"], request.full_name);
    assert_eq!(response["specialty"], request.specialty);
}

#[tokio::test]
async fn test_create_doctor_unauthorized() {
    let config = Arc::new(create_test_config());
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    
    let request = CreateDoctorRequest {
        full_name: "Dr. John Smith".to_string(),
        email: "dr.smith@example.com".to_string(),
        specialty: "Cardiology".to_string(),
        bio: None,
        license_number: None,
        years_experience: None,
        timezone: "UTC".to_string(),
    };

    let result = create_doctor(
        State(config),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id),
        Json(request)
    ).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(msg) => assert!(msg.contains("Only administrators")),
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_get_doctor_success() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();

    setup_comprehensive_mocks(&mock_server, &token, &user.id).await;

    let result = get_doctor(
        State(Arc::new(config)),
        axum::extract::Path(doctor_id.clone()),
        create_auth_header(&token)
    ).await;

    assert!(result.is_ok(), "Expected get_doctor to succeed, but got error: {:?}", result.err());
}

#[tokio::test]
async fn test_update_doctor_as_self() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let doctor_user = TestUser::doctor("doctor@example.com");
    let token = JwtTestUtils::create_test_token(&doctor_user, &config.supabase_jwt_secret, Some(24));
    
    let update_request = UpdateDoctorRequest {
        full_name: Some("Dr. John Smith Updated".to_string()),
        bio: Some("Updated bio".to_string()),
        specialty: None,
        years_experience: Some(15),
        timezone: None,
        is_available: None,
    };

    setup_comprehensive_mocks(&mock_server, &token, &doctor_user.id).await;

    let result = update_doctor(
        State(Arc::new(config)),
        axum::extract::Path(doctor_user.id.clone()),
        create_auth_header(&token),
        create_test_user_extension("doctor", &doctor_user.id),
        Json(update_request)
    ).await;

    assert!(result.is_ok(), "Expected update_doctor to succeed, but got error: {:?}", result.err());
}

#[tokio::test]
async fn test_update_doctor_unauthorized() {
    let config = Arc::new(create_test_config());
    let patient_user = TestUser::patient("patient@example.com");
    let doctor_id = Uuid::new_v4().to_string();
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    
    let update_request = UpdateDoctorRequest {
        full_name: Some("Dr. Hacker".to_string()),
        bio: None,
        specialty: None,
        years_experience: None,
        timezone: None,
        is_available: None,
    };

    let result = update_doctor(
        State(config),
        axum::extract::Path(doctor_id),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id),
        Json(update_request)
    ).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(msg) => assert!(msg.contains("Not authorized to update")),
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_verify_doctor_as_admin() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let admin_user = TestUser::admin("admin@example.com");
    let token = JwtTestUtils::create_test_token(&admin_user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();

    setup_comprehensive_mocks(&mock_server, &token, &admin_user.id).await;

    let result = verify_doctor(
        State(Arc::new(config)),
        axum::extract::Path(doctor_id.clone()),
        create_auth_header(&token),
        create_test_user_extension("admin", &admin_user.id),
        Json(json!({"is_verified": true}))
    ).await;

    assert!(result.is_ok(), "Expected verify_doctor to succeed, but got error: {:?}", result.err());
}

#[tokio::test]
async fn test_verify_doctor_unauthorized() {
    let config = Arc::new(create_test_config());
    let doctor_user = TestUser::doctor("doctor@example.com");
    let token = JwtTestUtils::create_test_token(&doctor_user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();

    let result = verify_doctor(
        State(config),
        axum::extract::Path(doctor_id),
        create_auth_header(&token),
        create_test_user_extension("doctor", &doctor_user.id),
        Json(json!({"is_verified": true}))
    ).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(msg) => assert!(msg.contains("Only administrators can verify")),
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_search_doctors_with_filters() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));

    setup_comprehensive_mocks(&mock_server, &token, &user.id).await;

    let result = search_doctors(
        State(Arc::new(config)),
        axum::extract::Query(DoctorSearchQuery {
            specialty: Some("Cardiology".to_string()),
            min_experience: Some(5),
            min_rating: Some(4.0),
            is_verified_only: Some(true),
            limit: Some(10),
            offset: Some(0),
        }),
        create_auth_header(&token)
    ).await;

    assert!(result.is_ok(), "Expected search_doctors to succeed, but got error: {:?}", result.err());
    let response = result.unwrap().0;
    assert!(response["doctors"].is_array());
    assert_eq!(response["total"], 1);
}

#[tokio::test]
async fn test_find_matching_doctors_no_specialty() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));

    setup_comprehensive_mocks(&mock_server, &token, &user.id).await;

    let result = find_matching_doctors(
        State(Arc::new(config)),
        create_auth_header(&token),
        create_test_user_extension("patient", &user.id),
        axum::extract::Query(MatchingQuery {
            preferred_date: Some(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap()),
            preferred_time_start: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            preferred_time_end: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            specialty_required: None,
            appointment_type: "consultation".to_string(),
            duration_minutes: 30,
            timezone: "UTC".to_string(),
            max_results: Some(5),
        })
    ).await;

    assert!(result.is_ok(), "Expected find_matching_doctors to succeed, but got error: {:?}", result.err());
    let response = result.unwrap().0;
    assert!(response["matches"].is_array());
}

#[tokio::test]
async fn test_find_matching_doctors() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));

    setup_comprehensive_mocks(&mock_server, &token, &user.id).await;

    let result = find_matching_doctors(
        State(Arc::new(config)),
        create_auth_header(&token),
        create_test_user_extension("patient", &user.id),
        axum::extract::Query(MatchingQuery {
            preferred_date: Some(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap()),
            preferred_time_start: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            preferred_time_end: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            specialty_required: Some("Cardiology".to_string()),
            appointment_type: "consultation".to_string(),
            duration_minutes: 30,
            timezone: "UTC".to_string(),
            max_results: Some(5),
        })
    ).await;

    assert!(result.is_ok(), "Expected find_matching_doctors to succeed, but got error: {:?}", result.err());
    let response = result.unwrap().0;
    assert!(response["matches"].is_array());
}

#[tokio::test]
async fn test_create_availability_as_doctor() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let doctor_user = TestUser::doctor("doctor@example.com");
    let token = JwtTestUtils::create_test_token(&doctor_user, &config.supabase_jwt_secret, Some(24));
    
    let availability_request = CreateAvailabilityRequest {
        day_of_week: 1,
        start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        end_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        duration_minutes: 30,
        timezone: "UTC".to_string(),
        appointment_type: "consultation".to_string(),
        buffer_minutes: None,
        max_concurrent_appointments: None,
        is_recurring: None,
        specific_date: None,
    };

    setup_comprehensive_mocks(&mock_server, &token, &doctor_user.id).await;

    let result = create_availability(
        State(Arc::new(config)),
        axum::extract::Path(doctor_user.id.clone()),
        create_auth_header(&token),
        create_test_user_extension("doctor", &doctor_user.id),
        Json(availability_request)
    ).await;

    assert!(result.is_ok(), "Expected create_availability to succeed, but got error: {:?}", result.err());
    let response = result.unwrap().0;
    assert_eq!(response["doctor_id"], doctor_user.id);
    assert_eq!(response["day_of_week"], 1);
}

#[tokio::test]
async fn test_create_availability_unauthorized() {
    let config = Arc::new(create_test_config());
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();
    
    let availability_request = CreateAvailabilityRequest {
        day_of_week: 1,
        start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        end_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        duration_minutes: 30,
        timezone: "UTC".to_string(),
        appointment_type: "consultation".to_string(),
        buffer_minutes: None,
        max_concurrent_appointments: None,
        is_recurring: None,
        specific_date: None,
    };

    let result = create_availability(
        State(config),
        axum::extract::Path(doctor_id),
        create_auth_header(&token),
        create_test_user_extension("patient", &patient_user.id),
        Json(availability_request)
    ).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Auth(msg) => assert!(msg.contains("Not authorized to create availability")),
        _ => panic!("Expected Auth error"),
    }
}

#[tokio::test]
async fn test_get_available_slots() {
    let mock_server = MockServer::start().await;
    let config = AppConfig {
        supabase_url: mock_server.uri(),
        supabase_anon_key: "test-anon-key".to_string(),
        supabase_jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
    };
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();

    setup_comprehensive_mocks(&mock_server, &token, &user.id).await;

    let result = get_available_slots(
        State(Arc::new(config)),
        axum::extract::Path(doctor_id.clone()),
        axum::extract::Query(AvailabilityQuery {
            date: NaiveDate::from_ymd_opt(2024, 12, 25).unwrap(),
            timezone: Some("UTC".to_string()),
            appointment_type: Some("consultation".to_string()),
            duration_minutes: Some(30),
        }),
        create_auth_header(&token)
    ).await;

    assert!(result.is_ok(), "Expected get_available_slots to succeed, but got error: {:?}", result.err());
    let response = result.unwrap().0;
    assert_eq!(response["doctor_id"], doctor_id);
    assert!(response["available_slots"].is_array());
}