// =====================================================================================
// METRICS COLLECTOR SERVICE
// =====================================================================================

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, instrument};

use crate::models::MetricsSnapshot;

#[derive(Debug)]
pub struct MetricsCollectorService {
    request_count: AtomicU64,
    error_count: AtomicU64,
    total_response_time_ms: AtomicU64,
    response_times: Arc<RwLock<Vec<u64>>>,
    start_time: Instant,
}

impl MetricsCollectorService {
    pub fn new() -> Self {
        Self {
            request_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            total_response_time_ms: AtomicU64::new(0),
            response_times: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
        }
    }

    #[instrument(skip(self))]
    pub async fn record_request(&self, response_time_ms: u64, is_error: bool) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.total_response_time_ms.fetch_add(response_time_ms, Ordering::Relaxed);
        
        if is_error {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }

        // Store response time for percentile calculations
        let mut times = self.response_times.write().await;
        times.push(response_time_ms);
        
        // Keep only recent response times (last 1000 requests)
        if times.len() > 1000 {
            times.drain(0..500); // Remove oldest half
        }
    }

    pub async fn get_current_stats(&self) -> MetricsSnapshot {
        let total_requests = self.request_count.load(Ordering::Relaxed);
        let total_errors = self.error_count.load(Ordering::Relaxed);
        let total_response_time = self.total_response_time_ms.load(Ordering::Relaxed);
        let uptime = self.start_time.elapsed().as_secs();

        let requests_per_second = if uptime > 0 {
            total_requests as f64 / uptime as f64
        } else {
            0.0
        };

        let average_response_time_ms = if total_requests > 0 {
            total_response_time as f64 / total_requests as f64
        } else {
            0.0
        };

        let error_rate_percentage = if total_requests > 0 {
            (total_errors as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        // Calculate P95
        let p95_response_time_ms = {
            let times = self.response_times.read().await;
            if times.is_empty() {
                0.0
            } else {
                let mut sorted_times = times.clone();
                sorted_times.sort_unstable();
                let p95_index = (sorted_times.len() as f64 * 0.95) as usize;
                sorted_times.get(p95_index).copied().unwrap_or(0) as f64
            }
        };

        MetricsSnapshot {
            requests_per_second,
            average_response_time_ms,
            p95_response_time_ms,
            error_rate_percentage,
            total_requests,
            total_errors,
            uptime_seconds: uptime,
        }
    }

    pub fn reset_metrics(&self) {
        self.request_count.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
        self.total_response_time_ms.store(0, Ordering::Relaxed);
    }

    pub async fn get_recent_response_times(&self, count: usize) -> Vec<u64> {
        let times = self.response_times.read().await;
        times.iter()
            .rev()
            .take(count)
            .copied()
            .collect()
    }
}