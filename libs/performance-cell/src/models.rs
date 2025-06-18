use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheStats {
    pub hit_rate: f64,
    pub total_entries: u64,
    pub memory_usage_mb: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceStats {
    pub cache_stats: CacheStats,
    pub request_count: u64,
    pub average_response_time_ms: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum PerformanceError {
    #[error("Cache error: {0}")]
    CacheError(String),
    #[error("Performance metrics unavailable")]
    MetricsUnavailable,
}