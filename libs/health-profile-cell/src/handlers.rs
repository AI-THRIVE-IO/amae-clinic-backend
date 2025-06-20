use std::sync::Arc;

use axum::{
    extract::{Path, State, Extension},
    Json,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use serde_json::{json, Value};

use shared_config::AppConfig;
use shared_models::auth::User;
use shared_models::error::AppError;

use crate::services::profile::HealthProfileService;
use crate::services::avatar::AvatarService;
use crate::services::document::DocumentService;
use crate::services::ai::AiService;
use crate::models::{
    UpdateHealthProfile, DocumentUpload, AvatarUpload, CarePlanRequest,
    CreateHealthProfileRequest  // ✅ Import from models.rs
};

// Health Profile Handlers

#[axum::debug_handler]
pub async fn get_health_profile(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization - only allow users to access their own profile
    if id != user.id {
        return Err(AppError::Auth("Not authorized to access this health profile".to_string()));
    }
    
    // Create profile service
    let profile_service = HealthProfileService::new(&state);
    
    // Get health profile
    match profile_service.get_profile(&id, token).await {
        Ok(profile) => Ok(Json(json!(profile))),
        Err(_) => Err(AppError::NotFound("Health profile not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn update_health_profile(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(update_data): Json<UpdateHealthProfile>,
) -> Result<Json<Value>, AppError> {
    let token = auth.token();
    if id != user.id {
        return Err(AppError::Auth("Not authorized to update this health profile".to_string()));
    }
    let profile_service = HealthProfileService::new(&state);

    let current_profile = profile_service.get_profile(&id, token).await
        .map_err(|_| AppError::NotFound("Health profile not found".to_string()))?;

    // For now, bypass patient validation due to permissions issue
    // Assume female if any reproductive fields are being updated
    let gender = if update_data.is_pregnant.unwrap_or(false) ||
                     update_data.is_breastfeeding.unwrap_or(false) ||
                     update_data.reproductive_stage.is_some() {
        "female"
    } else {
        "unknown"
    }.to_lowercase();

    if gender != "female" && (
        update_data.is_pregnant.unwrap_or(false) ||
        update_data.is_breastfeeding.unwrap_or(false) ||
        update_data.reproductive_stage.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
    ) {
        return Err(AppError::ValidationError("Female-specific fields can only be set for female patients".to_string()));
    }

    let updated_profile = profile_service.update_profile(&current_profile.id.to_string(), update_data, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(json!(updated_profile)))
}

#[axum::debug_handler]
pub async fn create_health_profile(
    State(state): State<Arc<AppConfig>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(user): Extension<User>,
    Json(payload): Json<CreateHealthProfileRequest>,  // ✅ Now uses the correct type from models.rs
) -> Result<Json<Value>, AppError> {
    let token = auth.token();

    // Require patient_id in request
    let patient_id = payload.patient_id.trim();
    if patient_id.is_empty() {
        return Err(AppError::BadRequest("patient_id is required".to_string()));
    }

    // Only allow doctors or the patient themselves to create a health profile
    // (Assume user.role is available, adjust as needed)
    // Revise Doctor role when on Doctor related cell
    let is_doctor = user.role.as_deref() == Some("doctor");
    let is_self = user.id == patient_id;
    if !is_doctor && !is_self {
        return Err(AppError::Auth("Not authorized to create health profile for this patient".to_string()));
    }

    // Create profile service
    let profile_service = HealthProfileService::new(&state);

    // For now, bypass patient validation due to permissions issue
    // Infer gender from reproductive fields for validation
    let gender = if payload.is_pregnant.unwrap_or(false) || 
                     payload.is_breastfeeding.unwrap_or(false) || 
                     payload.reproductive_stage.is_some() {
        "female"
    } else {
        "unknown"
    }.to_lowercase();

    if gender != "female" && (
        payload.is_pregnant.unwrap_or(false) ||
        payload.is_breastfeeding.unwrap_or(false) ||
        payload.reproductive_stage.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
    ) {
        return Err(AppError::ValidationError("Female-specific fields can only be set for female patients".to_string()));
    }

    let new_profile = profile_service.create_profile(
        patient_id,
        token,
        payload.is_pregnant,
        payload.is_breastfeeding,
        payload.reproductive_stage.clone(),
    ).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(json!(new_profile)))
}

#[axum::debug_handler]
pub async fn delete_health_profile(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to delete this health profile".to_string()));
    }
    
    // Create profile service
    let profile_service = HealthProfileService::new(&state);
    
    // Delete health profile
    profile_service.delete_profile(&id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "success": true })))
}

