use std::sync::Arc;
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use uuid::Uuid;

use shared_config::AppConfig;
use shared_models::auth::User;

pub struct TestConfig {
    pub jwt_secret: String,
    pub supabase_url: String,
    pub supabase_anon_key: String,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "test-secret-key-for-jwt-validation-must-be-long-enough".to_string(),
            supabase_url: "http://localhost:54321".to_string(),
            supabase_anon_key: "test-anon-key".to_string(),
        }
    }
}

impl TestConfig {
    pub fn to_app_config(&self) -> AppConfig {
        AppConfig {
            supabase_url: self.supabase_url.clone(),
            supabase_anon_key: self.supabase_anon_key.clone(),
            supabase_jwt_secret: self.jwt_secret.clone(),
            cloudflare_realtime_app_id: "test-app-id".to_string(),
            cloudflare_realtime_api_token: "test-token".to_string(),
            cloudflare_realtime_base_url: "https://test.cloudflare.com/v1".to_string(),
        }
    }
    
    pub fn to_arc(&self) -> Arc<AppConfig> {
        Arc::new(self.to_app_config())
    }
}

pub struct TestUser {
    pub id: String,
    pub email: String,
    pub role: String,
}

impl Default for TestUser {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            email: "test@example.com".to_string(),
            role: "patient".to_string(),
        }
    }
}

impl TestUser {
    pub fn new(email: &str, role: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            email: email.to_string(),
            role: role.to_string(),
        }
    }

    pub fn doctor(email: &str) -> Self {
        Self::new(email, "doctor")
    }

    pub fn patient(email: &str) -> Self {
        Self::new(email, "patient")
    }

    pub fn admin(email: &str) -> Self {
        Self::new(email, "admin")
    }

    pub fn to_user(&self) -> User {
        User {
            id: self.id.clone(),
            email: Some(self.email.clone()),
            role: Some(self.role.clone()),
            metadata: None,
            created_at: Some(Utc::now()),
        }
    }
}

pub struct JwtTestUtils;

impl JwtTestUtils {
    pub fn create_test_token(user: &TestUser, secret: &str, exp_hours: Option<i64>) -> String {
        let now = Utc::now();
        let exp = now + Duration::hours(exp_hours.unwrap_or(24));
        
        // Create proper JWT header
        let header = json!({
            "alg": "HS256",
            "typ": "JWT"
        });
        
        // Create proper JWT payload matching expected claims
        let payload = json!({
            "sub": user.id,
            "email": user.email,
            "role": user.role,
            "iat": now.timestamp() as u64,
            "exp": exp.timestamp() as u64,
            "aud": "authenticated"
        });
        
        // CRITICAL: Encode binary data, not JSON strings
        let header_encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let payload_encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        
        // Create signing input
        let signing_input = format!("{}.{}", header_encoded, payload_encoded);
        
        // Sign with HMAC-SHA256
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .expect("Invalid JWT secret");
        mac.update(signing_input.as_bytes());
        let signature = mac.finalize().into_bytes();
        
        // Encode signature
        let signature_encoded = URL_SAFE_NO_PAD.encode(&signature);
        
        // Return complete JWT token
        format!("{}.{}.{}", header_encoded, payload_encoded, signature_encoded)
    }
    
    // Helper for creating expired tokens (for testing)
    pub fn create_expired_token(user: &TestUser, secret: &str) -> String {
        Self::create_test_token(user, secret, Some(-1)) // Expired 1 hour ago
    }
    
    // Helper for creating invalid signature tokens (for testing)
    pub fn create_invalid_signature_token(user: &TestUser, _secret: &str) -> String {
        let header = json!({"alg": "HS256", "typ": "JWT"});
        let payload = json!({
            "sub": user.id,
            "email": user.email,
            "role": user.role,
            "iat": Utc::now().timestamp(),
            "exp": (Utc::now() + Duration::hours(24)).timestamp()
        });
        
        let header_encoded = URL_SAFE_NO_PAD.encode(header.to_string());
        let payload_encoded = URL_SAFE_NO_PAD.encode(payload.to_string());
        let invalid_signature = URL_SAFE_NO_PAD.encode("invalid_signature");
        
        format!("{}.{}.{}", header_encoded, payload_encoded, invalid_signature)
    }
}
pub struct MockSupabaseResponses;

