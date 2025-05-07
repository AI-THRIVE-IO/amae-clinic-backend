use anyhow::{Result, anyhow};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, error};
use uuid::Uuid;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{HealthProfile, UpdateHealthProfile};

pub struct HealthProfileService {
    supabase: SupabaseClient,
}

impl HealthProfileService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
        }
    }
    
    pub async fn get_profile(&self, patient_id: &str, auth_token: &str) -> Result<HealthProfile> {
        debug!("Fetching health profile for patient: {}", patient_id);
        
        let path = format!("/rest/v1/health_profiles?patient_id=eq.{}", patient_id);
        
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;
        
        if result.is_empty() {
            return Err(anyhow!("Health profile not found"));
        }
        
        let profile: HealthProfile = serde_json::from_value(result[0].clone())?;
        Ok(profile)
    }
    
    pub async fn update_profile(
        &self, 
        profile_id: &str, 
        update_data: UpdateHealthProfile, 
        auth_token: &str
    ) -> Result<HealthProfile> {
        debug!("Updating health profile: {}", profile_id);
        
        // Calculate BMI if both height and weight are provided
        let mut bmi = None;
        if let (Some(height_cm), Some(weight_kg)) = (update_data.height_cm, update_data.weight_kg) {
            if height_cm > 0 {
                let height_m = height_cm as f64 / 100.0;
                bmi = Some(weight_kg as f64 / (height_m * height_m));
            }
        }
        
        let mut update_json = json!({
            "blood_type": update_data.blood_type,
            "height_cm": update_data.height_cm,
            "weight_kg": update_data.weight_kg,
            "allergies": update_data.allergies,
            "chronic_conditions": update_data.chronic_conditions,
            "medications": update_data.medications,
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });
        
        if let Some(bmi_value) = bmi {
            update_json["bmi"] = json!(bmi_value);
        }
        
        let path = format!("/rest/v1/health_profiles?id=eq.{}", profile_id);
        
        let result: Vec<Value> = self.supabase.request(
            Method::PATCH,
            &path,
            Some(auth_token),
            Some(update_json),
        ).await?;
        
        if result.is_empty() {
            return Err(anyhow!("Failed to update health profile"));
        }
        
        let updated_profile: HealthProfile = serde_json::from_value(result[0].clone())?;
        Ok(updated_profile)
    }
    
    pub async fn create_profile(
        &self, 
        patient_id: &str, 
        auth_token: &str
    ) -> Result<HealthProfile> {
        debug!("Creating health profile for patient: {}", patient_id);
        
        // First check if patient exists
        let patient_path = format!("/rest/v1/patients?id=eq.{}", patient_id);
        
        let patient_result: Vec<Value> = self.supabase.request(
            Method::GET,
            &patient_path,
            Some(auth_token),
            None,
        ).await?;
        
        // If patient doesn't exist, create one | REVISE BEFORE PRODUCTION!!!
        if patient_result.is_empty() {
            debug!("Patient not found, creating patient record first");
            
            let patient_data = json!({
                "id": patient_id,
                "full_name": "Patient", // Default name
                "email": "test@test_email.com", // Can be updated later
                "created_at": chrono::Utc::now().to_rfc3339(),
                "updated_at": chrono::Utc::now().to_rfc3339()
            });
            
            let _: Vec<Value> = self.supabase.request(
                Method::POST,
                "/rest/v1/patients",
                Some(auth_token),
                Some(patient_data),
            ).await?;
            
            debug!("Patient record created successfully");
        }
        
        // Now create the health profile
        let profile_data = json!({
            "patient_id": patient_id,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });
        
        let path = "/rest/v1/health_profiles";
        
        let result: Vec<Value> = self.supabase.request(
            Method::POST,
            path,
            Some(auth_token),
            Some(profile_data),
        ).await?;
        
        if result.is_empty() {
            return Err(anyhow!("Failed to create health profile"));
        }
        
        let new_profile: HealthProfile = serde_json::from_value(result[0].clone())?;
        Ok(new_profile)
    }
}