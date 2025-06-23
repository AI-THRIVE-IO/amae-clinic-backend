use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthProfile {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub blood_type: Option<String>,
    pub height_cm: Option<i32>,
    pub weight_kg: Option<f64>,
    pub bmi: Option<f64>,
    
    // Handle database field name mappings and type differences
    #[serde(alias = "allergies")]
    pub allergies: Option<Vec<String>>,
    
    #[serde(alias = "chronic_conditions")]
    pub chronic_conditions: Option<Vec<String>>,
    
    #[serde(alias = "current_medications")]
    pub medications: Option<Vec<String>>,
    
    #[serde(alias = "medical_history")]
    pub medical_history: Option<Vec<String>>,
    
    pub avatar_url: Option<String>,
    pub is_pregnant: Option<bool>,
    pub is_breastfeeding: Option<bool>,
    pub reproductive_stage: Option<String>,
    pub gender: Option<String>,
    pub date_of_birth: Option<String>,
    pub emergency_contact_name: Option<String>,
    pub emergency_contact_phone: Option<String>,
    pub ai_health_summary: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateHealthProfile {
    pub blood_type: Option<String>,
    pub height_cm: Option<i32>,
    pub weight_kg: Option<f64>,
    pub allergies: Option<Vec<String>>,
    pub chronic_conditions: Option<Vec<String>>,
    pub medications: Option<Vec<String>>,
    pub medical_history: Option<Vec<String>>,
    pub is_pregnant: Option<bool>,
    pub is_breastfeeding: Option<bool>,
    pub reproductive_stage: Option<String>,
    pub gender: Option<String>,
    pub date_of_birth: Option<String>,
    pub emergency_contact_name: Option<String>,
    pub emergency_contact_phone: Option<String>,
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

/// ✅ Canonical CreateHealthProfileRequest - single source of truth
/// This is the type that should be used throughout the codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHealthProfileRequest {
    pub patient_id: String,
    pub is_pregnant: Option<bool>,
    pub is_breastfeeding: Option<bool>,
    pub reproductive_stage: Option<String>,
}

impl CreateHealthProfileRequest {
    /// Validation method to ensure data consistency
    pub fn validate(&self) -> Result<(), String> {
        if self.patient_id.trim().is_empty() {
            return Err("patient_id cannot be empty".to_string());
        }
        
        // Validate UUID format
        if uuid::Uuid::parse_str(&self.patient_id).is_err() {
            return Err("patient_id must be a valid UUID".to_string());
        }
        
        // Validate reproductive stage if provided
        if let Some(ref stage) = self.reproductive_stage {
            let valid_stages = [
                "premenopause", "perimenopause", "postmenopause", 
                "childbearing", "pregnancy", "lactation"
            ];
            if !stage.is_empty() && !valid_stages.contains(&stage.as_str()) {
                return Err(format!(
                    "Invalid reproductive stage. Must be one of: {}", 
                    valid_stages.join(", ")
                ));
            }
        }
        
        Ok(())
    }
    
    /// Check if any female-specific fields are set
    pub fn has_female_specific_fields(&self) -> bool {
        self.is_pregnant.unwrap_or(false) ||
        self.is_breastfeeding.unwrap_or(false) ||
        self.reproductive_stage.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
    }
}

impl Default for CreateHealthProfileRequest {
    fn default() -> Self {
        Self {
            patient_id: String::new(),
            is_pregnant: None,
            is_breastfeeding: None,
            reproductive_stage: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_create_health_profile_request_serialization() {
        let request = CreateHealthProfileRequest {
            patient_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            is_pregnant: Some(true),
            is_breastfeeding: Some(false),
            reproductive_stage: Some("childbearing".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateHealthProfileRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.patient_id, deserialized.patient_id);
        assert_eq!(request.is_pregnant, deserialized.is_pregnant);
        assert_eq!(request.is_breastfeeding, deserialized.is_breastfeeding);
        assert_eq!(request.reproductive_stage, deserialized.reproductive_stage);
    }

    #[test]
    fn test_create_health_profile_request_validation() {
        // Valid request
        let valid_request = CreateHealthProfileRequest {
            patient_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            is_pregnant: Some(false),
            is_breastfeeding: Some(false),
            reproductive_stage: Some("premenopause".to_string()),
        };
        assert!(valid_request.validate().is_ok());

        // Invalid patient_id
        let invalid_patient_id = CreateHealthProfileRequest {
            patient_id: "invalid-uuid".to_string(),
            is_pregnant: None,
            is_breastfeeding: None,
            reproductive_stage: None,
        };
        assert!(invalid_patient_id.validate().is_err());

        // Empty patient_id
        let empty_patient_id = CreateHealthProfileRequest {
            patient_id: "".to_string(),
            is_pregnant: None,
            is_breastfeeding: None,
            reproductive_stage: None,
        };
        assert!(empty_patient_id.validate().is_err());

        // Invalid reproductive stage
        let invalid_stage = CreateHealthProfileRequest {
            patient_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            is_pregnant: None,
            is_breastfeeding: None,
            reproductive_stage: Some("invalid_stage".to_string()),
        };
        assert!(invalid_stage.validate().is_err());
    }

    #[test]
    fn test_has_female_specific_fields() {
        let request_with_fields = CreateHealthProfileRequest {
            patient_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            is_pregnant: Some(true),
            is_breastfeeding: Some(false),
            reproductive_stage: Some("childbearing".to_string()),
        };
        assert!(request_with_fields.has_female_specific_fields());

        let request_without_fields = CreateHealthProfileRequest {
            patient_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            is_pregnant: None,
            is_breastfeeding: None,
            reproductive_stage: None,
        };
        assert!(!request_without_fields.has_female_specific_fields());
    }

    #[test]
    fn test_default_implementation() {
        let default_request = CreateHealthProfileRequest::default();
        assert!(default_request.patient_id.is_empty());
        assert!(default_request.is_pregnant.is_none());
        assert!(default_request.is_breastfeeding.is_none());
        assert!(default_request.reproductive_stage.is_none());
    }
}