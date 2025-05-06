use anyhow::{Result, anyhow};
use reqwest::{Client, header};
use serde_json::{json, Value};
use tracing::{debug, error};
use std::env;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{HealthProfile, CarePlanRequest, NutritionPlanRequest};

pub struct AiService {
    openai_api_key: String,
    supabase: SupabaseClient,
    http_client: Client,
}

impl AiService {
    pub fn new(config: &AppConfig) -> Result<Self> {
        let openai_api_key = env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow!("OPENAI_API_KEY environment variable not set"))?;
        
        Ok(Self {
            openai_api_key,
            supabase: SupabaseClient::new(config),
            http_client: Client::new(),
        })
    }
    
    async fn get_patient_data(&self, patient_id: &str, auth_token: &str) -> Result<Value> {
        // Fetch patient data
        let patient_path = format!("/rest/v1/patients?id=eq.{}", patient_id);
        
        let patient_result: Vec<Value> = self.supabase.request(
            reqwest::Method::GET,
            &patient_path,
            Some(auth_token),
            None,
        ).await?;
        
        if patient_result.is_empty() {
            return Err(anyhow!("Patient not found"));
        }
        
        // Fetch health profile
        let profile_path = format!("/rest/v1/health_profiles?patient_id=eq.{}", patient_id);
        
        let profile_result: Vec<Value> = self.supabase.request(
            reqwest::Method::GET,
            &profile_path,
            Some(auth_token),
            None,
        ).await?;
        
        let patient_data = patient_result[0].clone();
        
        // Combine data
        let mut combined_data = patient_data.clone();
        
        if !profile_result.is_empty() {
            let profile_data = profile_result[0].clone();
            
            // Add health profile fields to combined data
            if let (Some(patient_obj), Some(profile_obj)) = (combined_data.as_object_mut(), profile_data.as_object()) {
                for (key, value) in profile_obj {
                    // Skip ID and patient_id fields to avoid confusion
                    if key != "id" && key != "patient_id" {
                        patient_obj.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        
        // Calculate age if date_of_birth is present
        if let Some(dob_str) = combined_data["date_of_birth"].as_str() {
            if let Ok(dob) = chrono::NaiveDate::parse_from_str(dob_str, "%Y-%m-%d") {
                let today = chrono::Local::now().naive_local().date();
                let age = today.years_since(dob).unwrap_or(0);
                
                if let Some(obj) = combined_data.as_object_mut() {
                    obj.insert("age".to_string(), json!(age));
                }
            }
        }
        
        Ok(combined_data)
    }
    
    pub async fn generate_nutrition_plan(
        &self, 
        patient_id: &str, 
        auth_token: &str
    ) -> Result<Value> {
        debug!("Generating nutrition plan for patient: {}", patient_id);
        
        // Get patient data
        let patient_data = self.get_patient_data(patient_id, auth_token).await?;
        
        // Create OpenAI API request
        let prompt = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": "You are a clinical nutritionist creating a personalized nutrition plan. Provide evidence-based dietary recommendations based on the patient's health profile. Include specific dietary goals, foods to include/avoid, meal patterns, and general nutritional advice."
                },
                {
                    "role": "user",
                    "content": format!("Create a personalized nutrition plan based on this patient profile: {}", patient_data)
                }
            ],
            "temperature": 0.5
        });
        
        // Call OpenAI API
        let response = self.http_client.post("https://api.openai.com/v1/chat/completions")
            .header(header::AUTHORIZATION, format!("Bearer {}", self.openai_api_key))
            .header(header::CONTENT_TYPE, "application/json")
            .json(&prompt)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("OpenAI API error: {}", error_text));
        }
        
        let ai_response: Value = response.json().await?;
        let nutrition_text = ai_response["choices"][0]["message"]["content"].as_str()
            .ok_or_else(|| anyhow!("Invalid OpenAI response format"))?
            .to_string();
        
        // Save nutrition plan to database
        let nutrition_path = "/rest/v1/nutrition_plans";
        
        let plan_data = json!({
            "patient_id": patient_id,
            "ai_generated": true,
            "goal": extract_goal(&nutrition_text)?,
            "plan_details": nutrition_text,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "last_updated": chrono::Utc::now().to_rfc3339()
        });
        
        let plan_result: Vec<Value> = self.supabase.request(
            reqwest::Method::POST,
            nutrition_path,
            Some(auth_token),
            Some(plan_data.clone()),
        ).await?;
        
        if plan_result.is_empty() {
            return Err(anyhow!("Failed to save nutrition plan"));
        }
        
        Ok(plan_result[0].clone())
    }
    
    pub async fn generate_care_plan(
        &self, 
        patient_id: &str, 
        condition: &str,
        auth_token: &str
    ) -> Result<Value> {
        debug!("Generating care plan for patient: {} with condition: {}", patient_id, condition);
        
        // Get patient data
        let patient_data = self.get_patient_data(patient_id, auth_token).await?;
        
        // Create OpenAI API request
        let prompt = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": "You are a healthcare AI assistant creating a personalized care plan for a patient with a specific condition. Your care plan should include dietary guidance, physical activity recommendations, monitoring instructions, and general lifestyle advice."
                },
                {
                    "role": "user",
                    "content": format!("Create a comprehensive care plan for a patient with {} based on this patient profile: {}", condition, patient_data)
                }
            ],
            "temperature": 0.5
        });
        
        // Call OpenAI API
        let response = self.http_client.post("https://api.openai.com/v1/chat/completions")
            .header(header::AUTHORIZATION, format!("Bearer {}", self.openai_api_key))
            .header(header::CONTENT_TYPE, "application/json")
            .json(&prompt)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("OpenAI API error: {}", error_text));
        }
        
        let ai_response: Value = response.json().await?;
        let care_plan_text = ai_response["choices"][0]["message"]["content"].as_str()
            .ok_or_else(|| anyhow!("Invalid OpenAI response format"))?
            .to_string();
        
        // Try to extract structured sections
        let sections = extract_care_plan_sections(&care_plan_text)?;
        
        // Save care plan to database
        let care_plan_path = "/rest/v1/condition_care_plans";
        
        let plan_data = json!({
            "patient_id": patient_id,
            "condition": condition,
            "diet_guidance": sections.get("diet_guidance").unwrap_or(&"").to_string(),
            "activity_targets": sections.get("activity_targets").unwrap_or(&"").to_string(),
            "monitoring_instructions": sections.get("monitoring_instructions").unwrap_or(&"").to_string(),
            "ai_recommendations": care_plan_text,
            "last_updated": chrono::Utc::now().to_rfc3339()
        });
        
        let plan_result: Vec<Value> = self.supabase.request(
            reqwest::Method::POST,
            care_plan_path,
            Some(auth_token),
            Some(plan_data.clone()),
        ).await?;
        
        if plan_result.is_empty() {
            return Err(anyhow!("Failed to save care plan"));
        }
        
        Ok(plan_result[0].clone())
    }
}

