// libs/appointment-cell/src/services/lifecycle.rs
use chrono::{DateTime, Utc, Duration};
use chrono::Timelike;
use chrono::Datelike;
use tracing::{debug, info, warn};

use crate::models::{AppointmentStatus, AppointmentError};

pub struct AppointmentLifecycleService;

impl AppointmentLifecycleService {
    pub fn new() -> Self {
        Self
    }

    /// Validate that a status transition is allowed
    pub fn validate_status_transition(
        &self,
        current_status: &AppointmentStatus,
        new_status: &AppointmentStatus,
    ) -> Result<(), AppointmentError> {
        debug!("Validating status transition from {:?} to {:?}", current_status, new_status);

        let valid_transitions = self.get_valid_transitions(current_status);
        
        if !valid_transitions.contains(new_status) {
            warn!("Invalid status transition attempted: {:?} -> {:?}", current_status, new_status);
            return Err(AppointmentError::InvalidStatusTransition(current_status.clone()));
        }

        info!("Status transition validated: {:?} -> {:?}", current_status, new_status);
        Ok(())
    }

    /// Get all valid next statuses for a given current status
    pub fn get_valid_transitions(&self, current_status: &AppointmentStatus) -> Vec<AppointmentStatus> {
        match current_status {
            AppointmentStatus::Pending => vec![
                AppointmentStatus::Confirmed,
                AppointmentStatus::Cancelled,
                AppointmentStatus::NoShow,
            ],
            AppointmentStatus::Confirmed => vec![
                AppointmentStatus::InProgress,
                AppointmentStatus::Cancelled,
                AppointmentStatus::NoShow,
                AppointmentStatus::Rescheduled,
            ],
            AppointmentStatus::InProgress => vec![
                AppointmentStatus::Completed,
                AppointmentStatus::Cancelled, // Emergency cancellation
            ],
            AppointmentStatus::Rescheduled => vec![
                AppointmentStatus::Confirmed,
                AppointmentStatus::Cancelled,
            ],
            // Terminal states - no transitions allowed
            AppointmentStatus::Completed => vec![],
            AppointmentStatus::Cancelled => vec![],
            AppointmentStatus::NoShow => vec![],
        }
    }

    /// Check if an appointment can be started based on time and status
    pub fn can_start_appointment(
        &self,
        current_status: &AppointmentStatus,
        scheduled_start_time: DateTime<Utc>,
        current_time: DateTime<Utc>,
    ) -> Result<bool, AppointmentError> {
        debug!("Checking if appointment can be started");

        // Must be confirmed to start
        if *current_status != AppointmentStatus::Confirmed {
            return Ok(false);
        }

        // Allow starting up to 15 minutes before scheduled time
        let earliest_start = scheduled_start_time - Duration::minutes(15);
        
        // Don't allow starting more than 30 minutes after scheduled time
        let latest_start = scheduled_start_time + Duration::minutes(30);

        Ok(current_time >= earliest_start && current_time <= latest_start)
    }

    /// Check if an appointment should be marked as no-show
    pub fn should_mark_no_show(
        &self,
        current_status: &AppointmentStatus,
        scheduled_start_time: DateTime<Utc>,
        current_time: DateTime<Utc>,
    ) -> bool {
        // Only mark as no-show if currently confirmed or pending
        if !matches!(current_status, AppointmentStatus::Confirmed | AppointmentStatus::Pending) {
            return false;
        }

        // Mark as no-show if 30 minutes past scheduled start time
        let no_show_threshold = scheduled_start_time + Duration::minutes(30);
        current_time > no_show_threshold
    }

    /// Get recommended actions for an appointment based on its current state
    pub fn get_recommended_actions(
        &self,
        current_status: &AppointmentStatus,
        scheduled_start_time: DateTime<Utc>,
        current_time: DateTime<Utc>,
    ) -> Vec<String> {
        let mut actions = Vec::new();

        match current_status {
            AppointmentStatus::Pending => {
                actions.push("Waiting for confirmation".to_string());
                if current_time > scheduled_start_time - Duration::hours(24) {
                    actions.push("Send reminder notification".to_string());
                }
            },
            AppointmentStatus::Confirmed => {
                if self.can_start_appointment(current_status, scheduled_start_time, current_time).unwrap_or(false) {
                    actions.push("Ready to start consultation".to_string());
                } else if current_time < scheduled_start_time - Duration::hours(1) {
                    actions.push("Send reminder notification".to_string());
                } else if self.should_mark_no_show(current_status, scheduled_start_time, current_time) {
                    actions.push("Mark as no-show".to_string());
                }
            },
            AppointmentStatus::InProgress => {
                let duration_so_far = current_time - scheduled_start_time;
                if duration_so_far > Duration::hours(2) {
                    actions.push("Long consultation - consider completion".to_string());
                }
            },
            AppointmentStatus::Completed => {
                actions.push("Generate consultation report".to_string());
                actions.push("Send follow-up instructions".to_string());
                actions.push("Update patient history for future matching".to_string()); // NEW
            },
            AppointmentStatus::Cancelled => {
                actions.push("Process refund if applicable".to_string());
            },
            AppointmentStatus::NoShow => {
                actions.push("Apply no-show fee if applicable".to_string());
                actions.push("Send rescheduling options".to_string());
            },
            AppointmentStatus::Rescheduled => {
                actions.push("Create new appointment slot".to_string());
            },
        }

        actions
    }

