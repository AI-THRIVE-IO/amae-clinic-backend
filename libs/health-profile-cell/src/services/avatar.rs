use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, error};
use uuid::Uuid;
use std::str::FromStr;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

pub struct AvatarService {
    supabase: SupabaseClient,
}

impl AvatarService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
        }
    }
    
    pub async fn upload_avatar(
        &self, 
        patient_id: &str, 
        base64_image: &str,
        auth_token: &str
    ) -> Result<String> {
        debug!("Uploading avatar for patient: {}", patient_id);
        
        // Extract base64 data from format "data:image/jpeg;base64,/9j/4AAQ..."
        let parts: Vec<&str> = base64_image.split(',').collect();
        let base64_data = if parts.len() > 1 { parts[1] } else { base64_image };
        
        // Decode base64 data to bytes
        let image_data = BASE64.decode(base64_data)?;
        
        // Create a unique filename
        let file_ext = if base64_image.contains("image/png") {
            "png"
        } else if base64_image.contains("image/jpeg") || base64_image.contains("image/jpg") {
            "jpg"
        } else {
            "png" // Default to png
        };
        
        let filename = format!("avatars/{}/{}", patient_id, Uuid::new_v4().to_string());
        
        // Upload to Supabase storage
        let path = format!("/storage/v1/object/profiles/{}", filename);
        
        // Perform upload request directly
        let upload_result: Value = self.supabase.request(
            Method::POST,
            &path,
            Some(auth_token),
            Some(json!({
                "data": image_data,
                "contentType": format!("image/{}", file_ext)
            })),
        ).await?;
        
        // Extract public URL
        let public_url = format!(
            "{}/storage/v1/object/public/profiles/{}", 
            self.supabase.base_url, 
            filename
        );
        
        // Update the health profile with the new avatar URL
        let update_path = format!("/rest/v1/health_profiles?patient_id=eq.{}", patient_id);
        
        let update_result: Vec<Value> = self.supabase.request(
            Method::PATCH,
            &update_path,
            Some(auth_token),
            Some(json!({
                "avatar_url": public_url,
                "updated_at": chrono::Utc::now().to_rfc3339()
            })),
        ).await?;
        
        if update_result.is_empty() {
            return Err(anyhow!("Failed to update avatar URL in health profile"));
        }
        
        Ok(public_url)
    }
    
    pub async fn remove_avatar(
        &self, 
        patient_id: &str, 
        auth_token: &str
    ) -> Result<()> {
        debug!("Removing avatar for patient: {}", patient_id);
        
        // First get the current avatar URL
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
        
        let profile = &result[0];
        
        // If there's an avatar URL, extract the path and delete the file
        if let Some(avatar_url) = profile["avatar_url"].as_str() {
            // Extract filename from URL
            if let Some(filename) = avatar_url.split("profiles/").nth(1) {
                // Delete from storage
                let delete_path = format!("/storage/v1/object/profiles/{}", filename);
                
                let _: Value = self.supabase.request(
                    Method::DELETE,
                    &delete_path,
                    Some(auth_token),
                    None,
                ).await?;
            }
        }
        
        // Update profile to remove avatar URL
        let update_path = format!("/rest/v1/health_profiles?patient_id=eq.{}", patient_id);
        
        let _: Vec<Value> = self.supabase.request(
            Method::PATCH,
            &update_path,
            Some(auth_token),
            Some(json!({
                "avatar_url": null,
                "updated_at": chrono::Utc::now().to_rfc3339()
            })),
        ).await?;
        
        Ok(())
    }
}