// Helper function to extract the goal from nutrition plan text
fn extract_goal(text: &str) -> Result<String> {
    // Try to find a goal section
    if let Some(goal_idx) = text.to_lowercase().find("goal") {
        let after_goal = &text[goal_idx..];
        if let Some(end_idx) = after_goal.find("\n\n") {
            let goal_text = &after_goal[..end_idx];
            return Ok(goal_text.trim().to_string());
        }
    }
    
    // If no goal section found, return the first paragraph
    if let Some(end_idx) = text.find("\n\n") {
        let first_para = &text[..end_idx];
        return Ok(first_para.trim().to_string());
    }
    
    // Fallback to first 100 chars
    let end = std::cmp::min(text.len(), 100);
    Ok(text[..end].trim().to_string())
}

// Helper function to extract sections from care plan text
fn extract_care_plan_sections(text: &str) -> Result<std::collections::HashMap<String, String>> {
    let mut sections = std::collections::HashMap::new();
    
    // Common section keywords
    let section_keys = [
        ("diet", "diet_guidance"),
        ("nutrition", "diet_guidance"),
        ("food", "diet_guidance"),
        ("physical activity", "activity_targets"),
        ("exercise", "activity_targets"),
        ("activity", "activity_targets"),
        ("monitoring", "monitoring_instructions"),
        ("track", "monitoring_instructions"),
    ];
    
    let lines: Vec<&str> = text.split('\n').collect();
    let mut current_section = "";
    let mut current_content = Vec::new();
    
    for line in lines {
        let line_lower = line.to_lowercase();
        
        // Check if this line starts a new section
        let mut is_section_header = false;
        for (keyword, section_name) in &section_keys {
            if line_lower.contains(keyword) && (line.contains(':') || line.ends_with(':')) {
                // End previous section if any
                if !current_section.is_empty() && !current_content.is_empty() {
                    sections.insert(current_section.to_string(), current_content.join("\n"));
                    current_content.clear();
                }
                
                current_section = section_name;
                is_section_header = true;
                break;
            }
        }
        
        // Add content to current section
        if !current_section.is_empty() && !is_section_header {
            current_content.push(line);
        }
    }
    
    // Add final section
    if !current_section.is_empty() && !current_content.is_empty() {
        sections.insert(current_section.to_string(), current_content.join("\n"));
    }
    
    Ok(sections)
}