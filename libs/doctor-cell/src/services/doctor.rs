use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, error};
use uuid::Uuid;
use chrono::Utc;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::{
    Doctor, DoctorSpecialty, DoctorStats, DoctorSearchFilters,
    CreateDoctorRequest, UpdateDoctorRequest, CreateSpecialtyRequest,
    DoctorImageUpload, AvailableSlot
};

pub struct DoctorService {
    supabase: SupabaseClient,
}

impl DoctorService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
        }
    }

    pub fn supabase(&self) -> &SupabaseClient {
        &self.supabase
    }

    /// Create a new doctor profile
    pub async fn create_doctor(
        &self,
        request: CreateDoctorRequest,
        auth_token: &str,
    ) -> Result<Doctor> {
        debug!("Creating new doctor profile for: {}", request.email);

        // Validate timezone
        if !self.is_valid_timezone(&request.timezone) {
            return Err(anyhow!("Invalid timezone: {}", request.timezone));
        }

        // Check if doctor with email already exists
        let existing_check_path = format!("/rest/v1/doctors?email=eq.{}", request.email);
        let existing: Vec<Value> = self.supabase.request(
            Method::GET,
            &existing_check_path,
            Some(auth_token),
            None,
        ).await?;

        if !existing.is_empty() {
            return Err(anyhow!("Doctor with email {} already exists", request.email));
        }

        let doctor_data = json!({
            "full_name": request.full_name,
            "email": request.email,
            "specialty": request.specialty,
            "bio": request.bio,
            "license_number": request.license_number,
            "years_experience": request.years_experience,
            "timezone": request.timezone,
            "is_verified": false, // Requires admin verification
            "is_available": true,
            "rating": 0.0,
            "total_consultations": 0,
            "created_at": Utc::now().to_rfc3339(),
            "updated_at": Utc::now().to_rfc3339()
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            "/rest/v1/doctors",
            Some(auth_token),
            Some(doctor_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to create doctor profile"));
        }

        let doctor: Doctor = serde_json::from_value(result[0].clone())?;
        debug!("Doctor profile created successfully with ID: {}", doctor.id);

        Ok(doctor)
    }

    /// Get doctor by ID
    pub async fn get_doctor(
        &self,
        doctor_id: &str,
        auth_token: &str,
    ) -> Result<Doctor> {
        debug!("Fetching doctor profile: {}", doctor_id);

        let path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Doctor not found"));
        }

        let doctor: Doctor = serde_json::from_value(result[0].clone())?;
        Ok(doctor)
    }

    /// Update doctor profile
    pub async fn update_doctor(
        &self,
        doctor_id: &str,
        request: UpdateDoctorRequest,
        auth_token: &str,
    ) -> Result<Doctor> {
        debug!("Updating doctor profile: {}", doctor_id);

        // Validate timezone if provided
        if let Some(ref timezone) = request.timezone {
            if !self.is_valid_timezone(timezone) {
                return Err(anyhow!("Invalid timezone: {}", timezone));
            }
        }

        // Build update object with only provided fields
        let mut update_data = serde_json::Map::new();
        
        if let Some(name) = request.full_name {
            update_data.insert("full_name".to_string(), json!(name));
        }
        if let Some(bio) = request.bio {
            update_data.insert("bio".to_string(), json!(bio));
        }
        if let Some(specialty) = request.specialty {
            update_data.insert("specialty".to_string(), json!(specialty));
        }
        if let Some(experience) = request.years_experience {
            update_data.insert("years_experience".to_string(), json!(experience));
        }
        if let Some(timezone) = request.timezone {
            update_data.insert("timezone".to_string(), json!(timezone));
        }
        if let Some(available) = request.is_available {
            update_data.insert("is_available".to_string(), json!(available));
        }
        
        update_data.insert("updated_at".to_string(), json!(Utc::now().to_rfc3339()));

        let path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
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
            return Err(anyhow!("Failed to update doctor profile"));
        }

        let updated_doctor: Doctor = serde_json::from_value(result[0].clone())?;
        Ok(updated_doctor)
    }

    /// Search doctors with filters
    pub async fn search_doctors(
        &self,
        filters: DoctorSearchFilters,
        auth_token: &str,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<Doctor>> {
        debug!("Searching doctors with filters: {:?}", filters);

        let mut query_parts = vec!["is_available=eq.true".to_string()];

        // Add filters
        if let Some(specialty) = filters.specialty {
            query_parts.push(format!("specialty=ilike.%{}%", specialty));
        }
        if let Some(min_exp) = filters.min_experience {
            query_parts.push(format!("years_experience=gte.{}", min_exp));
        }
        if let Some(min_rating) = filters.min_rating {
            query_parts.push(format!("rating=gte.{}", min_rating));
        }
        if filters.is_verified_only.unwrap_or(false) {
            query_parts.push("is_verified=eq.true".to_string());
        }

        let mut path = format!("/rest/v1/doctors?{}", query_parts.join("&"));
        
        // Add ordering and pagination
        path.push_str("&order=rating.desc,total_consultations.desc");
        
        if let Some(limit_val) = limit {
            path.push_str(&format!("&limit={}", limit_val));
        }
        if let Some(offset_val) = offset {
            path.push_str(&format!("&offset={}", offset_val));
        }

        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        let doctors: Vec<Doctor> = result.into_iter()
            .map(|doc| serde_json::from_value(doc))
            .collect::<std::result::Result<Vec<Doctor>, _>>()?;

        Ok(doctors)
    }

    /// Get doctor specialties
    pub async fn get_doctor_specialties(
        &self,
        doctor_id: &str,
        auth_token: &str,
    ) -> Result<Vec<DoctorSpecialty>> {
        debug!("Fetching specialties for doctor: {}", doctor_id);

        let path = format!("/rest/v1/doctor_specialties?doctor_id=eq.{}&order=is_primary.desc,created_at.asc", doctor_id);
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;

        let specialties: Vec<DoctorSpecialty> = result.into_iter()
            .map(|spec| serde_json::from_value(spec))
            .collect::<std::result::Result<Vec<DoctorSpecialty>, _>>()?;

        Ok(specialties)
    }

    /// Add specialty to doctor
    pub async fn add_doctor_specialty(
        &self,
        doctor_id: &str,
        request: CreateSpecialtyRequest,
        auth_token: &str,
    ) -> Result<DoctorSpecialty> {
        debug!("Adding specialty to doctor: {}", doctor_id);

        // If this is marked as primary, unmark other primary specialties
        if request.is_primary.unwrap_or(false) {
            let update_path = format!("/rest/v1/doctor_specialties?doctor_id=eq.{}", doctor_id);
            let update_data = json!({
                "is_primary": false,
                "updated_at": Utc::now().to_rfc3339()
            });

            let _: Vec<Value> = self.supabase.request(
                Method::PATCH,
                &update_path,
                Some(auth_token),
                Some(update_data),
            ).await?;
        }

        let specialty_data = json!({
            "doctor_id": doctor_id,
            "specialty_name": request.specialty_name,
            "sub_specialty": request.sub_specialty,
            "certification_number": request.certification_number,
            "certification_date": request.certification_date,
            "is_primary": request.is_primary.unwrap_or(false),
            "created_at": Utc::now().to_rfc3339()
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::POST,
            "/rest/v1/doctor_specialties",
            Some(auth_token),
            Some(specialty_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to add specialty"));
        }

        let specialty: DoctorSpecialty = serde_json::from_value(result[0].clone())?;
        Ok(specialty)
    }

    /// Upload doctor profile image
    pub async fn upload_profile_image(
        &self,
        doctor_id: &str,
        upload: DoctorImageUpload,
        auth_token: &str,
    ) -> Result<String> {
        debug!("Uploading profile image for doctor: {}", doctor_id);

        // Extract base64 data
        let parts: Vec<&str> = upload.file_data.split(',').collect();
        let base64_data = if parts.len() > 1 { parts[1] } else { &upload.file_data };

        // Decode base64 data
        let image_data = BASE64.decode(base64_data)?;

        // Determine file extension
        let file_ext = if upload.file_data.contains("image/png") {
            "png"
        } else if upload.file_data.contains("image/jpeg") || upload.file_data.contains("image/jpg") {
            "jpg"
        } else {
            "png"
        };

        let filename = format!("doctor-profiles/{}/{}.{}", doctor_id, Uuid::new_v4(), file_ext);

        // Upload to storage
        let upload_path = format!("/storage/v1/object/profiles/{}", filename);
        let upload_data = json!({
            "data": image_data,
            "contentType": format!("image/{}", file_ext)
        });

        let _: Value = self.supabase.request(
            Method::POST,
            &upload_path,
            Some(auth_token),
            Some(upload_data),
        ).await?;

        // Get public URL
        let public_url = self.supabase.get_public_url(&format!("/storage/v1/object/public/profiles/{}", filename));

        // Update doctor profile with image URL
        let update_path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
        let update_data = json!({
            "profile_image_url": public_url,
            "updated_at": Utc::now().to_rfc3339()
        });

        let _: Vec<Value> = self.supabase.request(
            Method::PATCH,
            &update_path,
            Some(auth_token),
            Some(update_data),
        ).await?;

        Ok(public_url)
    }

    /// Get doctor statistics
    pub async fn get_doctor_stats(
        &self,
        doctor_id: &str,
        auth_token: &str,
    ) -> Result<DoctorStats> {
        debug!("Fetching statistics for doctor: {}", doctor_id);

        // Get basic doctor info
        let doctor = self.get_doctor(doctor_id, auth_token).await?;

        // Get appointment statistics
        let appointments_path = format!("/rest/v1/appointments?doctor_id=eq.{}", doctor_id);
        let appointments: Vec<Value> = self.supabase.request(
            Method::GET,
            &appointments_path,
            Some(auth_token),
            None,
        ).await?;

        let total_appointments = appointments.len() as i32;
        let completed_appointments = appointments.iter()
            .filter(|apt| apt["status"].as_str() == Some("completed"))
            .count() as i32;

        // Calculate average session duration (would need actual duration data)
        let avg_session_duration_minutes = 30; // Placeholder

        // Get specialties
        let specialties = self.get_doctor_specialties(doctor_id, auth_token).await?;

        // Get next available slot (would integrate with availability service)
        let next_available_slot: Option<AvailableSlot> = None; // Placeholder

        Ok(DoctorStats {
            total_appointments,
            completed_appointments,
            avg_session_duration_minutes,
            avg_rating: doctor.rating,
            total_reviews: doctor.total_consultations,
            specialties,
            next_available_slot,
        })
    }

    /// Verify doctor (admin only)
    pub async fn verify_doctor(
        &self,
        doctor_id: &str,
        is_verified: bool,
        auth_token: &str,
    ) -> Result<Doctor> {
        debug!("Setting doctor verification status: {} -> {}", doctor_id, is_verified);

        let update_data = json!({
            "is_verified": is_verified,
            "updated_at": Utc::now().to_rfc3339()
        });

        let path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Prefer", reqwest::header::HeaderValue::from_static("return=representation"));

        let result: Vec<Value> = self.supabase.request_with_headers(
            Method::PATCH,
            &path,
            Some(auth_token),
            Some(update_data),
            Some(headers),
        ).await?;

        if result.is_empty() {
            return Err(anyhow!("Failed to update doctor verification status"));
        }

        let updated_doctor: Doctor = serde_json::from_value(result[0].clone())?;
        Ok(updated_doctor)
    }

    /// Delete doctor profile (admin only)
    pub async fn delete_doctor(
        &self,
        doctor_id: &str,
        auth_token: &str,
    ) -> Result<()> {
        debug!("Deleting doctor profile: {}", doctor_id);

        // Check for existing appointments
        let appointments_path = format!("/rest/v1/appointments?doctor_id=eq.{}&status=in.(pending,confirmed)", doctor_id);
        let active_appointments: Vec<Value> = self.supabase.request(
            Method::GET,
            &appointments_path,
            Some(auth_token),
            None,
        ).await?;

        if !active_appointments.is_empty() {
            return Err(anyhow!("Cannot delete doctor with active appointments"));
        }

        // Delete doctor profile (cascade will handle related records)
        let path = format!("/rest/v1/doctors?id=eq.{}", doctor_id);
        let _: Vec<Value> = self.supabase.request(
            Method::DELETE,
            &path,
            Some(auth_token),
            None,
        ).await?;

        Ok(())
    }

    /// Helper function to validate timezone
    fn is_valid_timezone(&self, timezone: &str) -> bool {
        // Basic timezone validation - in production, use a proper timezone library
        let valid_timezones = [
            "UTC", "US/Eastern", "US/Central", "US/Mountain", "US/Pacific",
            "Europe/London", "Europe/Paris", "Europe/Berlin", "Asia/Tokyo",
            "Asia/Shanghai", "Australia/Sydney", "America/New_York",
            "America/Chicago", "America/Denver", "America/Los_Angeles"
        ];
        
        valid_timezones.contains(&timezone)
    }
}