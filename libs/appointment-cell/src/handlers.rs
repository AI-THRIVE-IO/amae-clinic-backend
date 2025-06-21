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

/// NEW: Async Smart appointment booking with automatic doctor selection and history prioritization
#[axum::debug_handler]
pub async fn smart_book_appointment_async(
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
    
    // Try to use booking queue if available, fallback to direct booking
    match try_async_booking(&state, request.clone(), token).await {
        Ok(booking_response) => {
            Ok(Json(json!({
                "success": true,
                "async_booking": true,
                "job_id": booking_response.job_id,
                "status": booking_response.status,
                "estimated_completion": booking_response.estimated_completion_time,
                "websocket_channel": booking_response.websocket_channel,
                "tracking_url": booking_response.tracking_url,
                "message": "Smart booking request queued for processing. You will receive real-time updates via WebSocket."
            })))
        }
        Err(_) => {
            // Fallback to synchronous booking
            smart_book_appointment_sync(State(state), TypedHeader(auth), Extension(user), Json(request)).await
        }
    }
}

/// Fallback synchronous smart booking
#[axum::debug_handler]
pub async fn smart_book_appointment_sync(
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
    
    // Add timeout protection for smart booking operations
    let smart_booking_future = booking_service.smart_book_appointment(request, token);
    let timeout_duration = std::time::Duration::from_secs(30); // 30 second timeout
    
    let smart_booking_response = tokio::time::timeout(timeout_duration, smart_booking_future)
        .await
        .map_err(|_| AppError::Internal("Smart booking operation timed out. Please try again.".to_string()))?
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
        "async_booking": false,
        "smart_booking": smart_booking_response,
        "message": if smart_booking_response.is_preferred_doctor {
            "Appointment booked with your preferred doctor based on consultation history"
        } else {
            "Appointment booked with best available doctor"
        }
    })))
}

