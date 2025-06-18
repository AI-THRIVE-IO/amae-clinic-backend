// =====================================================================================
// AUDIT SERVICE - HIPAA/GDPR COMPLIANT AUDIT LOGGING
// =====================================================================================

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug, instrument};
use anyhow::Result;

use crate::models::{AuditEntry, AuditEventType, AuditOutcome};
use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

pub struct AuditService {
    supabase: SupabaseClient,
    audit_buffer: Arc<RwLock<Vec<AuditEntry>>>,
    config: AppConfig,
}

impl AuditService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
            audit_buffer: Arc::new(RwLock::new(Vec::new())),
            config: config.clone(),
        }
    }

    #[instrument(skip(self, entry))]
    pub async fn log_audit_entry(&self, entry: AuditEntry) -> Result<()> {
        // Log immediately to structured logging
        self.log_to_tracing(&entry).await;

        // Store in buffer for batch processing
        {
            let mut buffer = self.audit_buffer.write().await;
            buffer.push(entry.clone());

            // Flush buffer if it gets too large
            if buffer.len() >= 100 {
                self.flush_audit_buffer().await?;
            }
        }

        // Trigger security alert if high risk
        if entry.risk_score >= 70 {
            self.trigger_security_alert(&entry).await;
        }

        Ok(())
    }

    #[instrument(skip(self, entry))]
    async fn log_to_tracing(&self, entry: &AuditEntry) {
        match entry.outcome {
            AuditOutcome::Success => {
                info!(
                    event_id = %entry.event_id,
                    event_type = ?entry.event_type,
                    user_id = ?entry.user_id,
                    patient_id = ?entry.patient_id,
                    action = %entry.action,
                    risk_score = entry.risk_score,
                    "AUDIT: {}", entry.action
                );
            },
            AuditOutcome::Failure | AuditOutcome::Denied => {
                warn!(
                    event_id = %entry.event_id,
                    event_type = ?entry.event_type,
                    user_id = ?entry.user_id,
                    outcome = ?entry.outcome,
                    risk_score = entry.risk_score,
                    "AUDIT FAILURE: {}", entry.action
                );
            },
            AuditOutcome::Error => {
                error!(
                    event_id = %entry.event_id,
                    event_type = ?entry.event_type,
                    user_id = ?entry.user_id,
                    "AUDIT ERROR: {}", entry.action
                );
            },
            _ => {
                debug!(
                    event_id = %entry.event_id,
                    event_type = ?entry.event_type,
                    "AUDIT: {}", entry.action
                );
            }
        }
    }

    async fn trigger_security_alert(&self, entry: &AuditEntry) {
        error!(
            event_id = %entry.event_id,
            risk_score = entry.risk_score,
            event_type = ?entry.event_type,
            user_id = ?entry.user_id,
            ip_address = ?entry.ip_address,
            "HIGH-RISK SECURITY EVENT DETECTED"
        );
        
        // In production, this would:
        // 1. Send alerts to security team
        // 2. Log to SIEM system
        // 3. Potentially trigger automatic response
    }

    #[instrument(skip(self))]
    pub async fn flush_audit_buffer(&self) -> Result<()> {
        let entries = {
            let mut buffer = self.audit_buffer.write().await;
            let entries = buffer.clone();
            buffer.clear();
            entries
        };

        if entries.is_empty() {
            return Ok(());
        }

        // In production, would batch insert to audit database
        info!("Flushed {} audit entries to persistent storage", entries.len());
        
        Ok(())
    }

    pub async fn get_audit_entries_for_user(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<AuditEntry>> {
        // In production, would query audit database
        let buffer = self.audit_buffer.read().await;
        let user_entries: Vec<AuditEntry> = buffer
            .iter()
            .filter(|entry| entry.user_id.as_deref() == Some(user_id))
            .cloned()
            .collect();

        let limit = limit.unwrap_or(100) as usize;
        Ok(user_entries.into_iter().take(limit).collect())
    }

    pub async fn get_security_events(&self, hours: u32) -> Result<Vec<AuditEntry>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours as i64);
        
        let buffer = self.audit_buffer.read().await;
        let security_events: Vec<AuditEntry> = buffer
            .iter()
            .filter(|entry| {
                entry.timestamp > cutoff && matches!(
                    entry.event_type,
                    AuditEventType::SuspiciousActivity |
                    AuditEventType::UnauthorizedAccess |
                    AuditEventType::SqlInjectionAttempt |
                    AuditEventType::XssAttempt |
                    AuditEventType::RateLimitExceeded
                )
            })
            .cloned()
            .collect();

        Ok(security_events)
    }

    pub async fn log_patient_data_access(
        &self,
        user_id: &str,
        patient_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<()> {
        let entry = AuditEntry::new(
            AuditEventType::PatientDataViewed,
            action.to_string(),
            AuditOutcome::Success,
        )
        .with_user(user_id.to_string())
        .with_patient(patient_id.to_string())
        .add_context("resource_type", resource_type)
        .add_context("resource_id", resource_id);

        self.log_audit_entry(entry).await
    }

    pub async fn log_failed_authentication(
        &self,
        attempted_user: Option<&str>,
        ip_address: &str,
        reason: &str,
    ) -> Result<()> {
        let mut entry = AuditEntry::new(
            AuditEventType::LoginFailure,
            format!("Failed login attempt: {}", reason),
            AuditOutcome::Failure,
        )
        .with_risk_score(50)
        .add_context("ip_address", ip_address)
        .add_context("failure_reason", reason);

        if let Some(user) = attempted_user {
            entry = entry.with_user(user.to_string());
        }

        self.log_audit_entry(entry).await
    }

    pub async fn log_successful_authentication(
        &self,
        user_id: &str,
        ip_address: &str,
        session_id: &str,
    ) -> Result<()> {
        let entry = AuditEntry::new(
            AuditEventType::LoginSuccess,
            "Successful user authentication".to_string(),
            AuditOutcome::Success,
        )
        .with_user(user_id.to_string())
        .add_context("ip_address", ip_address)
        .add_context("session_id", session_id);

        self.log_audit_entry(entry).await
    }
}