use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    pub typ: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub exp: Option<u64>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub app_metadata: Option<serde_json::Value>,
    pub user_metadata: Option<serde_json::Value>,
    pub aud: Option<String>,
    pub iat: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: Option<String>,
    pub role: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub valid: bool,
    pub user_id: String,
    pub email: Option<String>,
    pub role: Option<String>,
}