use std::sync::Arc;

use axum::{
    extract::{Path, Query, State, Extension},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::{json, Value};
use serde::{Deserialize};
use chrono::{NaiveDate, NaiveTime};

use shared_config::AppConfig;
use shared_models::auth::User;
use shared_models::error::AppError;

use crate::services::{
    doctor::DoctorService,
    availability::AvailabilityService,
    matching::DoctorMatchingService,
};
use crate::models::{
    CreateDoctorRequest, UpdateDoctorRequest, DoctorSearchFilters,
    CreateAvailabilityRequest, UpdateAvailabilityRequest, AvailabilityQueryRequest,
    DoctorImageUpload, DoctorMatchingRequest, CreateSpecialtyRequest,
    CreateAvailabilityOverrideRequest,
};

use crate::models::DoctorError;

// Query parameters for different endpoints
#[derive(Debug, Deserialize)]
pub struct DoctorSearchQuery {
    pub specialty: Option<String>,
    pub min_experience: Option<i32>,
    pub min_rating: Option<f32>,
    pub is_verified_only: Option<bool>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityQuery {
    pub date: NaiveDate,
    pub timezone: Option<String>,
    pub appointment_type: Option<String>,
    pub duration_minutes: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct MatchingQuery {
    pub preferred_date: Option<NaiveDate>,
    pub preferred_time_start: Option<NaiveTime>,
    pub preferred_time_end: Option<NaiveTime>,
    pub specialty_required: Option<String>,
    pub appointment_type: String,
    pub duration_minutes: i32,
    pub timezone: String,
    pub max_results: Option<usize>,
}

// ==============================================================================
// PUBLIC HANDLERS (NO AUTHENTICATION REQUIRED)
// ==============================================================================

#[axum::debug_handler]
pub async fn search_doctors_public(
    State(state): State<Arc<AppConfig>>,
    Query(query): Query<DoctorSearchQuery>,
) -> Result<Json<Value>, AppError> {
    // Use service account / anon key for public searches
    let doctor_service = DoctorService::new(&state);
    
    let filters = DoctorSearchFilters {
        specialty: query.specialty,
        sub_specialty: None,
        min_experience: query.min_experience,
        min_rating: query.min_rating,
        available_date: None,
        available_time_start: None,
        available_time_end: None,
        timezone: None,
        appointment_type: None,
        is_verified_only: Some(query.is_verified_only.unwrap_or(true)), // Default to verified only for public
    };
    
    let doctors = doctor_service.search_doctors_public(filters, query.limit, query.offset).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "doctors": doctors,
        "total": doctors.len()
    })))
}

#[axum::debug_handler]
pub async fn get_doctor_public(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let doctor_service = DoctorService::new(&state);
    
    let doctor = doctor_service.get_doctor_public(&doctor_id).await
        .map_err(|_| AppError::NotFound("Doctor not found".to_string()))?;
    
    Ok(Json(json!(doctor)))
}

#[axum::debug_handler]
pub async fn get_doctor_availability_public(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    Query(query): Query<AvailabilityQuery>,
) -> Result<Json<Value>, AppError> {
    let availability_service = AvailabilityService::new(&state);
    
    let availability_request = AvailabilityQueryRequest {
        date: query.date,
        timezone: query.timezone,
        appointment_type: query.appointment_type,
        duration_minutes: query.duration_minutes,
    };
    
    let availability = availability_service.get_doctor_availability_public(&doctor_id, availability_request).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(availability)))
}

#[axum::debug_handler]
pub async fn get_available_slots_public(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    Query(query): Query<AvailabilityQuery>,
) -> Result<Json<Value>, AppError> {
    let availability_service = AvailabilityService::new(&state);
    
    let availability_request = AvailabilityQueryRequest {
        date: query.date,
        timezone: query.timezone,
        appointment_type: query.appointment_type,
        duration_minutes: query.duration_minutes,
    };
    
    let slots = availability_service.get_available_slots_public(&doctor_id, availability_request).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "available_slots": slots,
        "doctor_id": doctor_id,
        "date": query.date,
        "total_slots": slots.len()
    })))
}

#[axum::debug_handler]
pub async fn get_doctor_specialties_public(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let doctor_service = DoctorService::new(&state);
    
    let specialties = doctor_service.get_doctor_specialties_public(&doctor_id).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "specialties": specialties,
        "doctor_id": doctor_id
    })))
}

// ==============================================================================
// PROTECTED DOCTOR PROFILE HANDLERS
// ==============================================================================

