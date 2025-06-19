// libs/doctor-cell/tests/handlers_test.rs - CORRECT TABLE NAME FIX
use std::sync::Arc;
use axum::{
    extract::{Extension, State},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header, query_param, query_param_contains};
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

fn create_complete_doctor_response(id: &str, email: &str, full_name: &str, specialty: &str) -> serde_json::Value {
    let name_parts: Vec<&str> = full_name.split_whitespace().collect();
    let (first_name, last_name) = if name_parts.len() >= 2 {
        (name_parts[0], name_parts[1..].join(" "))
    } else {
        (name_parts.get(0).copied().unwrap_or("Doctor"), "User".to_string())
    };
    
    json!({
        "id": id,
        "first_name": first_name,
        "last_name": last_name,
        "email": email,
        "specialty": specialty,
        "sub_specialty": "General Practice",
        "bio": "Experienced physician",
        "license_number": "MD123456",
        "years_experience": 10,
        "timezone": "UTC",
        "is_verified": true,
        "is_available": true,
        "rating": 4.5,
        "total_consultations": 150,
        "max_daily_appointments": 8,
        "available_days": [1, 2, 3, 4, 5],
        "date_of_birth": "1980-01-01",
        "created_at": Utc::now().to_rfc3339(),
        "updated_at": Utc::now().to_rfc3339()
    })
}

fn create_complete_availability_response(id: &str, doctor_id: &str, day_of_week: i32) -> serde_json::Value {
    json!({
        "id": id,
        "doctor_id": doctor_id,
        "day_of_week": day_of_week,
        "start_time": "09:00:00",
        "end_time": "17:00:00",
        "duration_minutes": 30,
        "timezone": "UTC",
        "appointment_type": "FollowUpConsultation",
        "buffer_minutes": 0,
        "max_concurrent_appointments": 1,
        "is_recurring": true,
        "specific_date": null,
        "is_available": true,
        "created_at": Utc::now().to_rfc3339(),
        "updated_at": Utc::now().to_rfc3339()
    })
}

// ==============================================================================
// CORRECT TABLE NAME MOCKS - USING ACTUAL DATABASE SCHEMA
// ==============================================================================