/// Legacy endpoint name for compatibility
pub async fn smart_book_appointment(
    state: State<Arc<AppConfig>>,
    auth: TypedHeader<Authorization<Bearer>>,
    user: Extension<User>,
    request: Json<SmartBookingRequest>,
) -> Result<Json<Value>, AppError> {
    smart_book_appointment_async(state, auth, user, request).await
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
    
    let appointments = booking_service.get_upcoming_appointments(patient_id, doctor_id, params.hours_ahead, token).await
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

// ==============================================================================
// ASYNC BOOKING HANDLERS
// ==============================================================================

/// Get booking job status
#[axum::debug_handler]
pub async fn get_booking_status(
    State(state): State<Arc<AppConfig>>,
    Path(job_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let _token = auth.token();
    
    match try_get_booking_status(&state, job_id).await {
        Ok(Some(job)) => {
            // Verify authorization - only patient who made the booking or admin
            let is_patient = job.patient_id.to_string() == user.id;
            let is_admin = user.role.as_deref() == Some("admin");
            
            if !is_patient && !is_admin {
                return Err(AppError::Auth("Not authorized to view this booking status".to_string()));
            }
            
            Ok(Json(json!({
                "job_id": job.job_id,
                "patient_id": job.patient_id,
                "status": job.status,
                "created_at": job.created_at,
                "updated_at": job.updated_at,
                "completed_at": job.completed_at,
                "retry_count": job.retry_count,
                "max_retries": job.max_retries,
                "error_message": job.error_message,
                "worker_id": job.worker_id
            })))
        }
        Ok(None) => Err(AppError::NotFound("Booking job not found".to_string())),
        Err(_) => Err(AppError::Internal("Booking queue service unavailable".to_string()))
    }
}

/// Cancel booking job
#[axum::debug_handler]
pub async fn cancel_booking(
    State(state): State<Arc<AppConfig>>,
    Path(job_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let _token = auth.token();
    
    // First verify the job exists and user has permission
    match try_get_booking_status(&state, job_id).await {
        Ok(Some(job)) => {
            let is_patient = job.patient_id.to_string() == user.id;
            let is_admin = user.role.as_deref() == Some("admin");
            
            if !is_patient && !is_admin {
                return Err(AppError::Auth("Not authorized to cancel this booking".to_string()));
            }
            
            // Cancel the job
            match try_cancel_booking(&state, job_id).await {
                Ok(()) => Ok(Json(json!({
                    "success": true,
                    "job_id": job_id,
                    "message": "Booking cancelled successfully"
                }))),
                Err(_) => Err(AppError::Internal("Failed to cancel booking".to_string()))
            }
        }
        Ok(None) => Err(AppError::NotFound("Booking job not found".to_string())),
        Err(_) => Err(AppError::Internal("Booking queue service unavailable".to_string()))
    }
}

/// Retry failed booking job
#[axum::debug_handler]
pub async fn retry_booking(
    State(state): State<Arc<AppConfig>>,
    Path(job_id): Path<Uuid>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let _token = auth.token();
    
    // First verify the job exists and user has permission
    match try_get_booking_status(&state, job_id).await {
        Ok(Some(job)) => {
            let is_patient = job.patient_id.to_string() == user.id;
            let is_admin = user.role.as_deref() == Some("admin");
            
            if !is_patient && !is_admin {
                return Err(AppError::Auth("Not authorized to retry this booking".to_string()));
            }
            
            // Retry the job
            match try_retry_booking(&state, job_id).await {
                Ok(()) => Ok(Json(json!({
                    "success": true,
                    "job_id": job_id,
                    "message": "Booking retry initiated successfully"
                }))),
                Err(_) => Err(AppError::Internal("Failed to retry booking".to_string()))
            }
        }
        Ok(None) => Err(AppError::NotFound("Booking job not found".to_string())),
        Err(_) => Err(AppError::Internal("Booking queue service unavailable".to_string()))
    }
}

/// Get queue statistics (admin only)
#[axum::debug_handler]
pub async fn get_queue_stats(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let _token = auth.token();
    
    // Only admins can view queue stats
    if user.role.as_deref() != Some("admin") {
        return Err(AppError::Auth("Not authorized to view queue statistics".to_string()));
    }
    
    match try_get_queue_stats(&state).await {
        Ok(stats) => Ok(Json(json!(stats))),
        Err(_) => Err(AppError::Internal("Booking queue service unavailable".to_string()))
    }
}

// ==============================================================================
// ASYNC BOOKING HELPER FUNCTIONS
// ==============================================================================

async fn try_async_booking(
    config: &AppConfig,
    request: SmartBookingRequest,
    token: &str,
) -> Result<booking_queue_cell::BookingJobResponse, Box<dyn std::error::Error + Send + Sync>> {
    use booking_queue_cell::{BookingConsumerService, WorkerConfig};
    
    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(worker_config, std::sync::Arc::new(config.clone())).await?;
    
    // Convert appointment-cell types to booking-queue-cell types
    let queue_request = booking_queue_cell::SmartBookingRequest {
        patient_id: request.patient_id,
        specialty: request.specialty_required,
        urgency: Some(booking_queue_cell::BookingUrgency::Normal), // Default urgency
        preferred_doctor_id: None, // Not available in current model
        preferred_time_slot: if let (Some(date), Some(time)) = (request.preferred_date, request.preferred_time_start) {
            Some(date.and_time(time).and_utc())
        } else {
            None
        },
        alternative_time_slots: None, // Not available in current model
        appointment_type: Some(match request.appointment_type {
            crate::models::AppointmentType::InitialConsultation => booking_queue_cell::AppointmentType::InitialConsultation,
            crate::models::AppointmentType::FollowUpConsultation => booking_queue_cell::AppointmentType::FollowUpConsultation,
            crate::models::AppointmentType::EmergencyConsultation => booking_queue_cell::AppointmentType::Emergency,
            crate::models::AppointmentType::SpecialtyConsultation => booking_queue_cell::AppointmentType::Specialist,
            _ => booking_queue_cell::AppointmentType::InitialConsultation, // Default fallback
        }),
        reason_for_visit: request.patient_notes.clone(),
        consultation_mode: Some(booking_queue_cell::ConsultationMode::InPerson), // Default mode
        is_follow_up: Some(matches!(request.appointment_type, crate::models::AppointmentType::FollowUpConsultation)),
        notes: request.patient_notes,
    };
    
    consumer.enqueue_booking(queue_request, token).await.map_err(Into::into)
}

async fn try_get_booking_status(
    config: &AppConfig,
    job_id: Uuid,
) -> Result<Option<booking_queue_cell::BookingJob>, Box<dyn std::error::Error + Send + Sync>> {
    use booking_queue_cell::{BookingConsumerService, WorkerConfig};
    
    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(worker_config, std::sync::Arc::new(config.clone())).await?;
    consumer.get_job_status(job_id).await.map_err(Into::into)
}

async fn try_cancel_booking(
    config: &AppConfig,
    job_id: Uuid,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use booking_queue_cell::{BookingConsumerService, WorkerConfig};
    
    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(worker_config, std::sync::Arc::new(config.clone())).await?;
    consumer.cancel_job(job_id).await.map_err(Into::into)
}

async fn try_retry_booking(
    config: &AppConfig,
    job_id: Uuid,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use booking_queue_cell::{BookingConsumerService, WorkerConfig};
    
    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(worker_config, std::sync::Arc::new(config.clone())).await?;
    consumer.retry_job(job_id).await.map_err(Into::into)
}

async fn try_get_queue_stats(
    config: &AppConfig,
) -> Result<booking_queue_cell::QueueStats, Box<dyn std::error::Error + Send + Sync>> {
    use booking_queue_cell::{BookingConsumerService, WorkerConfig};
    
    let worker_config = WorkerConfig::default();
    let consumer = BookingConsumerService::new(worker_config, std::sync::Arc::new(config.clone())).await?;
    Ok(consumer.get_queue_stats().await)
}

// ==============================================================================
// ENHANCED CONSISTENCY HANDLERS
// ==============================================================================

/// Enhanced consistency check for appointment scheduling
#[axum::debug_handler]
pub async fn check_scheduling_consistency(
    State(state): State<Arc<AppConfig>>,
    Query(params): Query<EnhancedConsistencyQuery>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    
    // Authorization check - only doctors and admins can perform consistency checks
    match user.role.as_deref() {
        Some("admin") | Some("doctor") => {},
        _ => return Err(AppError::Auth("Insufficient permissions for consistency check".to_string())),
    }
    
    let booking_service = AppointmentBookingService::new(&state);
    
    // Perform comprehensive consistency check
    let consistency_result = booking_service.check_comprehensive_scheduling_consistency(
        params.doctor_id,
        params.start_time,
        params.end_time,
        params.appointment_type,
        token,
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "consistency_check": consistency_result,
        "timestamp": Utc::now().to_rfc3339(),
        "service": "enhanced_scheduling_consistency"
    })))
}

/// Monitor scheduling system health and performance
#[axum::debug_handler]
pub async fn get_scheduling_health(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(_auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
) -> Result<Json<Value>, AppError> {
    // Only admins can access health monitoring
    match user.role.as_deref() {
        Some("admin") => {},
        _ => return Err(AppError::Auth("Insufficient permissions for health monitoring".to_string())),
    }
    
    let booking_service = AppointmentBookingService::new(&state);
    
    let health_data = booking_service.monitor_scheduling_health()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(health_data))
}

#[derive(Debug, Deserialize)]
pub struct EnhancedConsistencyQuery {
    pub doctor_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub appointment_type: AppointmentType,
}