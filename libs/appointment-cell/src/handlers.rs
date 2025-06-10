// libs/appointment-cell/src/handlers.rs
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State, Extension},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::{json, Value};
use serde::Deserialize;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_models::auth::User;
use shared_models::error::AppError;

use crate::models::{
    BookAppointmentRequest, UpdateAppointmentRequest, RescheduleAppointmentRequest,
    CancelAppointmentRequest, AppointmentSearchQuery, AppointmentStatus, AppointmentType,
    SmartBookingRequest, AppointmentError
};
use crate::services::booking::AppointmentBookingService;

// ==============================================================================
// QUERY PARAMETER STRUCTS
// ==============================================================================

#[derive(Debug, Deserialize)]
pub struct AppointmentQueryParams {
    pub patient_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub status: Option<AppointmentStatus>,
    pub appointment_type: Option<AppointmentType>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ConflictCheckQuery {
    pub doctor_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub exclude_appointment_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpcomingAppointmentsQuery {
    pub hours_ahead: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub patient_id: Option<Uuid>,
    pub doctor_id: Option<Uuid>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
}

// ==============================================================================
// ENHANCED APPOINTMENT BOOKING HANDLERS
// ==============================================================================

/// NEW: Smart appointment booking with automatic doctor selection and history prioritization
#[axum::debug_handler]
pub async fn smart_book_appointment(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<SmartBookingRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Verify authorization - only patient can book their own appointment or admin can book
    let is_patient = request.patient_id.to_string() == user.id;
    let is_admin = user.role.as_deref() == Some("admin");
    
    if !is_patient && !is_admin {
        return Err(AppError::Auth("Not authorized to book appointment for this patient".to_string()));
    }
    
    let booking_service = AppointmentBookingService::new(&state);
    
    let smart_booking_response = booking_service.smart_book_appointment(request, token).await
        .map_err(|e| match e {
            AppointmentError::SpecialtyNotAvailable { specialty } => {
                AppError::NotFound(format!("No {} doctors available at this time", specialty))
            },
            AppointmentError::DoctorNotAvailable => {
                AppError::NotFound("No doctors available at this time".to_string())
            },
            AppointmentError::ConflictDetected => {
                AppError::BadRequest("Appointment slot no longer available".to_string())
            },
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": true,
        "smart_booking": smart_booking_response,
        "message": if smart_booking_response.is_preferred_doctor {
            "Appointment booked with your preferred doctor based on consultation history"
        } else {
            "Appointment booked with best available doctor"
        }
    })))
}

/// Enhanced appointment booking with specialty validation
#[axum::debug_handler]
pub async fn book_appointment(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<BookAppointmentRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Verify authorization - only patient can book their own appointment or admin/doctor can book
    let is_patient = request.patient_id.to_string() == user.id;
    let is_admin = user.role.as_deref() == Some("admin");
    let is_doctor = user.role.as_deref() == Some("doctor");
    
    if !is_patient && !is_admin && !is_doctor {
        return Err(AppError::Auth("Not authorized to book appointment for this patient".to_string()));
    }
    
    let booking_service = AppointmentBookingService::new(&state);
    
    let appointment = booking_service.book_appointment(request, token).await
        .map_err(|e| match e {
            AppointmentError::SpecialtyNotAvailable { specialty } => {
                AppError::NotFound(format!("No {} doctors available at this time", specialty))
            },
            AppointmentError::DoctorNotAvailable => {
                AppError::NotFound("Doctor not available at requested time".to_string())
            },
            AppointmentError::ConflictDetected => {
                AppError::BadRequest("Appointment slot conflicts with existing booking".to_string())
            },
            AppointmentError::SlotNotAvailable => {
                AppError::BadRequest("Appointment slot no longer available".to_string())
            },
            AppointmentError::PatientNotFound => {
                AppError::NotFound("Patient not found".to_string())
            },
            AppointmentError::DoctorNotFound => {
                AppError::NotFound("Doctor not found".to_string())
            },
            AppointmentError::InvalidTime(msg) => {
                AppError::BadRequest(msg)
            },
            AppointmentError::ValidationError(msg) => {
                AppError::BadRequest(msg)
            },
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": true,
        "appointment": appointment,
        "message": "Appointment booked successfully"
    })))
}

