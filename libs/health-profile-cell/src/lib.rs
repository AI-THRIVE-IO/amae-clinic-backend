// ✅ Health Profile Cell - Clean module organization
pub mod handlers;
pub mod router;
pub mod models;
pub mod services;

// ✅ Re-export commonly used types for convenience
pub use models::{
    CreateHealthProfileRequest,
    UpdateHealthProfile,
    HealthProfile,
    Document,
    DocumentUpload,
    AvatarUpload,
    NutritionPlanRequest,
    CarePlanRequest,
};

// ✅ Re-export main router for integration
pub use router::health_profile_routes;

// ✅ Re-export handlers for direct usage if needed
pub use handlers::*;

// ✅ Public services API
pub mod api {
    pub use crate::services::profile::HealthProfileService;
    pub use crate::services::avatar::AvatarService;
    pub use crate::services::document::DocumentService;
    pub use crate::services::ai::AiService;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_module_exports() {
        // Verify that all important types are properly exported
        let _: CreateHealthProfileRequest = CreateHealthProfileRequest::default();
        let _: UpdateHealthProfile = UpdateHealthProfile {
            blood_type: None,
            height_cm: None,
            weight_kg: None,
            allergies: None,
            chronic_conditions: None,
            medications: None,
            medical_history: None,
            is_pregnant: None,
            is_breastfeeding: None,
            reproductive_stage: None,
            gender: None,
            date_of_birth: None,
            emergency_contact_name: None,
            emergency_contact_phone: None,
        };
    }
}