impl MockSupabaseResponses {
    pub fn user_profile_response(user_id: &str) -> serde_json::Value {
        json!({
            "id": user_id,
            "email": "test@example.com",
            "first_name": "Test",
            "last_name": "User",
            "avatar_url": null,
            "phone": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }
    
    pub fn health_profile_response(user_id: &str) -> serde_json::Value {
        json!({
            "id": Uuid::new_v4(),
            "user_id": user_id,
            "date_of_birth": "1990-01-01",
            "gender": "Other",
            "blood_type": null,
            "allergies": [],
            "medications": [],
            "conditions": [],
            "emergency_contact": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }
    
    pub fn doctor_response(id: &str, email: &str, name: &str, specialty: &str) -> serde_json::Value {
        let name_parts: Vec<&str> = name.split_whitespace().collect();
        let (first_name, last_name) = if name_parts.len() >= 2 {
            (name_parts[0], name_parts[1..].join(" "))
        } else {
            (name_parts.get(0).copied().unwrap_or("Doctor"), "User".to_string())
        };
        
        json!({
            "id": id,
            "first_name": first_name,
            "last_name": last_name,
            "email": email,
            "specialty": specialty,
            "bio": format!("Experienced {} practitioner", specialty),
            "years_experience": 10,
            "rating": 4.5,
            "total_consultations": 150,
            "is_available": true,
            "is_verified": true,
            "timezone": "UTC",
            "phone_number": "+1234567890",
            "license_number": "LIC123456",
            "medical_school": "Medical University",
            "residency": "General Hospital",
            "certifications": [specialty],
            "languages": ["English"],
            "profile_image_url": null,
            "available_days": [1, 2, 3, 4, 5],
            "date_of_birth": "1980-01-01",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }
    
    pub fn availability_response(id: &str, doctor_id: &str, day: i32) -> serde_json::Value {
        json!({
            "id": id,
            "doctor_id": doctor_id,
            "day_of_week": day,
            "start_time": "09:00:00",
            "end_time": "17:00:00",
            "duration_minutes": 30,
            "timezone": "UTC",
            "appointment_type": "consultation",
            "buffer_minutes": 0,
            "max_concurrent_appointments": 1,
            "is_recurring": true,
            "specific_date": null,
            "is_available": true,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }
    
     pub fn patient_response(id: &str, email: &str, name: &str) -> serde_json::Value {
        let name_parts: Vec<&str> = name.split_whitespace().collect();
        let (first_name, last_name) = if name_parts.len() >= 2 {
            (name_parts[0], name_parts[1..].join(" "))
        } else {
            (name_parts.get(0).copied().unwrap_or("Patient"), "User".to_string())
        };
        
        json!({
            "id": id,
            "first_name": first_name,
            "last_name": last_name,
            "email": email,
            "date_of_birth": "1990-01-01",
            "gender": "male",
            "phone_number": "+1234567890",
            "address": "123 Test Street",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }
    
    pub fn appointment_response(patient_id: &str, doctor_id: &str) -> serde_json::Value {
        json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "patient_id": patient_id,
            "doctor_id": doctor_id,
            "appointment_date": "2024-12-25T10:00:00Z",
            "status": "confirmed",
            "appointment_type": "general_consultation",
            "duration_minutes": 30,
            "timezone": "UTC",
            "scheduled_start_time": "2024-12-25T10:00:00Z",
            "scheduled_end_time": "2024-12-25T10:30:00Z",
            "actual_start_time": null,
            "actual_end_time": null,
            "notes": null,
            "patient_notes": "First consultation",
            "doctor_notes": null,
            "prescription_issued": false,
            "medical_certificate_issued": false,
            "report_generated": false,
            "video_conference_link": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }
    
    pub fn doctor_profile_response(id: &str) -> serde_json::Value {
        json!({
            "id": id,
            "first_name": "Dr. Test",
            "last_name": format!("Doctor {}", id),
            "email": format!("doctor{}@example.com", id),
            "specialty": "General Medicine",
            "bio": "Experienced physician",
            "years_experience": 10,
            "rating": 4.5,
            "total_consultations": 150,
            "is_available": true,
            "is_verified": true,
            "timezone": "UTC",
            "phone_number": "+1234567890",
            "license_number": "LIC123456",
            "medical_school": "Medical University",
            "residency": "General Hospital",
            "certifications": ["General Medicine"],
            "languages": ["English"],
            "profile_image_url": null,
            "available_days": [1, 2, 3, 4, 5],
            "date_of_birth": "1980-01-01",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }
    
    pub fn error_response(message: &str, code: &str) -> serde_json::Value {
        json!({
            "error": {
                "message": message,
                "code": code
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_creation() {
        let config = TestConfig::default();
        let app_config = config.to_app_config();
        
        assert_eq!(app_config.supabase_url, "http://localhost:54321");
        assert_eq!(app_config.supabase_anon_key, "test-anon-key");
        assert!(!app_config.supabase_jwt_secret.is_empty());
    }
    
    #[test]
    fn test_user_creation() {
        let user = TestUser::doctor("doc@example.com");
        assert_eq!(user.email, "doc@example.com");
        assert_eq!(user.role, "doctor");
        
        let user_model = user.to_user();
        assert_eq!(user_model.email, Some(user.email.clone()));
        assert_eq!(user_model.role, Some(user.role.clone()));
        assert_eq!(user_model.id, user.id);
    }
    
    #[test]
    fn test_jwt_token_creation() {
        let user = TestUser::default();
        let secret = "test-secret";
        let token = JwtTestUtils::create_test_token(&user, secret, Some(1));
        
        assert!(token.contains('.'));
        assert_eq!(token.split('.').count(), 3);
    }
    
    #[test]
    fn test_app_config_has_cloudflare_fields() {
        let config = TestConfig::default().to_app_config();
        
        // This test will fail to compile if the fields don't exist
        assert!(!config.cloudflare_realtime_app_id.is_empty());
        assert!(!config.cloudflare_realtime_api_token.is_empty());
        assert!(!config.cloudflare_realtime_base_url.is_empty());
    }
}