#[axum::debug_handler]
pub async fn get_appointment(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let booking_service = AppointmentBookingService::new(&state);
    
    let appointment = booking_service.get_appointment(appointment_id, token).await
        .map_err(|e| match e {
            AppointmentError::NotFound => AppError::NotFound("Appointment not found".to_string()),
            _ => AppError::Internal(e.to_string()),
        })?;
    
    // Verify authorization - only patient, doctor involved, or admin can view
    let is_patient = appointment.patient_id.to_string() == user.id;
    let is_doctor = appointment.doctor_id.to_string() == user.id;
    let is_admin = user.role.as_deref() == Some("admin");
    
    if !is_patient && !is_doctor && !is_admin {
        return Err(AppError::Auth("Not authorized to view this appointment".to_string()));
    }
    
    Ok(Json(json!(appointment)))
}

#[axum::debug_handler]
pub async fn update_appointment(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<UpdateAppointmentRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let booking_service = AppointmentBookingService::new(&state);
    
    // Get appointment to check authorization
    let appointment = booking_service.get_appointment(appointment_id, token).await
        .map_err(|e| match e {
            AppointmentError::NotFound => AppError::NotFound("Appointment not found".to_string()),
            _ => AppError::Internal(e.to_string()),
        })?;
    
    // Verify authorization
    let is_patient = appointment.patient_id.to_string() == user.id;
    let is_doctor = appointment.doctor_id.to_string() == user.id;
    let is_admin = user.role.as_deref() == Some("admin");
    
    if !is_patient && !is_doctor && !is_admin {
        return Err(AppError::Auth("Not authorized to update this appointment".to_string()));
    }
    
    // Patients can only update certain fields
    if is_patient && !is_admin && !is_doctor {
        if request.status.is_some() || request.doctor_notes.is_some() {
            return Err(AppError::Auth("Patients cannot update appointment status or doctor notes".to_string()));
        }
    }
    
    let updated_appointment = booking_service.update_appointment(appointment_id, request, token).await
        .map_err(|e| match e {
            AppointmentError::ConflictDetected => {
                AppError::BadRequest("Appointment conflicts with existing booking".to_string())
            },
            AppointmentError::InvalidStatusTransition(status) => {
                AppError::BadRequest(format!("Cannot transition from current status: {}", status))
            },
            AppointmentError::InvalidTime(msg) => {
                AppError::BadRequest(msg)
            },
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": true,
        "appointment": updated_appointment,
        "message": "Appointment updated successfully"
    })))
}

#[axum::debug_handler]
pub async fn reschedule_appointment(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<RescheduleAppointmentRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let booking_service = AppointmentBookingService::new(&state);
    
    // Get appointment to check authorization
    let appointment = booking_service.get_appointment(appointment_id, token).await
        .map_err(|e| match e {
            AppointmentError::NotFound => AppError::NotFound("Appointment not found".to_string()),
            _ => AppError::Internal(e.to_string()),
        })?;
    
    // Verify authorization - patient or doctor can reschedule
    let is_patient = appointment.patient_id.to_string() == user.id;
    let is_doctor = appointment.doctor_id.to_string() == user.id;
    let is_admin = user.role.as_deref() == Some("admin");
    
    if !is_patient && !is_doctor && !is_admin {
        return Err(AppError::Auth("Not authorized to reschedule this appointment".to_string()));
    }
    
    let rescheduled_appointment = booking_service.reschedule_appointment(appointment_id, request, token).await
        .map_err(|e| match e {
            AppointmentError::ConflictDetected => {
                AppError::BadRequest("New appointment time conflicts with existing booking".to_string())
            },
            AppointmentError::InvalidTime(msg) => {
                AppError::BadRequest(msg)
            },
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": true,
        "appointment": rescheduled_appointment,
        "message": "Appointment rescheduled successfully"
    })))
}