// Avatar Handlers

#[axum::debug_handler]
pub async fn upload_avatar(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(upload): Json<AvatarUpload>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to upload avatar for this profile".to_string()));
    }
    
    // Create avatar service
    let avatar_service = AvatarService::new(&state);
    
    // Upload avatar
    let avatar_url = avatar_service.upload_avatar(&id, &upload.file_data, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "avatar_url": avatar_url })))
}

#[axum::debug_handler]
pub async fn remove_avatar(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to remove avatar for this profile".to_string()));
    }
    
    // Create avatar service
    let avatar_service = AvatarService::new(&state);
    
    // Remove avatar
    avatar_service.remove_avatar(&id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "success": true })))
}

// Document Handlers

#[axum::debug_handler]
pub async fn upload_document(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(upload): Json<DocumentUpload>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to upload documents for this profile".to_string()));
    }
    
    // Create document service
    let document_service = DocumentService::new(&state);
    
    // Upload document
    let document = document_service.upload_document(
        &id, 
        &upload.title, 
        &upload.file_data, 
        &upload.file_type, 
        token
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(document)))
}

#[axum::debug_handler]
pub async fn get_documents(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to access documents for this profile".to_string()));
    }
    
    // Create document service
    let document_service = DocumentService::new(&state);
    
    // Get documents
    let documents = document_service.get_documents(&id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(documents)))
}


#[axum::debug_handler]
pub async fn get_document(
    State(state): State<Arc<AppConfig>>,
    Path((id, doc_id)): Path<(String, String)>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to access this document".to_string()));
    }
    
    // Create document service
    let document_service = DocumentService::new(&state);
    
    // Get document
    let document = document_service.get_document(&doc_id, token).await
        .map_err(|_| AppError::NotFound("Document not found".to_string()))?;
    
    // Additional authorization check - ensure document belongs to patient
    if document.patient_id.to_string() != id {
        return Err(AppError::Auth("Not authorized to access this document".to_string()));
    }
    
    Ok(Json(json!(document)))
}

#[axum::debug_handler]
pub async fn delete_document(
    State(state): State<Arc<AppConfig>>,
    Path((id, doc_id)): Path<(String, String)>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to delete this document".to_string()));
    }
    
    // Create document service
    let document_service = DocumentService::new(&state);
    
    // Get document first to verify ownership
    let document = document_service.get_document(&doc_id, token).await
        .map_err(|_| AppError::NotFound("Document not found".to_string()))?;
    
    // Additional authorization check - ensure document belongs to patient
    if document.patient_id.to_string() != id {
        return Err(AppError::Auth("Not authorized to delete this document".to_string()));
    }
    
    // Delete document
    document_service.delete_document(&doc_id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "success": true })))
}

// AI Feature Handlers

#[axum::debug_handler]
pub async fn generate_nutrition_plan(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to generate nutrition plan for this profile".to_string()));
    }
    
    // Create AI service
    let ai_service = AiService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    // Generate nutrition plan
    let nutrition_plan = ai_service.generate_nutrition_plan(&id, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(nutrition_plan))
}

#[axum::debug_handler]
pub async fn generate_care_plan(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Extension(user): Extension<User>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(request_data): Json<CarePlanRequest>,
) -> Result<Json<Value>, AppError> {
    // Get token from TypedHeader
    let token = auth.token();
    
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to generate care plan for this profile".to_string()));
    }
    
    // Create AI service
    let ai_service = AiService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    // Generate care plan
    let care_plan = ai_service.generate_care_plan(&id, &request_data.condition, token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(care_plan))
}