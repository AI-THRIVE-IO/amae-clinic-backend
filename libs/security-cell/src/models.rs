// =====================================================================================
// SECURITY CELL MODELS
// =====================================================================================

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =====================================================================================
// AUDIT MODELS
// =====================================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    // Authentication & Authorization
    LoginSuccess,
    LoginFailure,
    LogoutEvent,
    TokenExpired,
    UnauthorizedAccess,
    
    // Data Access Events (HIPAA Requirement)
    PatientDataViewed,
    PatientDataModified,
    PatientDataCreated,
    PatientDataDeleted,
    MedicalRecordAccessed,
    
    // Administrative Events
    UserCreated,
    UserModified,
    UserDeactivated,
    PermissionChanged,
    SystemConfigChanged,
    
    // Security Events
    SuspiciousActivity,
    RateLimitExceeded,
    InvalidDataSubmission,
    SqlInjectionAttempt,
    XssAttempt,
    
    // Clinical Events
    AppointmentBooked,
    AppointmentModified,
    AppointmentCancelled,
    PrescriptionIssued,
    DiagnosisEntered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOutcome {
    Success,
    Failure,
    Partial,
    Denied,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub event_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: AuditEventType,
    pub user_id: Option<String>,
    pub patient_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub action: String,
    pub outcome: AuditOutcome,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<String>,
    pub additional_data: HashMap<String, serde_json::Value>,
    pub risk_score: u8, // 0-100, higher = more suspicious
}

// =====================================================================================
// VALIDATION MODELS
// =====================================================================================

#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub max_string_length: usize,
    pub allowed_html_tags: Vec<String>,
    pub blocked_patterns: Vec<String>,
    pub sql_injection_patterns: Vec<String>,
    pub xss_patterns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub issues: Vec<ValidationIssue>,
    pub risk_score: u8,
    pub sanitized_input: String,
}

#[derive(Debug, Clone)]
pub enum ValidationIssue {
    ExceedsMaxLength { field: String, max_length: usize, actual_length: usize },
    SqlInjectionAttempt { field: String, pattern: String },
    XssAttempt { field: String, pattern: String },
    BlockedPattern { field: String, pattern: String },
    InvalidFormat { field: String, expected: String },
}

// =====================================================================================
// JWT SECURITY MODELS
// =====================================================================================

