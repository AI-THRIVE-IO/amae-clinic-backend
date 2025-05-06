use std::sync::Arc;

use axum::{
    extract::{State, Path, Query},
    Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::debug;
use uuid::Uuid;

use shared_config::AppConfig;
use shared_models::error::AppError;
use shared_utils::extractor::{AuthUser, BearerToken};

use crate::models::{UpdateHealthProfile, DocumentUpload, AvatarUpload, CarePlanRequest};
use crate::services::{
    profile::HealthProfileService,
    avatar::AvatarService,
    document::DocumentService,
    ai::AiService,
};

// Health Profile Handlers

pub async fn get_health_profile(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization - only allow users to access their own profile
    if id != user.id {
        return Err(AppError::Auth("Not authorized to access this health profile".to_string()));
    }
    
    // Create profile service
    let profile_service = HealthProfileService::new(&state);
    
    // Get health profile
    match profile_service.get_profile(&id, &token).await {
        Ok(profile) => Ok(Json(json!(profile))),
        Err(_) => Err(AppError::NotFound("Health profile not found".to_string())),
    }
}

pub async fn update_health_profile(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Json(update_data): Json<UpdateHealthProfile>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to update this health profile".to_string()));
    }
    
    // Create profile service
    let profile_service = HealthProfileService::new(&state);
    
    // Get current profile to get the ID
    let current_profile = profile_service.get_profile(&id, &token).await
        .map_err(|_| AppError::NotFound("Health profile not found".to_string()))?;
    
    // Update health profile
    let updated_profile = profile_service.update_profile(&current_profile.id.to_string(), update_data, &token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(updated_profile)))
}

pub async fn create_health_profile(
    State(state): State<Arc<AppConfig>>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Create profile service
    let profile_service = HealthProfileService::new(&state);
    
    // Create health profile
    let new_profile = profile_service.create_profile(&user.id, &token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(new_profile)))
}

// Avatar Handlers

pub async fn upload_avatar(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Json(upload): Json<AvatarUpload>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to upload avatar for this profile".to_string()));
    }
    
    // Create avatar service
    let avatar_service = AvatarService::new(&state);
    
    // Upload avatar
    let avatar_url = avatar_service.upload_avatar(&id, &upload.file_data, &token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "avatar_url": avatar_url })))
}

pub async fn remove_avatar(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to remove avatar for this profile".to_string()));
    }
    
    // Create avatar service
    let avatar_service = AvatarService::new(&state);
    
    // Remove avatar
    avatar_service.remove_avatar(&id, &token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "success": true })))
}

// Document Handlers

pub async fn upload_document(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Json(upload): Json<DocumentUpload>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
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
        &token
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(document)))
}

pub async fn get_documents(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to access documents for this profile".to_string()));
    }
    
    // Create document service
    let document_service = DocumentService::new(&state);
    
    // Get documents
    let documents = document_service.get_documents(&id, &token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!(documents)))
}

pub async fn get_document(
    State(state): State<Arc<AppConfig>>,
    Path((id, doc_id)): Path<(String, String)>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to access this document".to_string()));
    }
    
    // Create document service
    let document_service = DocumentService::new(&state);
    
    // Get document
    let document = document_service.get_document(&doc_id, &token).await
        .map_err(|_| AppError::NotFound("Document not found".to_string()))?;
    
    // Additional authorization check - ensure document belongs to patient
    if document.patient_id.to_string() != id {
        return Err(AppError::Auth("Not authorized to access this document".to_string()));
    }
    
    Ok(Json(json!(document)))
}

pub async fn delete_document(
    State(state): State<Arc<AppConfig>>,
    Path((id, doc_id)): Path<(String, String)>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to delete this document".to_string()));
    }
    
    // Create document service
    let document_service = DocumentService::new(&state);
    
    // Get document first to verify ownership
    let document = document_service.get_document(&doc_id, &token).await
        .map_err(|_| AppError::NotFound("Document not found".to_string()))?;
    
    // Additional authorization check - ensure document belongs to patient
    if document.patient_id.to_string() != id {
        return Err(AppError::Auth("Not authorized to delete this document".to_string()));
    }
    
    // Delete document
    document_service.delete_document(&doc_id, &token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(json!({ "success": true })))
}

// AI Feature Handlers

pub async fn generate_nutrition_plan(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to generate nutrition plan for this profile".to_string()));
    }
    
    // Create AI service
    let ai_service = AiService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    // Generate nutrition plan
    let nutrition_plan = ai_service.generate_nutrition_plan(&id, &token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(nutrition_plan))
}

pub async fn generate_care_plan(
    State(state): State<Arc<AppConfig>>,
    Path(id): Path<String>,
    Json(request_data): Json<CarePlanRequest>,
    AuthUser(user): AuthUser,
    BearerToken(token): BearerToken,
) -> Result<Json<Value>, AppError> {
    // Check authorization
    if id != user.id {
        return Err(AppError::Auth("Not authorized to generate care plan for this profile".to_string()));
    }
    
    // Create AI service
    let ai_service = AiService::new(&state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    // Generate care plan
    let care_plan = ai_service.generate_care_plan(&id, &request_data.condition, &token).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(care_plan))
}