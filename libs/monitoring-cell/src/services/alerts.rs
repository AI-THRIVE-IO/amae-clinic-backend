// =====================================================================================
// ALERT MANAGER SERVICE
// =====================================================================================

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{warn, error, info, instrument};
use uuid::Uuid;

use crate::models::{
    Alert, AlertSeverity, AlertRule, AlertComparison, MetricsSnapshot,
};

pub struct AlertManagerService {
    alert_rules: Vec<AlertRule>,
    active_alerts: Arc<RwLock<HashMap<String, Alert>>>,
}

impl AlertManagerService {
    pub fn new() -> Self {
        let alert_rules = vec![
            AlertRule {
                name: "High Error Rate".to_string(),
                metric_name: "error_rate_percentage".to_string(),
                threshold: 5.0,
                comparison: AlertComparison::GreaterThan,
                severity: AlertSeverity::Warning,
                duration_minutes: 5,
            },
            AlertRule {
                name: "Critical Error Rate".to_string(),
                metric_name: "error_rate_percentage".to_string(),
                threshold: 10.0,
                comparison: AlertComparison::GreaterThan,
                severity: AlertSeverity::Critical,
                duration_minutes: 2,
            },
            AlertRule {
                name: "High Response Time".to_string(),
                metric_name: "p95_response_time_ms".to_string(),
                threshold: 2000.0,
                comparison: AlertComparison::GreaterThan,
                severity: AlertSeverity::Warning,
                duration_minutes: 10,
            },
            AlertRule {
                name: "Low Requests Per Second".to_string(),
                metric_name: "requests_per_second".to_string(),
                threshold: 0.1,
                comparison: AlertComparison::LessThan,
                severity: AlertSeverity::Warning,
                duration_minutes: 15,
            },
        ];

        Self {
            alert_rules,
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[instrument(skip(self, metrics))]
    pub async fn evaluate_alerts(&self, metrics: &MetricsSnapshot) {
        for rule in &self.alert_rules {
            let metric_value = match rule.metric_name.as_str() {
                "error_rate_percentage" => metrics.error_rate_percentage,
                "p95_response_time_ms" => metrics.p95_response_time_ms,
                "average_response_time_ms" => metrics.average_response_time_ms,
                "requests_per_second" => metrics.requests_per_second,
                _ => continue,
            };

            let should_alert = match rule.comparison {
                AlertComparison::GreaterThan => metric_value > rule.threshold,
                AlertComparison::LessThan => metric_value < rule.threshold,
                AlertComparison::Equals => (metric_value - rule.threshold).abs() < 0.001,
            };

            if should_alert {
                self.trigger_alert(rule, metric_value).await;
            }
        }
    }

    async fn trigger_alert(&self, rule: &AlertRule, metric_value: f64) {
        let alert = Alert {
            alert_id: Uuid::new_v4().to_string(),
            severity: rule.severity.clone(),
            title: rule.name.clone(),
            description: format!(
                "Metric {} has value {:.2} which {} threshold {:.2}",
                rule.metric_name, 
                metric_value, 
                match rule.comparison {
                    AlertComparison::GreaterThan => "exceeds",
                    AlertComparison::LessThan => "is below",
                    AlertComparison::Equals => "equals",
                },
                rule.threshold
            ),
            component: "api".to_string(),
            metric_value,
            threshold: rule.threshold,
            timestamp: chrono::Utc::now(),
            tags: HashMap::new(),
        };

        match alert.severity {
            AlertSeverity::Critical | AlertSeverity::Emergency => {
                error!(
                    alert_id = %alert.alert_id,
                    severity = ?alert.severity,
                    metric = %rule.metric_name,
                    value = %metric_value,
                    threshold = %rule.threshold,
                    "CRITICAL ALERT TRIGGERED: {}", alert.title
                );
            },
            AlertSeverity::Warning => {
                warn!(
                    alert_id = %alert.alert_id,
                    metric = %rule.metric_name,
                    value = %metric_value,
                    "WARNING ALERT: {}", alert.title
                );
            },
            AlertSeverity::Info => {
                info!(
                    alert_id = %alert.alert_id,
                    "INFO ALERT: {}", alert.title
                );
            },
        }

        let mut active_alerts = self.active_alerts.write().await;
        active_alerts.insert(alert.alert_id.clone(), alert);
    }

    pub async fn get_active_alerts(&self) -> Vec<Alert> {
        let alerts = self.active_alerts.read().await;
        alerts.values().cloned().collect()
    }

    pub async fn acknowledge_alert(&self, alert_id: &str) -> bool {
        let mut alerts = self.active_alerts.write().await;
        alerts.remove(alert_id).is_some()
    }

    pub async fn clear_all_alerts(&self) {
        let mut alerts = self.active_alerts.write().await;
        alerts.clear();
    }

    pub async fn get_alert_summary(&self) -> HashMap<String, u32> {
        let alerts = self.active_alerts.read().await;
        let mut summary = HashMap::new();
        
        for alert in alerts.values() {
            let severity_key = format!("{:?}", alert.severity);
            *summary.entry(severity_key).or_insert(0) += 1;
        }
        
        summary
    }

    pub fn add_custom_rule(&mut self, rule: AlertRule) {
        self.alert_rules.push(rule);
    }

    pub fn get_all_rules(&self) -> &Vec<AlertRule> {
        &self.alert_rules
    }
}