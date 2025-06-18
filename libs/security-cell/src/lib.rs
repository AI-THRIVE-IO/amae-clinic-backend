// =====================================================================================
// SECURITY CELL - ENTERPRISE SECURITY HARDENING & AUDIT
// =====================================================================================
// 
// This cell provides comprehensive security services including:
// - HIPAA/GDPR compliant audit logging
// - Advanced input validation and sanitization
// - Security monitoring and threat detection
// - Password security and strength validation
// - IP blocking and rate limiting
// - Security middleware for request protection
//
// =====================================================================================

pub mod handlers;
pub mod models;
pub mod router;
pub mod services;

// Re-export commonly used types
pub use models::{
    AuditEntry, AuditEventType, AuditOutcome,
    ValidationResult, ValidationIssue,
    SecurityError, PasswordStrength, PasswordStrengthResult,
};

pub use services::{
    AuditService, ValidationService, 
    SecurityMonitoringService, PasswordSecurityService,
};

pub use router::create_security_router;

// Re-export handlers for direct use if needed
pub use handlers::SecurityHandlers;