#[axum::debug_handler]
pub async fn cancel_appointment(
    State(state): State<Arc<AppConfig>>,
    Path(appointment_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<CancelAppointmentRequest>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let booking_service = AppointmentBookingService::new(&state);
    
    // Get appointment to check authorization
    let appointment = booking_service.get_appointment(appointment_id, token).await
        .map_err(|e| match e {
            AppointmentError::NotFound => AppError::NotFound("Appointment not found".to_string()),
            _ => AppError::Internal(e.to_string()),
        })?;
    
    // Verify authorization - patient or doctor can cancel
    let is_patient = appointment.patient_id.to_string() == user.id;
    let is_doctor = appointment.doctor_id.to_string() == user.id;
    let is_admin = user.role.as_deref() == Some("admin");
    
    if !is_patient && !is_doctor && !is_admin {
        return Err(AppError::Auth("Not authorized to cancel this appointment".to_string()));
    }
    
    let cancelled_appointment = booking_service.cancel_appointment(appointment_id, request, token).await
        .map_err(|e| match e {
            AppointmentError::InvalidStatusTransition(status) => {
                AppError::BadRequest(format!("Cannot cancel appointment in status: {}", status))
            },
            AppointmentError::InvalidTime(msg) => {
                AppError::BadRequest(msg)
            },
            _ => AppError::Internal(e.to_string()),
        })?;
    
    Ok(Json(json!({
        "success": true,
        "appointment": cancelled_appointment,
        "message": "Appointment cancelled successfully"
    })))
}

// ==============================================================================
// APPOINTMENT SEARCH AND LISTING HANDLERS
// ==============================================================================

#[axum::debug_handler]
pub async fn search_appointments(
    State(state): State<Arc<AppConfig>>,
    Query(params): Query<AppointmentQueryParams>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let booking_service = AppointmentBookingService::new(&state);
    
    // Build search query
    let mut search_query = AppointmentSearchQuery {
        patient_id: params.patient_id,
        doctor_id: params.doctor_id,
        status: params.status,
        appointment_type: params.appointment_type,
        from_date: params.from_date,
        to_date: params.to_date,
        limit: params.limit,
        offset: params.offset,
    };
    
    // Apply authorization filters
    let is_admin = user.role.as_deref() == Some("admin");
    if !is_admin {
        // Non-admins can only see their own appointments
        match user.role.as_deref() {
            Some("doctor") => {
                // Doctors can only see their appointments
                if let Ok(doctor_uuid) = Uuid::parse_str(&user.id) {
                    search_query.doctor_id = Some(doctor_uuid);
                }
            },
            _ => {
                // Patients can only see their appointments
                if let Ok(patient_uuid) = Uuid::parse_str(&user.id) {
                    search_query.patient_id = Some(patient_uuid);
                }
            }
        }
    }
    
    let appointments = booking_service.search_appointments(search_query, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "appointments": appointments,
        "total": appointments.len(),
        "limit": params.limit,
        "offset": params.offset
    })))
}

#[axum::debug_handler]
pub async fn get_upcoming_appointments(
    State(state): State<Arc<AppConfig>>,
    Query(params): Query<UpcomingAppointmentsQuery>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let booking_service = AppointmentBookingService::new(&state);
    
    // Determine filters based on user role
    let (patient_id, doctor_id) = match user.role.as_deref() {
        Some("doctor") => {
            let doctor_uuid = Uuid::parse_str(&user.id)
                .map_err(|_| AppError::BadRequest("Invalid doctor ID".to_string()))?;
            (None, Some(doctor_uuid))
        },
        Some("admin") => (None, None), // Admins can see all
        _ => {
            let patient_uuid = Uuid::parse_str(&user.id)
                .map_err(|_| AppError::BadRequest("Invalid patient ID".to_string()))?;
            (Some(patient_uuid), None)
        }
    };
    
    let appointments = booking_service.get_upcoming_appointments(patient_id, doctor_id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "upcoming_appointments": appointments,
        "total": appointments.len(),
        "hours_ahead": params.hours_ahead.unwrap_or(24)
    })))
}

