use std::sync::Arc;

use axum::{
    extract::{Path, Query, State, Extension},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::{json, Value};
use serde::{Deserialize, Serialize};
use chrono::{NaiveDate, NaiveTime, DateTime, Utc};

use shared_config::AppConfig;
use shared_models::auth::User;
use shared_models::error::AppError;

use crate::services::{
    doctor::DoctorService,
    availability::AvailabilityService,
    matching::DoctorMatchingService,
    scheduling::SchedulingService,
};
use crate::models::{
    CreateDoctorRequest, UpdateDoctorRequest, DoctorSearchFilters,
    CreateAvailabilityRequest, UpdateAvailabilityRequest, AvailabilityQueryRequest,
    CreateSpecialtyRequest, CreateAvailabilityOverrideRequest,
    DoctorImageUpload, DoctorMatchingRequest, BookAppointmentRequest,
    UpdateAppointmentRequest,
};

// Query parameters for different endpoints
#[derive(Debug, Deserialize)]
pub struct DoctorSearchQuery {
    pub specialty: Option<String>,
    pub min_experience: Option<i32>,
    pub max_consultation_fee: Option<f64>,
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
pub struct AppointmentQuery {
    pub status: Option<String>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct MatchingQuery {
    pub preferred_date: Option<NaiveDate>,
    pub preferred_time_start: Option<NaiveTime>,
    pub preferred_time_end: Option<NaiveTime>,
    pub specialty_required: Option<String>,
    pub max_consultation_fee: Option<f64>,
    pub appointment_type: String,
    pub duration_minutes: i32,
    pub timezone: String,
    pub max_results: Option<usize>,
}

// ==============================================================================
// DOCTOR PROFILE HANDLERS
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
        max_consultation_fee: query.max_consultation_fee,
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
// AVAILABILITY HANDLERS
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
        "available_slots": slots
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
    
    let request = DoctorMatchingRequest {
        patient_id: uuid::Uuid::parse_str(&user.id)
            .map_err(|_| AppError::BadRequest("Invalid patient ID".to_string()))?,
        preferred_date: query.preferred_date,
        preferred_time_start: query.preferred_time_start,
        preferred_time_end: query.preferred_time_end,
        specialty_required: query.specialty_required,
        max_consultation_fee: query.max_consultation_fee,
        appointment_type: query.appointment_type,
        duration_minutes: query.duration_minutes,
        timezone: query.timezone,
    };
    
    let matches = matching_service.find_matching_doctors(request, token, query.max_results).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "matches": matches,
        "total": matches.len()
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
        "best_match": best_match
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
        "total": recommendations.len()
    })))
}

// ==============================================================================
// APPOINTMENT SCHEDULING HANDLERS
// ==============================================================================

#[axum::debug_handler]
pub async fn book_appointment(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<BookAppointmentRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Ensure the patient ID matches the authenticated user
    if request.patient_id.to_string() != user.id {
        return Err(AppError::Auth("Not authorized to book appointments for this patient".to_string()));
    }
    
    let scheduling_service = SchedulingService::new(&state);
    
    let appointment = scheduling_service.book_appointment(request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(appointment)))
}

#[axum::debug_handler]
pub async fn update_appointment(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<UpdateAppointmentRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let scheduling_service = SchedulingService::new(&state);
    
    // Get appointment to check authorization
    let appointment = scheduling_service.get_appointment(&appointment_id, token).await
        .map_err(|_| AppError::NotFound("Appointment not found".to_string()))?;
    
    // Only the patient or doctor involved can update the appointment
    let is_patient = appointment.patient_id.to_string() == user.id;
    let is_doctor = appointment.doctor_id.to_string() == user.id;
    
    if !is_patient && !is_doctor {
        return Err(AppError::Auth("Not authorized to update this appointment".to_string()));
    }
    
    let updated_appointment = scheduling_service.update_appointment(&appointment_id, request, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(updated_appointment)))
}

#[axum::debug_handler]
pub async fn cancel_appointment(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let scheduling_service = SchedulingService::new(&state);
    
    // Get appointment to check authorization
    let appointment = scheduling_service.get_appointment(&appointment_id, token).await
        .map_err(|_| AppError::NotFound("Appointment not found".to_string()))?;
    
    // Only the patient or doctor involved can cancel the appointment
    let is_patient = appointment.patient_id.to_string() == user.id;
    let is_doctor = appointment.doctor_id.to_string() == user.id;
    
    if !is_patient && !is_doctor {
        return Err(AppError::Auth("Not authorized to cancel this appointment".to_string()));
    }
    
    let reason = payload.get("reason").and_then(|r| r.as_str()).map(String::from);
    
    let cancelled_appointment = scheduling_service.cancel_appointment(&appointment_id, reason, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(cancelled_appointment)))
}

#[axum::debug_handler]
pub async fn get_appointment(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<String>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let scheduling_service = SchedulingService::new(&state);
    
    let appointment = scheduling_service.get_appointment(&appointment_id, token).await
        .map_err(|_| AppError::NotFound("Appointment not found".to_string()))?;
    
    // Only the patient or doctor involved can view the appointment
    let is_patient = appointment.patient_id.to_string() == user.id;
    let is_doctor = appointment.doctor_id.to_string() == user.id;
    
    if !is_patient && !is_doctor {
        return Err(AppError::Auth("Not authorized to view this appointment".to_string()));
    }
    
    Ok(Json(json!(appointment)))
}

#[axum::debug_handler]
pub async fn get_patient_appointments(
    State(state): State<Arc<AppConfig>>,
    Path(patient_id): Path<String>,
    Query(query): Query<AppointmentQuery>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the patient themselves can view their appointments
    if patient_id != user.id {
        return Err(AppError::Auth("Not authorized to view appointments for this patient".to_string()));
    }
    
    let scheduling_service = SchedulingService::new(&state);
    
    let appointments = scheduling_service.get_patient_appointments(
        &patient_id,
        query.status,
        query.from_date,
        query.to_date,
        token,
        query.limit,
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "appointments": appointments,
        "total": appointments.len()
    })))
}

#[axum::debug_handler]
pub async fn get_doctor_appointments(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<String>,
    Query(query): Query<AppointmentQuery>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Only the doctor themselves can view their appointments
    if doctor_id != user.id {
        return Err(AppError::Auth("Not authorized to view appointments for this doctor".to_string()));
    }
    
    let scheduling_service = SchedulingService::new(&state);
    
    let appointments = scheduling_service.get_doctor_appointments(
        &doctor_id,
        query.status,
        query.from_date,
        query.to_date,
        token,
        query.limit,
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "appointments": appointments,
        "total": appointments.len()
    })))
}

#[axum::debug_handler]
pub async fn get_upcoming_appointments(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Query(query): Query<serde_json::Value>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    let scheduling_service = SchedulingService::new(&state);
    
    // Determine if user is patient or doctor and filter accordingly
    let doctor_id = if user.role.as_deref() == Some("doctor") {
        Some(user.id.clone())
    } else {
        None
    };
    
    let patient_id = if user.role.as_deref() != Some("doctor") {
        Some(user.id.clone())
    } else {
        None
    };
    
    let appointments = scheduling_service.get_upcoming_appointments(doctor_id, patient_id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "upcoming_appointments": appointments,
        "total": appointments.len()
    })))
}