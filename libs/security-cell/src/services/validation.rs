// =====================================================================================
// VALIDATION SERVICE - INPUT VALIDATION & SANITIZATION
// =====================================================================================

use regex::Regex;
use tracing::{debug, instrument};
use anyhow::Result;
use uuid::Uuid;

use crate::models::{ValidationConfig, ValidationResult, ValidationIssue};

pub struct ValidationService {
    config: ValidationConfig,
    sql_patterns: Vec<Regex>,
    xss_patterns: Vec<Regex>,
    blocked_patterns: Vec<Regex>,
}

impl ValidationService {
    pub fn new(config: ValidationConfig) -> Self {
        let sql_patterns = config.sql_injection_patterns.iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        
        let xss_patterns = config.xss_patterns.iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        
        let blocked_patterns = config.blocked_patterns.iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        Self {
            config,
            sql_patterns,
            xss_patterns,
            blocked_patterns,
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(ValidationConfig::default())
    }

    #[instrument(skip(self, input))]
    pub fn validate_input(&self, input: &str, field_name: &str) -> ValidationResult {
        let mut issues = Vec::new();
        let mut risk_score = 0u8;

        // Length validation
        if input.len() > self.config.max_string_length {
            issues.push(ValidationIssue::ExceedsMaxLength {
                field: field_name.to_string(),
                max_length: self.config.max_string_length,
                actual_length: input.len(),
            });
            risk_score += 10;
        }

        // SQL injection detection
        for pattern in &self.sql_patterns {
            if pattern.is_match(input) {
                issues.push(ValidationIssue::SqlInjectionAttempt {
                    field: field_name.to_string(),
                    pattern: pattern.as_str().to_string(),
                });
                risk_score += 50;
            }
        }

        // XSS detection
        for pattern in &self.xss_patterns {
            if pattern.is_match(input) {
                issues.push(ValidationIssue::XssAttempt {
                    field: field_name.to_string(),
                    pattern: pattern.as_str().to_string(),
                });
                risk_score += 40;
            }
        }

        // Blocked patterns
        for pattern in &self.blocked_patterns {
            if pattern.is_match(input) {
                issues.push(ValidationIssue::BlockedPattern {
                    field: field_name.to_string(),
                    pattern: pattern.as_str().to_string(),
                });
                risk_score += 30;
            }
        }

        ValidationResult {
            is_valid: issues.is_empty(),
            issues,
            risk_score,
            sanitized_input: self.sanitize_input(input),
        }
    }

    pub fn sanitize_input(&self, input: &str) -> String {
        // Basic HTML entity encoding
        input
            .replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;")
            .replace("\"", "&quot;")
            .replace("'", "&#x27;")
            .replace("/", "&#x2F;")
    }

    pub fn validate_email(&self, email: &str) -> bool {
        let email_regex = Regex::new(
            r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
        ).unwrap();
        
        email_regex.is_match(email) && email.len() <= 254
    }

    pub fn validate_phone(&self, phone: &str) -> bool {
        let phone_regex = Regex::new(
            r"^\+?[1-9]\d{1,14}$|^\+?\d{1,4}[\s\-\.\(\)]*\d{1,14}$"
        ).unwrap();
        
        phone_regex.is_match(phone)
    }

    pub fn validate_uuid(&self, uuid_str: &str) -> bool {
        Uuid::parse_str(uuid_str).is_ok()
    }

    pub fn validate_medical_id(&self, medical_id: &str) -> ValidationResult {
        let mut issues = Vec::new();
        let mut risk_score = 0u8;

        // Medical ID should be alphanumeric with possible hyphens
        let medical_id_regex = Regex::new(r"^[A-Za-z0-9\-]{6,20}$").unwrap();
        
        if !medical_id_regex.is_match(medical_id) {
            issues.push(ValidationIssue::InvalidFormat {
                field: "medical_id".to_string(),
                expected: "6-20 alphanumeric characters with optional hyphens".to_string(),
            });
            risk_score += 20;
        }

        ValidationResult {
            is_valid: issues.is_empty(),
            issues,
            risk_score,
            sanitized_input: medical_id.to_string(),
        }
    }

    pub fn validate_patient_data(&self, data: &serde_json::Value) -> ValidationResult {
        let mut issues = Vec::new();
        let mut risk_score = 0u8;

        // Check for sensitive data patterns
        let data_str = data.to_string();
        
        // Check for SSN patterns
        let ssn_regex = Regex::new(r"\b\d{3}-?\d{2}-?\d{4}\b").unwrap();
        if ssn_regex.is_match(&data_str) {
            debug!("Detected potential SSN in patient data");
            risk_score += 30;
        }

        // Check for credit card patterns
        let cc_regex = Regex::new(r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b").unwrap();
        if cc_regex.is_match(&data_str) {
            debug!("Detected potential credit card number in patient data");
            risk_score += 40;
        }

        ValidationResult {
            is_valid: issues.is_empty(),
            issues,
            risk_score,
            sanitized_input: self.sanitize_patient_data(data),
        }
    }

    fn sanitize_patient_data(&self, data: &serde_json::Value) -> String {
        // In production, would implement comprehensive PHI sanitization
        // For now, basic sanitization
        let data_str = data.to_string();
        self.sanitize_input(&data_str)
    }

    pub fn validate_prescription_data(&self, prescription: &str) -> ValidationResult {
        let mut issues = Vec::new();
        let mut risk_score = 0u8;

        // Check for controlled substance patterns
        let controlled_substances = [
            "oxycodone", "morphine", "fentanyl", "adderall", "xanax", "valium"
        ];

        let prescription_lower = prescription.to_lowercase();
        for substance in &controlled_substances {
            if prescription_lower.contains(substance) {
                debug!("Detected controlled substance in prescription: {}", substance);
                risk_score += 10; // Not an issue, but worth monitoring
            }
        }

        // Validate prescription format
        if prescription.len() > 500 {
            issues.push(ValidationIssue::ExceedsMaxLength {
                field: "prescription".to_string(),
                max_length: 500,
                actual_length: prescription.len(),
            });
            risk_score += 15;
        }

        ValidationResult {
            is_valid: issues.is_empty(),
            issues,
            risk_score,
            sanitized_input: self.sanitize_input(prescription),
        }
    }
}