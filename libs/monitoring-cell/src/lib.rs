// =====================================================================================
// MONITORING CELL - COMPREHENSIVE HEALTH & PERFORMANCE MONITORING
// =====================================================================================
// 
// This cell provides comprehensive monitoring services including:
// - System health checks and component monitoring
// - Real-time performance metrics collection
// - Alert management and notification system
// - Business metrics tracking
// - Service dependency monitoring
//
// =====================================================================================

pub mod handlers;
pub mod models;
pub mod router;
pub mod services;

// Re-export commonly used types
pub use models::{
    HealthStatus, HealthCheck, SystemHealth,
    PerformanceMetrics, BusinessMetrics, MetricsSnapshot,
    Alert, AlertSeverity, MonitoringError,
};

pub use services::{
    HealthMonitorService, MetricsCollectorService, AlertManagerService,
};

pub use router::create_monitoring_router;
pub use handlers::MonitoringHandlers;