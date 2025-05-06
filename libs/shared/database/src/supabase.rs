use anyhow::{Result, anyhow};
use reqwest::{
    Client, 
    header::{HeaderMap, HeaderValue, CONTENT_TYPE, AUTHORIZATION},
    Method,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tracing::{debug, error};

use shared_config::AppConfig;

pub struct SupabaseClient {
    client: Client,
    base_url: String,
    anon_key: String,
}

impl SupabaseClient {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            client: Client::new(),
            base_url: config.supabase_url.clone(),
            anon_key: config.supabase_anon_key.clone(),
        }
    }
    
    fn get_headers(&self, auth_token: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        
        headers.insert("apikey", HeaderValue::from_str(&self.anon_key).unwrap());
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        if let Some(token) = auth_token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token)).unwrap()
            );
        }
        
        headers
    }
    
    pub async fn request<T>(&self, method: Method, path: &str, 
                            auth_token: Option<&str>, body: Option<Value>) 
                            -> Result<T> 
    where T: DeserializeOwned {
        let url = format!("{}{}", self.base_url, path);
        debug!("Making request to {}", url);
        
        let headers = self.get_headers(auth_token);
        
        let mut req = self.client.request(method, &url)
            .headers(headers);
            
        if let Some(body_data) = body {
            req = req.json(&body_data);
        }
        
        let response = req.send().await?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            error!("API error ({}): {}", status, error_text);
            
            return Err(match status.as_u16() {
                401 | 403 => anyhow!("Authentication error: {}", error_text),
                404 => anyhow!("Resource not found: {}", error_text),
                _ => anyhow!("API error ({}): {}", status, error_text),
            });
        }
        
        let data = response.json::<T>().await?;
        Ok(data)
    }
    
    pub async fn get_user_profile(&self, _user_id: &str, auth_token: &str) -> Result<Value> {
        // Use the Supabase Auth API to get user data
        let path = "/auth/v1/user";
        
        self.request::<Value>(
            Method::GET,
            path,
            Some(auth_token),
            None,
        ).await
    }
    
    pub async fn get_health_profile(&self, user_id: &str, auth_token: &str) -> Result<Value> {
        let path = format!("/rest/v1/health_profiles?patient_id=eq.{}", user_id);
        
        let result: Vec<Value> = self.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;
        
        if result.is_empty() {
            // Return empty profile if none exists
            return Ok(json!({
                "patient_id": user_id,
                "exists": false
            }));
        }
        
        Ok(result[0].clone())
    }

    pub fn get_base_url(&self) -> &str {
        &self.base_url
    }

    // Method to get public URL for a storage path
    pub fn get_public_url(&self, storage_path: &str) -> String {
        format!("{}{}", self.base_url, storage_path)
    }
}