#[derive(Debug, Clone)]
pub struct JwtSecurityConfig {
    pub algorithm: String,
    pub issuer: String,
    pub audience: String,
    pub token_lifetime: Duration,
    pub refresh_token_lifetime: Duration,
    pub key_rotation_interval: Duration,
    pub max_concurrent_sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureJwtClaims {
    pub sub: String,           // Subject (user ID)
    pub iss: String,           // Issuer
    pub aud: String,           // Audience
    pub exp: u64,              // Expiration time
    pub iat: u64,              // Issued at
    pub nbf: u64,              // Not before
    pub jti: String,           // JWT ID (unique)
    pub role: String,          // User role
    pub session_id: String,    // Session identifier
    pub permissions: Vec<String>, // Detailed permissions
    pub ip_hash: String,       // Hashed IP for binding
    pub device_fingerprint: Option<String>, // Device identification
}

// =====================================================================================
// PASSWORD SECURITY MODELS
// =====================================================================================

#[derive(Debug, Clone)]
pub struct PasswordStrengthResult {
    pub strength: PasswordStrength,
    pub score: u8,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub enum PasswordStrength {
    Weak,
    Fair,
    Good,
    Strong,
}

// =====================================================================================
// SECURITY MONITORING MODELS
// =====================================================================================

#[derive(Debug, Clone)]
pub struct FailedLoginTracker {
    pub attempts: u32,
    pub first_attempt: chrono::DateTime<chrono::Utc>,
    pub last_attempt: chrono::DateTime<chrono::Utc>,
    pub blocked_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuspiciousActivity {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub ip_address: String,
    pub user_id: Option<String>,
    pub activity_type: String,
    pub risk_score: u8,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct BlockedIp {
    pub blocked_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
    pub block_duration: Duration,
    pub attempts_count: u32,
}

// =====================================================================================
// REQUEST/RESPONSE MODELS
// =====================================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityHealthRequest {
    pub include_details: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SecurityHealthResponse {
    pub status: String,
    pub blocked_ips: u32,
    pub failed_login_attempts: u32,
    pub suspicious_activities: u32,
    pub audit_entries_today: u32,
    pub details: Option<SecurityHealthDetails>,
}

#[derive(Debug, Serialize)]
pub struct SecurityHealthDetails {
    pub recent_blocked_ips: Vec<String>,
    pub recent_suspicious_activities: Vec<SuspiciousActivity>,
    pub security_metrics: SecurityMetrics,
}

#[derive(Debug, Serialize)]
pub struct SecurityMetrics {
    pub login_success_rate: f64,
    pub average_risk_score: f64,
    pub top_threat_types: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateInputRequest {
    pub field_name: String,
    pub value: String,
    pub validation_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ValidateInputResponse {
    pub is_valid: bool,
    pub sanitized_value: String,
    pub risk_score: u8,
    pub issues: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PasswordValidationRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct PasswordValidationResponse {
    pub strength: PasswordStrength,
    pub score: u8,
    pub requirements_met: bool,
    pub suggestions: Vec<String>,
}

// =====================================================================================
// ERROR MODELS
// =====================================================================================

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Validation failed: {0:?}")]
    ValidationFailed(Vec<ValidationIssue>),
    #[error("IP blocked due to suspicious activity")]
    IpBlocked,
    #[error("Authentication required")]
    AuthenticationRequired,
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    #[error("Security service error: {0}")]
    ServiceError(String),
}

// =====================================================================================
// IMPLEMENTATIONS
// =====================================================================================

impl AuditEntry {
    pub fn new(
        event_type: AuditEventType,
        action: String,
        outcome: AuditOutcome,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            event_type,
            user_id: None,
            patient_id: None,
            resource_type: None,
            resource_id: None,
            action,
            outcome,
            ip_address: None,
            user_agent: None,
            session_id: None,
            additional_data: HashMap::new(),
            risk_score: 0,
        }
    }

    pub fn with_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_patient(mut self, patient_id: String) -> Self {
        self.patient_id = Some(patient_id);
        self
    }

    pub fn with_risk_score(mut self, score: u8) -> Self {
        self.risk_score = score.min(100);
        self
    }

    pub fn add_context<T: Serialize>(mut self, key: &str, value: T) -> Self {
        if let Ok(serialized) = serde_json::to_value(value) {
            self.additional_data.insert(key.to_string(), serialized);
        }
        self
    }
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_string_length: 10_000,
            allowed_html_tags: vec![],  // No HTML allowed by default
            blocked_patterns: vec![
                r"<script".to_string(),
                r"javascript:".to_string(),
                r"vbscript:".to_string(),
                r"data:text/html".to_string(),
            ],
            sql_injection_patterns: vec![
                r"(?i)(union|select|insert|update|delete|drop|create|alter|exec|execute)".to_string(),
                r"(?i)(\bor\b|\band\b)\s*[=<>]".to_string(),
                r"[';]|--|/\*|\*/".to_string(),
            ],
            xss_patterns: vec![
                r"(?i)<script[^>]*>.*?</script>".to_string(),
                r"(?i)on\w+\s*=".to_string(),
                r"(?i)javascript:".to_string(),
            ],
        }
    }
}

impl Default for JwtSecurityConfig {
    fn default() -> Self {
        Self {
            algorithm: "HS256".to_string(),
            issuer: "amae-clinic".to_string(),
            audience: "amae-clinic-api".to_string(),
            token_lifetime: Duration::from_secs(3600),      // 1 hour
            refresh_token_lifetime: Duration::from_secs(86400 * 7), // 7 days
            key_rotation_interval: Duration::from_secs(86400),      // 24 hours
            max_concurrent_sessions: 5,
        }
    }
}

impl SecureJwtClaims {
    pub fn new(
        user_id: String,
        role: String,
        permissions: Vec<String>,
        ip_address: &str,
        config: &JwtSecurityConfig,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            sub: user_id,
            iss: config.issuer.clone(),
            aud: config.audience.clone(),
            exp: now + config.token_lifetime.as_secs(),
            iat: now,
            nbf: now,
            jti: Uuid::new_v4().to_string(),
            role,
            session_id: Uuid::new_v4().to_string(),
            permissions,
            ip_hash: Self::hash_ip(ip_address),
            device_fingerprint: None,
        }
    }

    fn hash_ip(ip: &str) -> String {
        use ring::digest;
        use base64::{Engine as _, engine::general_purpose};
        
        let digest = digest::digest(&digest::SHA256, ip.as_bytes());
        general_purpose::STANDARD.encode(digest.as_ref())
    }

    pub fn validate_ip_binding(&self, current_ip: &str) -> bool {
        self.ip_hash == Self::hash_ip(current_ip)
    }

    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        now >= self.exp
    }

    pub fn has_permission(&self, required_permission: &str) -> bool {
        self.permissions.contains(&required_permission.to_string()) ||
        self.permissions.contains(&"admin".to_string())
    }
}