#[axum::debug_handler]
pub async fn create_doctor(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<CreateDoctorRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only admins can create doctor profiles
    if user.role.as_deref() != Some("admin") {
        return Err(AppError::Auth("Only administrators can create doctor profiles".to_string()));
    }
    
    let doctor_service = DoctorService::new(&state);
    
    let doctor = doctor_service.create_doctor(request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(doctor)))
}

#[axum::debug_handler]
pub async fn get_doctor(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let doctor_service = DoctorService::new(&state);
    
    let doctor = doctor_service.get_doctor(&doctor_id, token).await
        .map_err(|_| AppError::NotFound("Doctor not found".to_string()))?;
    
    Ok(Json(json!(doctor)))
}

#[axum::debug_handler]
pub async fn update_doctor(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<UpdateDoctorRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves or an admin can update profile
    let is_admin = user.role.as_deref() == Some("admin");
    let is_doctor_self = user.id == doctor_id;
    
    if !is_admin && !is_doctor_self {
        return Err(AppError::Auth("Not authorized to update this doctor profile".to_string()));
    }
    
    let doctor_service = DoctorService::new(&state);
    
    let updated_doctor = doctor_service.update_doctor(&doctor_id, request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(updated_doctor)))
}

#[axum::debug_handler]
pub async fn search_doctors(
    State(state): State<Arc<AppConfig>>,
    Query(query): Query<DoctorSearchQuery>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let doctor_service = DoctorService::new(&state);
    
    let filters = DoctorSearchFilters {
        specialty: query.specialty,
        sub_specialty: None,
        min_experience: query.min_experience,
        min_rating: query.min_rating,
        available_date: None,
        available_time_start: None,
        available_time_end: None,
        timezone: None,
        appointment_type: None,
        is_verified_only: query.is_verified_only,
    };
    
    let doctors = doctor_service.search_doctors(filters, token, query.limit, query.offset).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "doctors": doctors,
        "total": doctors.len()
    })))
}

#[axum::debug_handler]
pub async fn get_doctor_specialties(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let doctor_service = DoctorService::new(&state);
    
    let specialties = doctor_service.get_doctor_specialties(&doctor_id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(specialties)))
}

#[axum::debug_handler]
pub async fn add_doctor_specialty(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<CreateSpecialtyRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves or an admin can add specialties
    let is_admin = user.role.as_deref() == Some("admin");
    let is_doctor_self = user.id == doctor_id;
    
    if !is_admin && !is_doctor_self {
        return Err(AppError::Auth("Not authorized to add specialties for this doctor".to_string()));
    }
    
    let doctor_service = DoctorService::new(&state);
    
    let specialty = doctor_service.add_doctor_specialty(&doctor_id, request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(specialty)))
}

#[axum::debug_handler]
pub async fn upload_doctor_profile_image(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(upload): Json<DoctorImageUpload>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves can upload their profile image
    if user.id != doctor_id {
        return Err(AppError::Auth("Not authorized to upload profile image for this doctor".to_string()));
    }
    
    let doctor_service = DoctorService::new(&state);
    
    let profile_image_url = doctor_service.upload_profile_image(&doctor_id, upload, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "profile_image_url": profile_image_url })))
}

#[axum::debug_handler]
pub async fn get_doctor_stats(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves or an admin can view detailed stats
    let is_admin = user.role.as_deref() == Some("admin");
    let is_doctor_self = user.id == doctor_id;
    
    if !is_admin && !is_doctor_self {
        return Err(AppError::Auth("Not authorized to view detailed stats for this doctor".to_string()));
    }
    
    let doctor_service = DoctorService::new(&state);
    
    let stats = doctor_service.get_doctor_stats(&doctor_id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(stats)))
}

#[axum::debug_handler]
pub async fn verify_doctor(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only admins can verify doctors
    if user.role.as_deref() != Some("admin") {
        return Err(AppError::Auth("Only administrators can verify doctors".to_string()));
    }
    
    let is_verified = payload["is_verified"].as_bool()
        .ok_or_else(|| AppError::BadRequest("is_verified field is required".to_string()))?;
    
    let doctor_service = DoctorService::new(&state);
    
    let doctor = doctor_service.verify_doctor(&doctor_id, is_verified, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(doctor)))
}

// ==============================================================================
// AVAILABILITY HANDLERS (Doctor Configuration)
// ==============================================================================

#[axum::debug_handler]
pub async fn create_availability(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<CreateAvailabilityRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves can create their availability
    if user.id != doctor_id {
        return Err(AppError::Auth("Not authorized to create availability for this doctor".to_string()));
    }
    
    let availability_service = AvailabilityService::new(&state);
    
    let availability = availability_service.create_availability(&doctor_id, request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(availability)))
}

#[axum::debug_handler]
pub async fn get_doctor_availability(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let availability_service = AvailabilityService::new(&state);
    
    let availability = availability_service.get_doctor_availability(&doctor_id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(availability)))
}