async fn setup_get_available_slots_mocks(mock_server: &MockServer, doctor_id: &str, date: &str) {
    // Calculate weekday for the mock date (2024-12-25 is Wednesday = 3)
    let weekday = 3; // Wednesday

    // CORRECT: get_availability_for_day call uses appointment_availabilities table
    // This matches the complex query with or condition for recurring/specific date
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointment_availabilities"))
        .and(query_param_contains("doctor_id", format!("eq.{}", doctor_id)))
        .and(query_param_contains("day_of_week", format!("eq.{}", weekday)))
        .and(query_param_contains("is_available", "eq.true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            create_complete_availability_response(&Uuid::new_v4().to_string(), doctor_id, weekday)
        ])))
        .mount(mock_server)
        .await;

    // CORRECT: get_availability_overrides call  
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctor_availability_overrides"))
        .and(query_param("doctor_id", format!("eq.{}", doctor_id)))
        .and(query_param("override_date", format!("eq.{}", date)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(mock_server)
        .await;
}

async fn setup_create_availability_mocks(mock_server: &MockServer, doctor_id: &str, day_of_week: i32) {
    // CORRECT: check_availability_conflicts call uses appointment_availabilities table
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointment_availabilities"))
        .and(query_param_contains("doctor_id", format!("eq.{}", doctor_id)))
        .and(query_param_contains("day_of_week", format!("eq.{}", day_of_week)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(mock_server)
        .await;

    // CORRECT: actual create call uses appointment_availabilities table
    Mock::given(method("POST"))
        .and(path("/rest/v1/appointment_availabilities"))
        .and(header("Prefer", "return=representation"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            create_complete_availability_response(&Uuid::new_v4().to_string(), doctor_id, day_of_week)
        ])))
        .mount(mock_server)
        .await;
}

async fn setup_matching_service_mocks(mock_server: &MockServer, user_id: &str, specialty: Option<&str>) {
    println!("🎯 [SIMPLE FIX] Setting up mocks for user: {}, specialty: {:?}", user_id, specialty);

    // Mock 1: Specialty validation (ONLY if specialty provided)
    if let Some(specialty_name) = specialty {
        println!("🎯 [SIMPLE FIX] Creating specialty validation mock for: {}", specialty_name);
        Mock::given(method("GET"))
            .and(path("/rest/v1/doctors"))
            .and(query_param("is_available", "eq.true"))
            .and(query_param("specialty", format!("ilike.%{}%", specialty_name)))
            .and(query_param("is_verified", "eq.true"))
            .and(query_param("order", "rating.desc,total_consultations.desc"))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                create_complete_doctor_response(&Uuid::new_v4().to_string(), "specialist@example.com", "Dr. Specialist", specialty_name)
            ])))
            .mount(mock_server)
            .await;
    }

    // Mock 2: Patient info - FIXED with correct schema
    println!("🎯 [SIMPLE FIX] Creating patient info mock");
    Mock::given(method("GET"))
        .and(path("/rest/v1/patients"))
        .and(query_param("id", format!("eq.{}", user_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": user_id,
            "first_name": "Test",
            "last_name": "Patient",
            "date_of_birth": "1990-01-01",
            "gender": "male",
            "email": "patient@example.com",
            "phone_number": "+1234567890",
            "address": "123 Test Street",
            "eircode": "D01 A1B2",
            "allergies": null,
            "chronic_conditions": [],
            "current_medications": null,
            "smoking_status": "never",
            "alcohol_use": "occasional",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }])))
        .mount(mock_server)
        .await;

    // Mock 3: Appointment history
    println!("🎯 [SIMPLE FIX] Creating appointment history mock");
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointments"))
        .and(query_param("patient_id", format!("eq.{}", user_id)))
        .and(query_param("status", "eq.completed"))
        .and(query_param("order", "created_at.desc"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(mock_server)
        .await;

    // Mock 4: Main doctor search
    if let Some(specialty_name) = specialty {
        println!("🎯 [SIMPLE FIX] Creating main search WITH specialty: {}", specialty_name);
        Mock::given(method("GET"))
            .and(path("/rest/v1/doctors"))
            .and(query_param("is_available", "eq.true"))
            .and(query_param("specialty", format!("ilike.%{}%", specialty_name)))
            .and(query_param("rating", "gte.3"))
            .and(query_param("is_verified", "eq.true"))
            .and(query_param("order", "rating.desc,total_consultations.desc"))
            .and(query_param("limit", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                create_complete_doctor_response(&Uuid::new_v4().to_string(), "doctor1@example.com", "Dr. Primary", specialty_name),
                create_complete_doctor_response(&Uuid::new_v4().to_string(), "doctor2@example.com", "Dr. Secondary", specialty_name)
            ])))
            .mount(mock_server)
            .await;
    } else {
        println!("🎯 [SIMPLE FIX] Creating main search WITHOUT specialty");
        Mock::given(method("GET"))
            .and(path("/rest/v1/doctors"))
            .and(query_param("is_available", "eq.true"))
            .and(query_param("rating", "gte.3"))
            .and(query_param("is_verified", "eq.true"))
            .and(query_param("order", "rating.desc,total_consultations.desc"))
            .and(query_param("limit", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                create_complete_doctor_response(&Uuid::new_v4().to_string(), "doctor1@example.com", "Dr. Primary", "General Medicine"),
                create_complete_doctor_response(&Uuid::new_v4().to_string(), "doctor2@example.com", "Dr. Secondary", "Internal Medicine")
            ])))
            .mount(mock_server)
            .await;
    }

    // Mock 5: Availability slots
    println!("🎯 [SIMPLE FIX] Creating availability slots mock");
    Mock::given(method("GET"))
        .and(path("/rest/v1/appointment_availabilities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": Uuid::new_v4().to_string(),
                "doctor_id": Uuid::new_v4().to_string(),
                "day_of_week": 1,
                "start_time": "09:00:00",
                "end_time": "17:00:00",
                "duration_minutes": 30,
                "is_available": true,
                "appointment_type": "FollowUpConsultation",
                "timezone": "UTC",
                "created_at": "2024-01-01T00:00:00Z"
            }
        ])))
        .mount(mock_server)
        .await;

    println!("🎯 [SIMPLE FIX] All mocks created successfully! No verification expectations.");
}

// ===================================================================
//  DEBUG TEST (placed at the bottom of the test file):
// ===================================================================

