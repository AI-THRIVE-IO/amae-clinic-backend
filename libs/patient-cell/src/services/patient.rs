use anyhow::{Result, anyhow};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, error};
use uuid::Uuid;
use chrono::Utc;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{Patient, CreatePatientRequest, UpdatePatientRequest, PatientSearchQuery, PatientError};

pub struct PatientService {
    supabase: SupabaseClient,
}

impl PatientService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
        }
    }

    pub async fn create_patient(
        &self,
        request: CreatePatientRequest,
        auth_token: &str,
    ) -> Result<Patient> {
        debug!("Creating new patient profile for: {}", request.email);

        // Check if patient with email already exists
        let existing_check_path = format!("/rest/v1/patients?email=eq.{}", request.email);
        let existing: Vec<Value> = self.supabase.request(
            Method::GET,
            &existing_check_path,
            Some(auth_token),
            None,
        ).await?;

        if !existing.is_empty() {
            return Err(anyhow!("Patient with email {} already exists", request.email));
        }

        let patient_data = json!({
            "first_name": request.first_name,
            "last_name": request.last_name,
            "email": request.email,
            "phone_number": request.phone_number,
            "address": request.address,
            "eircode": request.eircode,
            "date_of_birth": request.date_of_birth.format("%Y-%m-%d").to_string(),
            "birth_gender": request.birth_gender,
            "ppsn": request.ppsn,
            "allergies": request.allergies,
            "chronic_conditions": request.chronic_conditions,
            "current_medications": request.current_medications,
            "smoking_status": request.smoking_status,
            "alcohol_use": request.alcohol_use,
            "surgery_history": request.surgery_history,
            "created_at": Utc::now().to_rfc3339(),
            "updated_at": Utc::now().to_rfc3339()
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            "/rest/v1/patients",
            Some(auth_token),
            Some(patient_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to create patient profile"));
        }

        let patient: Patient = serde_json::from_value(result[0].clone())?;
        debug!("Patient profile created successfully with ID: {}", patient.id);

        Ok(patient)
    }

    pub async fn get_patient(
        &self,
        patient_id: &str,
        auth_token: &str,
    ) -> Result<Patient> {
        debug!("Fetching patient profile: {}", patient_id);

        let path = format!("/rest/v1/patients?id=eq.{}", patient_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Patient not found"));
        }

        let patient: Patient = serde_json::from_value(result[0].clone())?;
        Ok(patient)
    }

    pub async fn update_patient(
        &self,
        patient_id: &str,
        request: UpdatePatientRequest,
        auth_token: &str,
    ) -> Result<Patient> {
        debug!("Updating patient profile: {}", patient_id);

        let mut update_data = serde_json::Map::new();
        
        if let Some(first_name) = request.first_name {
            update_data.insert("first_name".to_string(), json!(first_name));
        }
        if let Some(last_name) = request.last_name {
            update_data.insert("last_name".to_string(), json!(last_name));
        }
        if let Some(phone_number) = request.phone_number {
            update_data.insert("phone_number".to_string(), json!(phone_number));
        }
        if let Some(address) = request.address {
            update_data.insert("address".to_string(), json!(address));
        }
        if let Some(eircode) = request.eircode {
            update_data.insert("eircode".to_string(), json!(eircode));
        }
        if let Some(allergies) = request.allergies {
            update_data.insert("allergies".to_string(), json!(allergies));
        }
        if let Some(chronic_conditions) = request.chronic_conditions {
            update_data.insert("chronic_conditions".to_string(), json!(chronic_conditions));
        }
        if let Some(current_medications) = request.current_medications {
            update_data.insert("current_medications".to_string(), json!(current_medications));
        }
        if let Some(smoking_status) = request.smoking_status {
            update_data.insert("smoking_status".to_string(), json!(smoking_status));
        }
        if let Some(alcohol_use) = request.alcohol_use {
            update_data.insert("alcohol_use".to_string(), json!(alcohol_use));
        }
        if let Some(surgery_history) = request.surgery_history {
            update_data.insert("surgery_history".to_string(), json!(surgery_history));
        }
        
        update_data.insert("updated_at".to_string(), json!(Utc::now().to_rfc3339()));

        let path = format!("/rest/v1/patients?id=eq.{}", patient_id);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::PATCH,
            &path,
            Some(auth_token),
            Some(Value::Object(update_data)),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to update patient profile"));
        }

        let updated_patient: Patient = serde_json::from_value(result[0].clone())?;
        Ok(updated_patient)
    }

    pub async fn search_patients(
        &self,
        query: PatientSearchQuery,
        auth_token: &str,
    ) -> Result<Vec<Patient>> {
        debug!("Searching patients with query: {:?}", query);

        let mut query_parts = vec![];

        if let Some(name) = query.name {
            query_parts.push(format!("or=(first_name.ilike.%{}%,last_name.ilike.%{}%)", name, name));
        }
        if let Some(email) = query.email {
            query_parts.push(format!("email=ilike.%{}%", email));
        }
        if let Some(phone) = query.phone {
            query_parts.push(format!("phone_number=ilike.%{}%", phone));
        }
        if let Some(ppsn) = query.ppsn {
            query_parts.push(format!("ppsn=eq.{}", ppsn));
        }

        let query_string = if query_parts.is_empty() {
            String::new()
        } else {
            format!("?{}", query_parts.join("&"))
        };

        let limit = query.limit.unwrap_or(50);
        let offset = query.offset.unwrap_or(0);
        let separator = if query_string.is_empty() { "?" } else { "&" };
        let path = format!("/rest/v1/patients{}{}limit={}&offset={}", 
            query_string, separator, limit, offset);

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        let patients: Vec<Patient> = result
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(patients)
    }
}