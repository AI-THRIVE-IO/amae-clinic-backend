use std::sync::Arc;
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose};
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
        
        let header = json!({
            "alg": "HS256",
            "typ": "JWT"
        });
        
        let payload = json!({
            "sub": user.id,
            "email": user.email,
            "role": user.role,
            "iat": now.timestamp(),
            "exp": exp.timestamp()
        });
        
        let header_encoded = general_purpose::URL_SAFE_NO_PAD.encode(header.to_string());
        let payload_encoded = general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
        
        let signing_input = format!("{}.{}", header_encoded, payload_encoded);
        
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(signing_input.as_bytes());
        let signature = mac.finalize().into_bytes();
        let signature_encoded = general_purpose::URL_SAFE_NO_PAD.encode(signature);
        
        format!("{}.{}", signing_input, signature_encoded)
    }
    
    pub fn create_expired_token(user: &TestUser, secret: &str) -> String {
        Self::create_test_token(user, secret, Some(-1))
    }
    
    pub fn create_invalid_signature_token(user: &TestUser) -> String {
        Self::create_test_token(user, "wrong-secret", Some(24))
    }
    
    pub fn create_malformed_token() -> String {
        "invalid.token.format".to_string()
    }
}

pub struct MockSupabaseResponses;

impl MockSupabaseResponses {
    pub fn user_profile_response(user_id: &str) -> serde_json::Value {
        json!({
            "id": user_id,
            "email": "test@example.com",
            "full_name": "Test User",
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
    
    pub fn doctor_profile_response(user_id: &str) -> serde_json::Value {
        json!({
            "id": Uuid::new_v4(),
            "user_id": user_id,
            "specialty": "General Practice",
            "license_number": "MD123456",
            "experience_years": 10,
            "education": "Medical University",
            "certifications": ["Board Certified"],
            "languages": ["English", "Spanish"],
            "bio": "Experienced general practitioner",
            "consultation_fee": 150.00,
            "is_available": true,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }
    
    pub fn appointment_response(user_id: &str, doctor_id: &str) -> serde_json::Value {
        json!({
            "id": Uuid::new_v4(),
            "patient_id": user_id,
            "doctor_id": doctor_id,
            "scheduled_time": "2024-12-25T10:00:00Z",
            "duration_minutes": 30,
            "status": "scheduled",
            "type": "consultation",
            "notes": null,
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
}