#[tokio::test]
async fn test_debug_actual_requests() {
    let mock_server = MockServer::start().await;
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));

    // Set up the interceptor
    setup_matching_service_mocks(&mock_server, &user.id, None).await;

    println!("🧪 [DEBUG TEST] Starting test with interceptor...");

    let result = find_matching_doctors(
        State(Arc::new(config)),
        create_auth_header(&token),
        create_test_user_extension("patient", &user.id),
        axum::extract::Query(MatchingQuery {
            preferred_date: Some(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap()),
            preferred_time_start: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            preferred_time_end: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            specialty_required: None,
            appointment_type: "FollowUpConsultation".to_string(),
            duration_minutes: 30,
            timezone: "UTC".to_string(),
            max_results: Some(5),
        })
    ).await;

    println!("🧪 [DEBUG TEST] Result: {:?}", result);
    
    // Print all requests received by the mock server
    let received_requests = mock_server.received_requests().await.unwrap();
    println!("🔍 [DEBUG TEST] Total requests received: {}", received_requests.len());
    
    for (i, request) in received_requests.iter().enumerate() {
        println!("🔍 [DEBUG TEST] Request {}: {} {}", i + 1, request.method, request.url);
        println!("🔍 [DEBUG TEST] Headers: {:?}", request.headers);
        if !request.body.is_empty() {
            println!("🔍 [DEBUG TEST] Body: {:?}", std::str::from_utf8(&request.body));
        }
        println!("🔍 [DEBUG TEST] ----");
    }
    
    // The test should succeed now since we return 200 for everything
    if result.is_err() {
        println!("❌ [DEBUG TEST] Still failed even with catch-all: {:?}", result.err());
    } else {
        println!("✅ [DEBUG TEST] Succeeded with catch-all mock");
    }
}

#[allow(dead_code)]
async fn debug_mock_server_requests(_mock_server: &MockServer) {
    println!("🔍 [DEBUG] Mock server received these requests:");
    // Add request logging if available in wiremock version
    println!("🔍 [DEBUG] Check if requests are reaching the mock server");
}

#[tokio::test]
async fn test_mock_server_basic_functionality() {
    let mock_server = MockServer::start().await;
    
    println!("🧪 [BASIC TEST] Mock server URL: {}", mock_server.uri());
    
    // Create a very simple mock
    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"test": "success"})))
        .mount(&mock_server)
        .await;
    
    // Make a direct HTTP request to the mock server
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/test", mock_server.uri()))
        .send()
        .await
        .unwrap();
    
    println!("🧪 [BASIC TEST] Response status: {}", response.status());
    
    assert_eq!(response.status(), 200);
    
    let json: serde_json::Value = response.json().await.unwrap();
    println!("🧪 [BASIC TEST] Response body: {:?}", json);
    
    assert_eq!(json["test"], "success");
    println!("✅ [BASIC TEST] Mock server is working correctly!");
}


// ==============================================================================
// PASSING TESTS (NO CHANGES NEEDED)
// ==============================================================================

#[tokio::test]
async fn test_create_doctor_success() {
    let mock_server = MockServer::start().await;
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let admin_user = TestUser::admin("admin@example.com");
    let token = JwtTestUtils::create_test_token(&admin_user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();
    
    let request = CreateDoctorRequest {
        first_name: "Dr. John".to_string(),
        last_name: "Smith".to_string(),
        email: "dr.smith@example.com".to_string(),
        specialty: "Cardiology".to_string(),
        sub_specialty: Some("Interventional Cardiology".to_string()),
        bio: Some("Experienced cardiologist".to_string()),
        license_number: "MD123456".to_string(),
        years_experience: Some(10),
        timezone: Some("UTC".to_string()),
        max_daily_appointments: Some(8),
        available_days: Some(vec![1, 2, 3, 4, 5]),
        date_of_birth: chrono::NaiveDate::from_ymd_opt(1980, 1, 1).unwrap(),
    };

    // Mock email check - return empty array (no existing doctor)
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock_server)
        .await;

    // Mock create doctor
    Mock::given(method("POST"))
        .and(path("/rest/v1/doctors"))
        .and(header("Prefer", "return=representation"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([{
            "id": doctor_id,
            "first_name": request.first_name,
            "last_name": request.last_name,
            "email": request.email,
            "specialty": request.specialty,
            "sub_specialty": request.sub_specialty,
            "bio": request.bio,
            "license_number": request.license_number,
            "years_experience": request.years_experience,
            "timezone": request.timezone,
            "max_daily_appointments": request.max_daily_appointments,
            "available_days": request.available_days.clone().unwrap_or_else(|| vec![1, 2, 3, 4, 5]),
            "date_of_birth": request.date_of_birth.format("%Y-%m-%d").to_string(),
            "is_verified": false,
            "is_available": true,
            "rating": 0.0,
            "total_consultations": 0,
            "created_at": Utc::now().to_rfc3339(),
            "updated_at": Utc::now().to_rfc3339()
        }])))
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
    assert_eq!(response["first_name"], request.first_name);
    assert_eq!(response["last_name"], request.last_name);
}

