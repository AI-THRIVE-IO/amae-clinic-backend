use std::sync::Arc;
use axum::{
    extract::{Path, Query, State, Extension},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::{json, Value};

use shared_config::AppConfig;
use shared_models::auth::User;
use shared_models::error::AppError;

use crate::models::{CreatePatientRequest, UpdatePatientRequest, PatientSearchQuery};
use crate::services::PatientService;

#[axum::debug_handler]
pub async fn create_patient(
    State(config): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(request): Json<CreatePatientRequest>,
) -> Result<Json<Value>, AppError> {
    let service = PatientService::new(&config);
    
    let patient = service.create_patient(request, auth.token())
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(patient)))
}

#[axum::debug_handler]
pub async fn get_patient(
    State(config): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Path(patient_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let service = PatientService::new(&config);
    
    let patient = service.get_patient(&patient_id, auth.token())
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(patient)))
}

#[axum::debug_handler]
pub async fn update_patient(
    State(config): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Path(patient_id): Path<String>,
    Json(request): Json<UpdatePatientRequest>,
) -> Result<Json<Value>, AppError> {
    let service = PatientService::new(&config);
    
    let patient = service.update_patient(&patient_id, request, auth.token())
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(patient)))
}

#[axum::debug_handler]
pub async fn search_patients(
    State(config): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Query(query): Query<PatientSearchQuery>,
) -> Result<Json<Value>, AppError> {
    let service = PatientService::new(&config);
    
    let patients = service.search_patients(query, auth.token())
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({
        "patients": patients,
        "total": patients.len()
    })))
}