#[axum::debug_handler]
pub async fn get_available_slots(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    Query(query): Query<AvailabilityQuery>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let availability_service = AvailabilityService::new(&state);
    
    let availability_query = AvailabilityQueryRequest {
        date: query.date,
        timezone: query.timezone,
        appointment_type: query.appointment_type,
        duration_minutes: query.duration_minutes,
    };
    
    let slots = availability_service.get_available_slots(&doctor_id, availability_query, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "doctor_id": doctor_id,
        "date": query.date,
        "available_slots": slots,
        "note": "These are theoretical availability slots. Verify actual availability with appointment-cell."
    })))
}

#[axum::debug_handler]
pub async fn update_availability(
    State(state): State<Arc<AppConfig>>,
    Path((doctor_id, availability_id)): Path<(String, String)>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<UpdateAvailabilityRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves can update their availability
    if user.id != doctor_id {
        return Err(AppError::Auth("Not authorized to update availability for this doctor".to_string()));
    }
    
    let availability_service = AvailabilityService::new(&state);
    
    let updated_availability = availability_service.update_availability(&availability_id, request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(updated_availability)))
}

#[axum::debug_handler]
pub async fn create_availability_override(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<CreateAvailabilityOverrideRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves can create availability overrides
    if user.id != doctor_id {
        return Err(AppError::Auth("Not authorized to create availability overrides for this doctor".to_string()));
    }
    
    let availability_service = AvailabilityService::new(&state);
    
    let override_entry = availability_service.create_availability_override(&doctor_id, request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(override_entry)))
}

#[axum::debug_handler]
pub async fn delete_availability(
    State(state): State<Arc<AppConfig>>,
    Path((doctor_id, availability_id)): Path<(String, String)>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves can delete their availability
    if user.id != doctor_id {
        return Err(AppError::Auth("Not authorized to delete availability for this doctor".to_string()));
    }
    
    let availability_service = AvailabilityService::new(&state);
    
    availability_service.delete_availability(&availability_id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "success": true })))
}

// ==============================================================================
// DOCTOR MATCHING HANDLERS
// ==============================================================================

#[axum::debug_handler]
pub async fn find_matching_doctors(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Query(query): Query<MatchingQuery>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let matching_service = DoctorMatchingService::new(&state);
    
    // Clone specialty_required so it can be used after moving into DoctorMatchingRequest
    let specialty_required = query.specialty_required.clone();

    let request = DoctorMatchingRequest {
        patient_id: uuid::Uuid::parse_str(&user.id)
            .map_err(|_| AppError::BadRequest("Invalid patient ID".to_string()))?,
        preferred_date: query.preferred_date,
        preferred_time_start: query.preferred_time_start,
        preferred_time_end: query.preferred_time_end,
        specialty_required,
        appointment_type: query.appointment_type,
        duration_minutes: query.duration_minutes,
        timezone: query.timezone,
    };
    
    let matches = matching_service.find_matching_doctors(request, token, query.max_results).await
        .map_err(|e| match e {
            DoctorError::NotAvailable => {
                if let Some(specialty) = query.specialty_required.clone() {
                    AppError::NotFound(format!("No {} doctors available at this time", specialty))
                } else {
                    AppError::NotFound("No doctors available at this time".to_string())
                }
            },
            DoctorError::ValidationError(msg) => AppError::BadRequest(msg),
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "matches": matches,
        "total": matches.len(),
        "note": "Results prioritize doctors with previous patient relationship"
    })))
}

#[axum::debug_handler]
pub async fn find_best_doctor(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<DoctorMatchingRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Ensure the patient ID matches the authenticated user
    if request.patient_id.to_string() != user.id {
        return Err(AppError::Auth("Not authorized to find doctors for this patient".to_string()));
    }
    
    let matching_service = DoctorMatchingService::new(&state);
    
    let best_match = matching_service.find_best_doctor(request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "best_match": best_match,
        "note": "Theoretical match based on doctor schedule. Verify actual availability with appointment-cell."
    })))
}

#[axum::debug_handler]
pub async fn get_recommended_doctors(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Query(query): Query<serde_json::Value>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let specialty = query.get("specialty").and_then(|s| s.as_str()).map(String::from);
    let limit = query.get("limit").and_then(|l| l.as_u64()).map(|l| l as usize);
    
    let matching_service = DoctorMatchingService::new(&state);
    
    let recommendations = matching_service.get_recommended_doctors(&user.id, specialty, token, limit).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "recommendations": recommendations,
        "total": recommendations.len(),
        "note": "Doctor recommendations based on professional credentials and ratings."
    })))
}