#[tokio::test]
async fn test_create_doctor_unauthorized() {
    let config = Arc::new(create_test_config());
    let patient_user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    
    let request = CreateDoctorRequest {
        first_name: "Dr. John".to_string(),
        last_name: "Smith".to_string(),
        email: "dr.smith@example.com".to_string(),
        specialty: "Cardiology".to_string(),
        sub_specialty: None,
        bio: None,
        license_number: "TEMP123456".to_string(),
        years_experience: None,
        timezone: Some("UTC".to_string()),
        max_daily_appointments: None,
        available_days: None,
        date_of_birth: chrono::NaiveDate::from_ymd_opt(1980, 1, 1).unwrap(),
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
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();

    // Mock get doctor by ID
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            create_complete_doctor_response(&doctor_id, "doctor@example.com", "Dr. Test", "General Practice")
        ])))
        .mount(&mock_server)
        .await;

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
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let doctor_user = TestUser::doctor("doctor@example.com");
    let token = JwtTestUtils::create_test_token(&doctor_user, &config.supabase_jwt_secret, Some(24));
    
    let update_request = UpdateDoctorRequest {
        first_name: Some("Dr. John".to_string()),
        last_name: Some("Smith Updated".to_string()),
        bio: Some("Updated bio".to_string()),
        specialty: None,
        years_experience: Some(15),
        timezone: None,
        is_available: None,
        max_daily_appointments: None,
        available_days: None,
        sub_specialty: None,
        date_of_birth: None,
    };

    // Mock update doctor
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/doctors"))
        .and(header("Prefer", "return=representation"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            create_complete_doctor_response(&doctor_user.id, "doctor@example.com", "Dr. John Smith Updated", "General Practice")
        ])))
        .mount(&mock_server)
        .await;

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
    let token = JwtTestUtils::create_test_token(&patient_user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();
    
    let update_request = UpdateDoctorRequest {
        first_name: Some("Dr. John".to_string()),
        last_name: Some("Smith Updated".to_string()),
        bio: None,
        specialty: None,
        years_experience: None,
        timezone: None,
        is_available: None,
        max_daily_appointments: None,
        available_days: None,
        sub_specialty: None,
        date_of_birth: None,
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
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let admin_user = TestUser::admin("admin@example.com");
    let token = JwtTestUtils::create_test_token(&admin_user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();

    // Mock verify doctor
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/doctors"))
        .and(header("Prefer", "return=representation"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            create_complete_doctor_response(&doctor_id, "verified@example.com", "Dr. Verified", "Cardiology")
        ])))
        .mount(&mock_server)
        .await;

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
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));

    // Mock search doctors
    Mock::given(method("GET"))
        .and(path("/rest/v1/doctors"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            create_complete_doctor_response(&Uuid::new_v4().to_string(), "doctor1@example.com", "Dr. Alice", "Cardiology"),
            create_complete_doctor_response(&Uuid::new_v4().to_string(), "doctor2@example.com", "Dr. Bob", "Neurology")
        ])))
        .mount(&mock_server)
        .await;

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
    assert_eq!(response["total"], 2);
}

// ==============================================================================
// FIXED FAILING TESTS - CORRECT TABLE NAMES
// ==============================================================================

