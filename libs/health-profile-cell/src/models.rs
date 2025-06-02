use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthProfile {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub blood_type: Option<String>,
    pub height_cm: Option<i32>,
    pub weight_kg: Option<i32>,
    pub bmi: Option<f64>,
    pub allergies: Option<String>,
    pub chronic_conditions: Option<Vec<String>>,
    pub medications: Option<String>,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateHealthProfile {
    pub blood_type: Option<String>,
    pub height_cm: Option<i32>,
    pub weight_kg: Option<i32>,
    pub allergies: Option<String>,
    pub chronic_conditions: Option<Vec<String>>,
    pub medications: Option<String>,
    pub is_pregnant: Option<bool>,
    pub is_breastfeeding: Option<bool>,
    pub reproductive_stage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub title: String,
    pub file_url: String,
    pub file_type: String,
    pub uploaded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentUpload {
    pub title: String,
    pub file_data: String, // Base64 encoded file
    pub file_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarUpload {
    pub file_data: String, // Base64 encoded image
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NutritionPlanRequest {
    pub patient_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarePlanRequest {
    pub patient_id: Uuid,
    pub condition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHealthProfileRequest {
    pub patient_id: String,
    pub is_pregnant: Option<bool>,
    pub is_breastfeeding: Option<bool>,
    pub reproductive_stage: Option<String>,
}