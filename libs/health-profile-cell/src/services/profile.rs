use anyhow::{Result, anyhow};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, error};
use uuid::Uuid;
use headers::HeaderMap;
use headers::HeaderValue;

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

    pub fn supabase(&self) -> &SupabaseClient {
        &self.supabase
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

        // Add the Prefer header to get the updated record back
        let mut headers = HeaderMap::new();
        headers.insert("Prefer", HeaderValue::from_static("return=representation"));
        
        // Use request_with_headers instead of request
        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::PATCH,
            &path,
            Some(auth_token),
            Some(update_json),
            Some(headers),
        ).await?;
        
        if result.is_empty() {
            return Err(anyhow!("Failed to update health profile"));
        }
        
            // Better error handling for deserialization
        let updated_profile = match serde_json::from_value::<HealthProfile>(result[0].clone()) {
            Ok(profile) => profile,
            Err(e) => {
                debug!("Error deserializing profile: {}", e);
                debug!("Raw JSON: {}", result[0]);
                return Err(anyhow!("Failed to deserialize health profile: {}", e));
            }
        };

        Ok(updated_profile)
    }
    
    pub async fn create_profile(
        &self, 
        patient_id: &str, 
        auth_token: &str,
        // Add optional female-only fields
        is_pregnant: Option<bool>,
        is_breastfeeding: Option<bool>,
        reproductive_stage: Option<String>,
    ) -> Result<HealthProfile> {
        debug!("Processing health profile for patient: {}", patient_id);

        // Check if patient exists
        let patient_path = format!("/rest/v1/patients?id=eq.{}", patient_id);
        let patient_result: Vec<Value> = self.supabase.request_with_headers(
            Method::GET,
            &patient_path,
            Some(auth_token),
            None,
            None,
        ).await?;
        if patient_result.is_empty() {
            debug!("Patient not found, aborting health profile creation");
            return Err(anyhow!("Patient not found"));
        }

        // Check if health profile already exists
        let profile_path = format!("/rest/v1/health_profiles?patient_id=eq.{}", patient_id);
        let existing_profiles: Vec<Value> = self.supabase.request_with_headers(
            Method::GET,
            &profile_path,
            Some(auth_token),
            None,
            None,
        ).await?;
        if !existing_profiles.is_empty() {
            debug!("Health profile already exists, returning existing profile");
            let existing_profile = match serde_json::from_value::<HealthProfile>(existing_profiles[0].clone()) {
                Ok(profile) => profile,
                Err(e) => {
                    debug!("Error deserializing existing profile: {}", e);
                    return Err(anyhow!("Failed to deserialize existing health profile: {}", e));
                }
            };
            return Ok(existing_profile);
        }

        // Create new health profile
        debug!("No health profile found, creating new one");
        let mut profile_data = json!({
            "patient_id": patient_id,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });

        if let Some(val) = is_pregnant {
            profile_data["is_pregnant"] = json!(val);
        }
        if let Some(val) = is_breastfeeding {
            profile_data["is_breastfeeding"] = json!(val);
        }
        if let Some(ref val) = reproductive_stage {
            if !val.is_empty() {
                profile_data["reproductive_stage"] = json!(val);
            }
        }

        let mut headers = HeaderMap::new();
        headers.insert("Prefer", HeaderValue::from_static("return=representation"));
        let path = "/rest/v1/health_profiles";
        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            path,
            Some(auth_token),
            Some(profile_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to create health profile - no response returned"));
        }

        let new_profile = match serde_json::from_value::<HealthProfile>(result[0].clone()) {
            Ok(profile) => profile,
            Err(e) => {
                debug!("Error deserializing profile: {}", e);
                debug!("Raw JSON: {}", result[0]);
                return Err(anyhow!("Failed to deserialize health profile: {}", e));
            }
        };
        debug!("Health profile created successfully with ID: {}", new_profile.id);
        Ok(new_profile)
    }

    pub async fn delete_profile(
        &self, 
        patient_id: &str, 
        auth_token: &str
    ) -> Result<()> {
        debug!("Deleting health profile for patient: {}", patient_id);
        
        // First get the profile ID
        let profile_path = format!("/rest/v1/health_profiles?patient_id=eq.{}", patient_id);
        
        let profiles: Vec<Value> = self.supabase.request_with_headers(
            Method::GET,
            &profile_path,
            Some(auth_token),
            None,
            None,
        ).await?;
        
        if profiles.is_empty() {
            return Err(anyhow!("Health profile not found"));
        }
        
        // Delete the profile
        let delete_path = format!("/rest/v1/health_profiles?id=eq.{}", 
            profiles[0]["id"].as_str().unwrap_or(""));
        
        // Add headers to properly handle the response
        let mut headers = HeaderMap::new();
        headers.insert("Prefer", HeaderValue::from_static("return=minimal"));
        
        // Use request_with_headers with the proper return type
        let _: () = self.supabase.request_with_headers(
            Method::DELETE,
            &delete_path,
            Some(auth_token),
            None,
            Some(headers),
        ).await?;
        
        Ok(())
    }
    
}