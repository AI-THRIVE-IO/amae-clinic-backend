// =====================================================================================
// SECURITY MONITORING SERVICE - THREAT DETECTION & IP BLOCKING
// =====================================================================================

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{warn, error, instrument};
use anyhow::Result;

use crate::models::{
    FailedLoginTracker, SuspiciousActivity, BlockedIp, 
    AuditEntry, AuditEventType, AuditOutcome
};
use crate::services::AuditService;

pub struct SecurityMonitoringService {
    failed_login_attempts: Arc<RwLock<HashMap<String, FailedLoginTracker>>>,
    suspicious_activities: Arc<RwLock<Vec<SuspiciousActivity>>>,
    blocked_ips: Arc<RwLock<HashMap<String, BlockedIp>>>,
    audit_service: Arc<AuditService>,
}

impl SecurityMonitoringService {
    pub fn new(audit_service: Arc<AuditService>) -> Self {
        Self {
            failed_login_attempts: Arc::new(RwLock::new(HashMap::new())),
            suspicious_activities: Arc::new(RwLock::new(Vec::new())),
            blocked_ips: Arc::new(RwLock::new(HashMap::new())),
            audit_service,
        }
    }

    #[instrument(skip(self))]
    pub async fn record_failed_login(&self, ip_address: &str, user_id: Option<&str>) -> Result<bool> {
        let mut tracker_map = self.failed_login_attempts.write().await;
        let now = chrono::Utc::now();
        
        let tracker = tracker_map.entry(ip_address.to_string()).or_insert_with(|| {
            FailedLoginTracker {
                attempts: 0,
                first_attempt: now,
                last_attempt: now,
                blocked_until: None,
            }
        });

        tracker.attempts += 1;
        tracker.last_attempt = now;

        // Progressive blocking: 1 min, 5 min, 15 min, 1 hour, 24 hours
        let block_duration = match tracker.attempts {
            3..=4 => Duration::from_secs(60),      // 1 minute
            5..=6 => Duration::from_secs(300),     // 5 minutes
            7..=9 => Duration::from_secs(900),     // 15 minutes
            10..=15 => Duration::from_secs(3600),  // 1 hour
            _ => Duration::from_secs(86400),       // 24 hours
        };

        if tracker.attempts >= 3 {
            tracker.blocked_until = Some(now + chrono::Duration::from_std(block_duration).unwrap());
            
            // Log security event
            let audit_entry = AuditEntry::new(
                AuditEventType::SuspiciousActivity,
                format!("Multiple failed login attempts from IP: {}", ip_address),
                AuditOutcome::Denied,
            )
            .with_risk_score(70)
            .add_context("ip_address", ip_address)
            .add_context("attempts", tracker.attempts)
            .add_context("user_id", user_id);

            self.audit_service.log_audit_entry(audit_entry).await?;

            // Record as blocked IP
            self.block_ip(
                ip_address,
                format!("Failed login attempts: {}", tracker.attempts),
                block_duration,
                tracker.attempts,
            ).await;

            Ok(true) // IP should be blocked
        } else {
            Ok(false)
        }
    }

    pub async fn is_ip_blocked(&self, ip_address: &str) -> bool {
        let tracker_map = self.failed_login_attempts.read().await;
        if let Some(tracker) = tracker_map.get(ip_address) {
            if let Some(blocked_until) = tracker.blocked_until {
                return chrono::Utc::now() < blocked_until;
            }
        }

        // Also check explicit IP blocks
        let blocked_ips = self.blocked_ips.read().await;
        if let Some(blocked_ip) = blocked_ips.get(ip_address) {
            let blocked_until = blocked_ip.blocked_at + chrono::Duration::from_std(blocked_ip.block_duration).unwrap();
            return chrono::Utc::now() < blocked_until;
        }

        false
    }

    pub async fn clear_failed_attempts(&self, ip_address: &str) {
        let mut tracker_map = self.failed_login_attempts.write().await;
        tracker_map.remove(ip_address);
    }

    #[instrument(skip(self))]
    pub async fn detect_suspicious_patterns(&self, user_id: &str, ip_address: &str) -> u8 {
        let activities = self.suspicious_activities.read().await;
        let now = chrono::Utc::now();
        let window = chrono::Duration::hours(1);

        // Check for rapid requests from same IP
        let recent_activities: Vec<_> = activities.iter()
            .filter(|a| a.ip_address == ip_address && (now - a.timestamp) < window)
            .collect();

        let mut risk_score = 0u8;

        // Too many activities from same IP
        if recent_activities.len() > 100 {
            risk_score += 40;
        }

        // Activities from multiple IPs for same user
        let user_activities: Vec<_> = activities.iter()
            .filter(|a| a.user_id.as_deref() == Some(user_id) && (now - a.timestamp) < window)
            .collect();

        let unique_ips: std::collections::HashSet<_> = user_activities.iter()
            .map(|a| &a.ip_address)
            .collect();

        if unique_ips.len() > 3 {
            risk_score += 30;
        }

        risk_score
    }

