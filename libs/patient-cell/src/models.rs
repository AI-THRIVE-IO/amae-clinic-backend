use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc, NaiveDate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patient {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub phone_number: String,
    pub address: String,
    pub eircode: Option<String>,
    pub date_of_birth: NaiveDate,
    pub birth_gender: String,
    pub ppsn: Option<String>,
    pub allergies: Option<String>,
    pub chronic_conditions: Option<Vec<String>>,
    pub current_medications: Option<String>,
    pub smoking_status: Option<String>,
    pub alcohol_use: Option<String>,
    pub surgery_history: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Patient {
    pub fn full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }
    
    pub fn age(&self) -> i32 {
        let today = chrono::Utc::now().date_naive();
        let years = today.years_since(self.date_of_birth).unwrap_or(0) as i32;
        years
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePatientRequest {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub phone_number: String,
    pub address: String,
    pub eircode: Option<String>,
    pub date_of_birth: NaiveDate,
    pub birth_gender: String,
    pub ppsn: Option<String>,
    pub allergies: Option<String>,
    pub chronic_conditions: Option<Vec<String>>,
    pub current_medications: Option<String>,
    pub smoking_status: Option<String>,
    pub alcohol_use: Option<String>,
    pub surgery_history: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePatientRequest {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone_number: Option<String>,
    pub address: Option<String>,
    pub eircode: Option<String>,
    pub allergies: Option<String>,
    pub chronic_conditions: Option<Vec<String>>,
    pub current_medications: Option<String>,
    pub smoking_status: Option<String>,
    pub alcohol_use: Option<String>,
    pub surgery_history: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientSearchQuery {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub ppsn: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum PatientError {
    #[error("Patient not found")]
    NotFound,
    
    #[error("Patient with email {email} already exists")]
    EmailAlreadyExists { email: String },
    
    #[error("Invalid date of birth")]
    InvalidDateOfBirth,
    
    #[error("Unauthorized access to patient data")]
    Unauthorized,
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
}