#[tokio::test]
async fn test_get_available_slots() {
    let mock_server = MockServer::start().await;
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));
    let doctor_id = Uuid::new_v4().to_string();

    // FIXED: Setup correct mocks using appointment_availabilities table
    setup_get_available_slots_mocks(&mock_server, &doctor_id, "2024-12-25").await;

    let result = get_available_slots(
        State(Arc::new(config)),
        axum::extract::Path(doctor_id.clone()),
        axum::extract::Query(AvailabilityQuery {
            date: NaiveDate::from_ymd_opt(2024, 12, 25).unwrap().to_string(),
            timezone: Some("UTC".to_string()),
            appointment_type: Some(AppointmentType::FollowUpConsultation),
            duration_minutes: Some(30),
            patient_id: None,
            include_concurrent: Some(false),
            priority_filter: None,
        }),
        create_auth_header(&token)
    ).await;

    assert!(result.is_ok(), "Expected get_available_slots to succeed, but got error: {:?}", result.err());
    let response = result.unwrap().0;
    assert!(response["available_slots"].is_array());
}

#[tokio::test]
async fn test_create_availability_as_doctor() {
    let mock_server = MockServer::start().await;
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let doctor_user = TestUser::doctor("doctor@example.com");
    let token = JwtTestUtils::create_test_token(&doctor_user, &config.supabase_jwt_secret, Some(24));
    
    let availability_request = CreateAvailabilityRequest {
        day_of_week: 1,
        duration_minutes: 30,
        morning_start_time: Some(Utc::now().date_naive().and_time(NaiveTime::from_hms_opt(9, 0, 0).unwrap()).and_utc()),
        morning_end_time: Some(Utc::now().date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap()).and_utc()),
        afternoon_start_time: Some(Utc::now().date_naive().and_time(NaiveTime::from_hms_opt(13, 0, 0).unwrap()).and_utc()),
        afternoon_end_time: Some(Utc::now().date_naive().and_time(NaiveTime::from_hms_opt(17, 0, 0).unwrap()).and_utc()),
        is_available: Some(true),
        // Enhanced medical scheduling fields
        appointment_type: AppointmentType::FollowUpConsultation,
        buffer_minutes: Some(10),
        max_concurrent_appointments: Some(1),
        is_recurring: Some(true),
        specific_date: None,
        // Additional fields for curl compatibility
        timezone: Some("UTC".to_string()),
        appointment_types: Some(vec!["FollowUpConsultation".to_string()]),
        is_active: Some(true),
    };

    // FIXED: Setup correct mocks using appointment_availabilities table
    setup_create_availability_mocks(&mock_server, &doctor_user.id, 1).await;

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
        duration_minutes: 30,
        morning_start_time: Some(Utc::now().date_naive().and_time(NaiveTime::from_hms_opt(9, 0, 0).unwrap()).and_utc()),
        morning_end_time: Some(Utc::now().date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap()).and_utc()),
        afternoon_start_time: Some(Utc::now().date_naive().and_time(NaiveTime::from_hms_opt(13, 0, 0).unwrap()).and_utc()),
        afternoon_end_time: Some(Utc::now().date_naive().and_time(NaiveTime::from_hms_opt(17, 0, 0).unwrap()).and_utc()),
        is_available: Some(true),
        // Enhanced medical scheduling fields
        appointment_type: AppointmentType::FollowUpConsultation,
        buffer_minutes: Some(10),
        max_concurrent_appointments: Some(1),
        is_recurring: Some(true),
        specific_date: None,
        // Additional fields for curl compatibility
        timezone: Some("UTC".to_string()),
        appointment_types: Some(vec!["FollowUpConsultation".to_string()]),
        is_active: Some(true),
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
async fn test_find_matching_doctors_no_specialty() {
    let mock_server = MockServer::start().await;
    let mut config = create_test_config();
    config.supabase_url = mock_server.uri();
    
    let user = TestUser::patient("patient@example.com");
    let token = JwtTestUtils::create_test_token(&user, &config.supabase_jwt_secret, Some(24));

    // FIXED: Setup correct mocks using appointment_availabilities table
    setup_matching_service_mocks(&mock_server, &user.id, None).await;

    let result = find_matching_doctors(
        State(Arc::new(config)),
        create_auth_header(&token),
        create_test_user_extension("patient", &user.id),
        axum::extract::Query(MatchingQuery {
            preferred_date: Some(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap()),
            preferred_time_start: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            preferred_time_end: Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            specialty_required: None,
            appointment_type: "FollowUpConsultation".to_string(),
            duration_minutes: 30,
            timezone: "UTC".to_string(),
            max_results: Some(5),
        })
    ).await;

    assert!(result.is_ok(), "Expected find_matching_doctors to succeed, but got error: {:?}", result.err());
}