    pub async fn record_suspicious_activity(
        &self,
        ip_address: &str,
        user_id: Option<&str>,
        activity_type: &str,
        risk_score: u8,
        details: HashMap<String, String>,
    ) -> Result<()> {
        let activity = SuspiciousActivity {
            timestamp: chrono::Utc::now(),
            ip_address: ip_address.to_string(),
            user_id: user_id.map(|s| s.to_string()),
            activity_type: activity_type.to_string(),
            risk_score,
            details,
        };

        {
            let mut activities = self.suspicious_activities.write().await;
            activities.push(activity.clone());

            // Keep only recent activities (last 1000)
            if activities.len() > 1000 {
                activities.drain(0..500);
            }
        }

        // Log to audit if high risk
        if risk_score >= 60 {
            let audit_entry = AuditEntry::new(
                AuditEventType::SuspiciousActivity,
                format!("Suspicious activity detected: {}", activity_type),
                AuditOutcome::Denied,
            )
            .with_risk_score(risk_score)
            .add_context("ip_address", ip_address)
            .add_context("activity_type", activity_type);

            let audit_entry = if let Some(uid) = user_id {
                audit_entry.with_user(uid.to_string())
            } else {
                audit_entry
            };

            self.audit_service.log_audit_entry(audit_entry).await?;
        }

        Ok(())
    }

    async fn block_ip(
        &self,
        ip_address: &str,
        reason: String,
        duration: Duration,
        attempts_count: u32,
    ) {
        let blocked_ip = BlockedIp {
            blocked_at: chrono::Utc::now(),
            reason,
            block_duration: duration,
            attempts_count,
        };

        let mut blocked_ips = self.blocked_ips.write().await;
        blocked_ips.insert(ip_address.to_string(), blocked_ip);

        warn!("Blocked IP {} for {} seconds due to {} attempts", 
              ip_address, duration.as_secs(), attempts_count);
    }

    pub async fn manually_block_ip(
        &self,
        ip_address: &str,
        reason: &str,
        duration_hours: u32,
    ) -> Result<()> {
        let duration = Duration::from_secs(duration_hours as u64 * 3600);
        
        self.block_ip(
            ip_address,
            reason.to_string(),
            duration,
            0, // Manual block
        ).await;

        // Log the manual block
        let audit_entry = AuditEntry::new(
            AuditEventType::SuspiciousActivity,
            format!("IP manually blocked: {}", reason),
            AuditOutcome::Success,
        )
        .with_risk_score(80)
        .add_context("ip_address", ip_address)
        .add_context("duration_hours", duration_hours)
        .add_context("reason", reason);

        self.audit_service.log_audit_entry(audit_entry).await?;

        Ok(())
    }

    pub async fn unblock_ip(&self, ip_address: &str) -> Result<()> {
        {
            let mut blocked_ips = self.blocked_ips.write().await;
            blocked_ips.remove(ip_address);
        }

        {
            let mut failed_attempts = self.failed_login_attempts.write().await;
            failed_attempts.remove(ip_address);
        }

        // Log the unblock
        let audit_entry = AuditEntry::new(
            AuditEventType::SuspiciousActivity,
            "IP manually unblocked".to_string(),
            AuditOutcome::Success,
        )
        .add_context("ip_address", ip_address);

        self.audit_service.log_audit_entry(audit_entry).await?;

        Ok(())
    }

    pub async fn get_blocked_ips(&self) -> Vec<(String, BlockedIp)> {
        let blocked_ips = self.blocked_ips.read().await;
        blocked_ips.iter()
            .map(|(ip, block_info)| (ip.clone(), block_info.clone()))
            .collect()
    }

    pub async fn get_failed_login_stats(&self) -> HashMap<String, u32> {
        let failed_attempts = self.failed_login_attempts.read().await;
        failed_attempts.iter()
            .map(|(ip, tracker)| (ip.clone(), tracker.attempts))
            .collect()
    }

    pub async fn get_recent_suspicious_activities(&self, hours: u32) -> Vec<SuspiciousActivity> {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours as i64);
        let activities = self.suspicious_activities.read().await;
        
        activities.iter()
            .filter(|activity| activity.timestamp > cutoff)
            .cloned()
            .collect()
    }

    pub async fn cleanup_old_data(&self) -> Result<()> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(7);

        // Clean up old failed login attempts
        {
            let mut failed_attempts = self.failed_login_attempts.write().await;
            failed_attempts.retain(|_, tracker| tracker.last_attempt > cutoff);
        }

        // Clean up old blocked IPs
        {
            let mut blocked_ips = self.blocked_ips.write().await;
            let now = chrono::Utc::now();
            blocked_ips.retain(|_, blocked_ip| {
                let unblock_time = blocked_ip.blocked_at + 
                    chrono::Duration::from_std(blocked_ip.block_duration).unwrap();
                now < unblock_time
            });
        }

        // Clean up old suspicious activities
        {
            let mut activities = self.suspicious_activities.write().await;
            activities.retain(|activity| activity.timestamp > cutoff);
        }

        Ok(())
    }
}