    /// Automatically transition appointments based on time and business rules
    pub fn get_automatic_transitions(
        &self,
        current_status: &AppointmentStatus,
        scheduled_start_time: DateTime<Utc>,
        scheduled_end_time: DateTime<Utc>,
        current_time: DateTime<Utc>,
    ) -> Option<AppointmentStatus> {
        match current_status {
            AppointmentStatus::Confirmed => {
                // Auto mark as no-show if 30 minutes past start time
                if self.should_mark_no_show(current_status, scheduled_start_time, current_time) {
                    return Some(AppointmentStatus::NoShow);
                }
            },
            AppointmentStatus::InProgress => {
                // Auto complete if 30 minutes past scheduled end time
                let auto_complete_threshold = scheduled_end_time + Duration::minutes(30);
                if current_time > auto_complete_threshold {
                    return Some(AppointmentStatus::Completed);
                }
            },
            _ => {
                // No automatic transitions for other statuses
            }
        }

        None
    }

    /// Get the business rules for appointment lifecycle
    pub fn get_lifecycle_rules(&self) -> AppointmentLifecycleRules {
        AppointmentLifecycleRules::default()
    }

    /// Validate appointment timing constraints
    pub fn validate_appointment_timing(
        &self,
        scheduled_start_time: DateTime<Utc>,
        duration_minutes: i32,
        current_time: DateTime<Utc>,
    ) -> Result<(), AppointmentError> {
        let scheduled_end_time = scheduled_start_time + Duration::minutes(duration_minutes as i64);

        // Appointment must be in the future
        if scheduled_start_time <= current_time {
            return Err(AppointmentError::InvalidTime(
                "Appointment must be scheduled for a future time".to_string()
            ));
        }

        // Validate business hours (8 AM - 8 PM)
        let start_hour = scheduled_start_time.hour();
        let end_hour = scheduled_end_time.hour();
        
        if start_hour < 8 || start_hour >= 20 || end_hour > 20 {
            return Err(AppointmentError::InvalidTime(
                "Appointments must be scheduled between 8 AM and 8 PM".to_string()
            ));
        }

        // Validate weekend scheduling (optional - depends on business rules)
        let weekday = scheduled_start_time.weekday();
        if weekday == chrono::Weekday::Sun {
            return Err(AppointmentError::InvalidTime(
                "Appointments cannot be scheduled on Sundays".to_string()
            ));
        }

        Ok(())
    }

    /// Calculate appointment metrics for analytics including continuity metrics
    pub fn calculate_appointment_metrics(
        &self,
        scheduled_start_time: DateTime<Utc>,
        scheduled_end_time: DateTime<Utc>,
        actual_start_time: Option<DateTime<Utc>>,
        actual_end_time: Option<DateTime<Utc>>,
        has_patient_history: bool, // NEW: Track if patient has seen this doctor before
    ) -> AppointmentMetrics {
        let scheduled_duration = (scheduled_end_time - scheduled_start_time).num_minutes();
        
        let (actual_duration, start_delay, end_variance) = if let (Some(actual_start), Some(actual_end)) = (actual_start_time, actual_end_time) {
            let actual_duration = (actual_end - actual_start).num_minutes();
            let start_delay = (actual_start - scheduled_start_time).num_minutes();
            let end_variance = (actual_end - scheduled_end_time).num_minutes();
            
            (Some(actual_duration), Some(start_delay), Some(end_variance))
        } else {
            (None, None, None)
        };

        AppointmentMetrics {
            scheduled_duration_minutes: scheduled_duration,
            actual_duration_minutes: actual_duration,
            start_delay_minutes: start_delay,
            end_variance_minutes: end_variance,
            has_patient_history, // NEW
        }
    }
}

/// Business rules for appointment lifecycle management
#[derive(Debug, Clone)]
pub struct AppointmentLifecycleRules {
    pub max_early_start_minutes: i32,
    pub max_late_start_minutes: i32,
    pub no_show_threshold_minutes: i32,
    pub auto_complete_delay_minutes: i32,
    pub min_cancellation_notice_hours: i32,
    pub max_reschedule_count: i32,
}

impl Default for AppointmentLifecycleRules {
    fn default() -> Self {
        Self {
            max_early_start_minutes: 15,   // Can start up to 15 minutes early
            max_late_start_minutes: 30,    // Can start up to 30 minutes late
            no_show_threshold_minutes: 30, // Mark as no-show after 30 minutes
            auto_complete_delay_minutes: 30, // Auto-complete 30 minutes after scheduled end
            min_cancellation_notice_hours: 24, // Must cancel at least 24 hours ahead
            max_reschedule_count: 3,       // Maximum 3 reschedules per appointment
        }
    }
}

/// Enhanced metrics for appointment performance analysis
#[derive(Debug, Clone)]
pub struct AppointmentMetrics {
    pub scheduled_duration_minutes: i64,
    pub actual_duration_minutes: Option<i64>,
    pub start_delay_minutes: Option<i64>,
    pub end_variance_minutes: Option<i64>,
    pub has_patient_history: bool, // NEW: Track continuity of care
}

impl AppointmentMetrics {
    pub fn was_on_time(&self) -> Option<bool> {
        self.start_delay_minutes.map(|delay| delay.abs() <= 5) // Within 5 minutes
    }

    pub fn duration_variance_percentage(&self) -> Option<f64> {
        if let Some(actual_duration) = self.actual_duration_minutes {
            let variance = actual_duration - self.scheduled_duration_minutes;
            Some((variance as f64 / self.scheduled_duration_minutes as f64) * 100.0)
        } else {
            None
        }
    }

    /// NEW: Check if this appointment contributes to doctor continuity
    pub fn contributes_to_continuity(&self) -> bool {
        self.has_patient_history
    }
}