#[axum::debug_handler]
pub async fn get_patient_appointments(
    State(state): State<Arc<AppConfig>>,
    Path(patient_id): Path<Uuid>,
    Query(params): Query<AppointmentQueryParams>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Verify authorization - only the patient themselves or admin can view
    let is_own_appointments = patient_id.to_string() == user.id;
    let is_admin = user.role.as_deref() == Some("admin");
    
    if !is_own_appointments && !is_admin {
        return Err(AppError::Auth("Not authorized to view appointments for this patient".to_string()));
    }
    
    let booking_service = AppointmentBookingService::new(&state);
    
    let search_query = AppointmentSearchQuery {
        patient_id: Some(patient_id),
        doctor_id: params.doctor_id,
        status: params.status,
        appointment_type: params.appointment_type,
        from_date: params.from_date,
        to_date: params.to_date,
        limit: params.limit,
        offset: params.offset,
    };
    
    let appointments = booking_service.search_appointments(search_query, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "patient_id": patient_id,
        "appointments": appointments,
        "total": appointments.len()
    })))
}

#[axum::debug_handler]
pub async fn get_doctor_appointments(
    State(state): State<Arc<AppConfig>>,
    Path(doctor_id): Path<Uuid>,
    Query(params): Query<AppointmentQueryParams>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Verify authorization - only the doctor themselves or admin can view
    let is_own_appointments = doctor_id.to_string() == user.id;
    let is_admin = user.role.as_deref() == Some("admin");
    
    if !is_own_appointments && !is_admin {
        return Err(AppError::Auth("Not authorized to view appointments for this doctor".to_string()));
    }
    
    let booking_service = AppointmentBookingService::new(&state);
    
    let search_query = AppointmentSearchQuery {
        patient_id: params.patient_id,
        doctor_id: Some(doctor_id),
        status: params.status,
        appointment_type: params.appointment_type,
        from_date: params.from_date,
        to_date: params.to_date,
        limit: params.limit,
        offset: params.offset,
    };
    
    let appointments = booking_service.search_appointments(search_query, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "doctor_id": doctor_id,
        "appointments": appointments,
        "total": appointments.len()
    })))
}

// ==============================================================================
// CONFLICT DETECTION AND UTILITY HANDLERS
// ==============================================================================

#[axum::debug_handler]
pub async fn check_appointment_conflicts(
    State(state): State<Arc<AppConfig>>,
    Query(params): Query<ConflictCheckQuery>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(_user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    let booking_service = AppointmentBookingService::new(&state);

    let conflict_response = booking_service
        .check_conflicts(
            params.doctor_id,
            params.start_time,
            params.end_time,
            params.exclude_appointment_id,
            token,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(json!(conflict_response)))
}

/// Enhanced appointment statistics with doctor continuity metrics
#[axum::debug_handler]
pub async fn get_appointment_stats(
    State(state): State<Arc<AppConfig>>,
    Query(params): Query<StatsQuery>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Apply authorization filters
    let (filtered_patient_id, filtered_doctor_id) = match user.role.as_deref() {
        Some("admin") => (params.patient_id, params.doctor_id), // Admins can see all stats
        Some("doctor") => {
            let doctor_uuid = Uuid::parse_str(&user.id)
                .map_err(|_| AppError::BadRequest("Invalid doctor ID".to_string()))?;
            (params.patient_id, Some(doctor_uuid)) // Doctors can only see their own stats
        },
        _ => {
            let patient_uuid = Uuid::parse_str(&user.id)
                .map_err(|_| AppError::BadRequest("Invalid patient ID".to_string()))?;
            (Some(patient_uuid), None) // Patients can only see their own stats
        }
    };
    
    let booking_service = AppointmentBookingService::new(&state);
    
    let stats = booking_service.get_appointment_stats(
        filtered_patient_id,
        filtered_doctor_id,
        params.from_date,
        params.to_date,
        token,
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "stats": stats,
        "note": "Statistics include doctor continuity rate showing percentage of appointments with previously seen doctors"
    })))
}