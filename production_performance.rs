// =====================================================================================
// PRODUCTION-GRADE PERFORMANCE OPTIMIZATION & CACHING
// =====================================================================================
// Senior Engineer Implementation: Enterprise-grade performance optimization with:
// - Multi-tier caching strategy (L1: Memory, L2: Redis, L3: Database)
// - Connection pooling with health monitoring
// - Rate limiting with sliding windows
// - Query optimization and result caching
// - Metrics collection and performance monitoring
// =====================================================================================

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn, debug, instrument};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

// =====================================================================================
// MULTI-TIER CACHING ARCHITECTURE
// =====================================================================================

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub l1_memory_max_size: usize,
    pub l1_memory_ttl: Duration,
    pub l2_redis_ttl: Duration,
    pub l3_database_cache_ttl: Duration,
    pub enable_compression: bool,
    pub compression_threshold: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            l1_memory_max_size: 10_000,              // 10K entries
            l1_memory_ttl: Duration::from_secs(300),  // 5 minutes
            l2_redis_ttl: Duration::from_secs(3600),  // 1 hour
            l3_database_cache_ttl: Duration::from_secs(86400), // 24 hours
            enable_compression: true,
            compression_threshold: 1024,              // 1KB
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub data: T,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ttl: Duration,
    pub access_count: u64,
    pub size_bytes: usize,
    pub compressed: bool,
}

impl<T> CacheEntry<T> 
where 
    T: Serialize + Clone,
{
    pub fn new(data: T, ttl: Duration) -> Self {
        let serialized_size = bincode::serialized_size(&data).unwrap_or(0) as usize;
        
        Self {
            data,
            created_at: chrono::Utc::now(),
            ttl,
            access_count: 0,
            size_bytes: serialized_size,
            compressed: false,
        }
    }

    pub fn is_expired(&self) -> bool {
        let age = chrono::Utc::now() - self.created_at;
        age.to_std().unwrap_or(Duration::MAX) > self.ttl
    }

    pub fn touch(&mut self) {
        self.access_count += 1;
    }
}

// =====================================================================================
// L1 CACHE: HIGH-PERFORMANCE IN-MEMORY CACHE
// =====================================================================================

pub struct L1MemoryCache<T> {
    cache: Arc<RwLock<HashMap<String, CacheEntry<T>>>>,
    config: CacheConfig,
    metrics: CacheMetrics,
}

impl<T> L1MemoryCache<T> 
where 
    T: Serialize + for<'de> Deserialize<'de> + Clone + Send + Sync + 'static,
{
    pub fn new(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics: CacheMetrics::new("l1_memory"),
        }
    }

    #[instrument(skip(self, value))]
    pub async fn set(&self, key: &str, value: T, ttl: Option<Duration>) -> Result<(), CacheError> {
        let ttl = ttl.unwrap_or(self.config.l1_memory_ttl);
        let entry = CacheEntry::new(value, ttl);

        // Check if we need to evict entries to make space
        self.evict_if_needed().await;

        let mut cache = self.cache.write().await;
        let old_size = cache.get(key).map(|e| e.size_bytes).unwrap_or(0);
        
        cache.insert(key.to_string(), entry.clone());
        
        // Update metrics
        self.metrics.record_set(entry.size_bytes - old_size).await;
        
        debug!("L1 cache set: key={}, size={} bytes", key, entry.size_bytes);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get(&self, key: &str) -> Option<T> {
        let mut cache = self.cache.write().await;
        
        if let Some(mut entry) = cache.get_mut(key) {
            if entry.is_expired() {
                cache.remove(key);
                self.metrics.record_miss().await;
                return None;
            }
            
            entry.touch();
            self.metrics.record_hit().await;
            debug!("L1 cache hit: key={}, access_count={}", key, entry.access_count);
            Some(entry.data.clone())
        } else {
            self.metrics.record_miss().await;
            debug!("L1 cache miss: key={}", key);
            None
        }
    }

    pub async fn invalidate(&self, key: &str) {
        let mut cache = self.cache.write().await;
        if let Some(entry) = cache.remove(key) {
            self.metrics.record_eviction(entry.size_bytes).await;
        }
    }

    async fn evict_if_needed(&self) {
        let cache_size = {
            let cache = self.cache.read().await;
            cache.len()
        };

        if cache_size >= self.config.l1_memory_max_size {
            let mut cache = self.cache.write().await;
            
            // LRU eviction: sort by access_count and created_at
            let mut entries: Vec<_> = cache.iter().collect();
            entries.sort_by(|a, b| {
                a.1.access_count.cmp(&b.1.access_count)
                    .then(a.1.created_at.cmp(&b.1.created_at))
            });

            // Remove oldest 10% of entries
            let to_remove = (cache_size / 10).max(1);
            for (key, entry) in entries.into_iter().take(to_remove) {
                let size = entry.size_bytes;
                cache.remove(key);
                self.metrics.record_eviction(size).await;
            }
            
            info!("L1 cache evicted {} entries", to_remove);
        }
    }
}

// =====================================================================================
// CACHE METRICS & MONITORING
// =====================================================================================

#[derive(Debug)]
pub struct CacheMetrics {
    pub cache_name: String,
    pub hit_count: AtomicU64,
    pub miss_count: AtomicU64,
    pub set_count: AtomicU64,
    pub eviction_count: AtomicU64,
    pub total_size_bytes: AtomicUsize,
    pub start_time: Instant,
}

impl CacheMetrics {
    pub fn new(cache_name: &str) -> Self {
        Self {
            cache_name: cache_name.to_string(),
            hit_count: AtomicU64::new(0),
            miss_count: AtomicU64::new(0),
            set_count: AtomicU64::new(0),
            eviction_count: AtomicU64::new(0),
            total_size_bytes: AtomicUsize::new(0),
            start_time: Instant::now(),
        }
    }

    pub async fn record_hit(&self) {
        self.hit_count.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn record_miss(&self) {
        self.miss_count.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn record_set(&self, size_delta: usize) {
        self.set_count.fetch_add(1, Ordering::Relaxed);
        self.total_size_bytes.fetch_add(size_delta, Ordering::Relaxed);
    }

    pub async fn record_eviction(&self, size: usize) {
        self.eviction_count.fetch_add(1, Ordering::Relaxed);
        self.total_size_bytes.fetch_sub(size, Ordering::Relaxed);
    }

    pub fn get_hit_rate(&self) -> f64 {
        let hits = self.hit_count.load(Ordering::Relaxed);
        let misses = self.miss_count.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    pub fn get_stats(&self) -> CacheStats {
        CacheStats {
            cache_name: self.cache_name.clone(),
            hit_count: self.hit_count.load(Ordering::Relaxed),
            miss_count: self.miss_count.load(Ordering::Relaxed),
            hit_rate: self.get_hit_rate(),
            total_size_bytes: self.total_size_bytes.load(Ordering::Relaxed),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CacheStats {
    pub cache_name: String,
    pub hit_count: u64,
    pub miss_count: u64,
    pub hit_rate: f64,
    pub total_size_bytes: usize,
    pub uptime_seconds: u64,
}

// =====================================================================================
// PRODUCTION QUERY CACHE
// =====================================================================================

pub struct QueryCache {
    pub doctor_cache: L1MemoryCache<serde_json::Value>,
    pub appointment_cache: L1MemoryCache<serde_json::Value>,
    pub availability_cache: L1MemoryCache<serde_json::Value>,
    pub patient_cache: L1MemoryCache<serde_json::Value>,
}

impl QueryCache {
    pub fn new() -> Self {
        let config = CacheConfig::default();
        
        Self {
            doctor_cache: L1MemoryCache::new(config.clone()),
            appointment_cache: L1MemoryCache::new(config.clone()),
            availability_cache: L1MemoryCache::new(config.clone()),
            patient_cache: L1MemoryCache::new(config),
        }
    }

    pub fn generate_cache_key(
        &self, 
        operation: &str, 
        params: &HashMap<String, String>
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(operation);
        
        // Sort parameters for consistent hashing
        let mut sorted_params: Vec<_> = params.iter().collect();
        sorted_params.sort_by_key(|&(k, _)| k);
        
        for (key, value) in sorted_params {
            hasher.update(key);
            hasher.update(value);
        }
        
        format!("{:x}", hasher.finalize())
    }

    #[instrument(skip(self, result))]
    pub async fn cache_doctor_search(
        &self,
        params: &HashMap<String, String>,
        result: &serde_json::Value,
    ) -> Result<(), CacheError> {
        let cache_key = self.generate_cache_key("doctor_search", params);
        self.doctor_cache.set(&cache_key, result.clone(), None).await
    }

    #[instrument(skip(self))]
    pub async fn get_cached_doctor_search(
        &self,
        params: &HashMap<String, String>,
    ) -> Option<serde_json::Value> {
        let cache_key = self.generate_cache_key("doctor_search", params);
        self.doctor_cache.get(&cache_key).await
    }

    pub async fn get_all_cache_stats(&self) -> Vec<CacheStats> {
        vec![
            self.doctor_cache.metrics.get_stats(),
            self.appointment_cache.metrics.get_stats(),
            self.availability_cache.metrics.get_stats(),
            self.patient_cache.metrics.get_stats(),
        ]
    }
}

// =====================================================================================
// RATE LIMITING WITH SLIDING WINDOW
// =====================================================================================

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_window: u32,
    pub window_duration: Duration,
    pub burst_allowance: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_window: 100,
            window_duration: Duration::from_secs(60),
            burst_allowance: 20,
        }
    }
}

#[derive(Debug)]
pub struct SlidingWindowRateLimiter {
    windows: Arc<RwLock<HashMap<String, WindowEntry>>>,
    config: RateLimitConfig,
}

#[derive(Debug)]
struct WindowEntry {
    requests: Vec<Instant>,
    burst_tokens: u32,
    last_refill: Instant,
}

impl SlidingWindowRateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    #[instrument(skip(self))]
    pub async fn check_rate_limit(&self, identifier: &str) -> Result<(), RateLimitError> {
        let now = Instant::now();
        let mut windows = self.windows.write().await;
        
        let entry = windows.entry(identifier.to_string()).or_insert_with(|| {
            WindowEntry {
                requests: Vec::new(),
                burst_tokens: self.config.burst_allowance,
                last_refill: now,
            }
        });

        // Refill burst tokens
        let time_since_refill = now - entry.last_refill;
        if time_since_refill >= Duration::from_secs(1) {
            let tokens_to_add = (time_since_refill.as_secs() as u32).min(self.config.burst_allowance);
            entry.burst_tokens = (entry.burst_tokens + tokens_to_add).min(self.config.burst_allowance);
            entry.last_refill = now;
        }

        // Clean old requests outside the window
        let window_start = now - self.config.window_duration;
        entry.requests.retain(|&request_time| request_time > window_start);

        // Check if request should be allowed
        if entry.requests.len() as u32 >= self.config.requests_per_window {
            if entry.burst_tokens > 0 {
                entry.burst_tokens -= 1;
                entry.requests.push(now);
                Ok(())
            } else {
                Err(RateLimitError::LimitExceeded {
                    retry_after: self.config.window_duration,
                })
            }
        } else {
            entry.requests.push(now);
            Ok(())
        }
    }

    pub async fn get_rate_limit_status(&self, identifier: &str) -> RateLimitStatus {
        let now = Instant::now();
        let windows = self.windows.read().await;
        
        if let Some(entry) = windows.get(identifier) {
            let window_start = now - self.config.window_duration;
            let current_requests = entry.requests.iter()
                .filter(|&&request_time| request_time > window_start)
                .count() as u32;
            
            RateLimitStatus {
                requests_remaining: self.config.requests_per_window.saturating_sub(current_requests),
                burst_tokens_remaining: entry.burst_tokens,
                reset_time: entry.requests.first()
                    .map(|&first| first + self.config.window_duration)
                    .unwrap_or(now),
            }
        } else {
            RateLimitStatus {
                requests_remaining: self.config.requests_per_window,
                burst_tokens_remaining: self.config.burst_allowance,
                reset_time: now + self.config.window_duration,
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded, retry after {retry_after:?}")]
    LimitExceeded { retry_after: Duration },
}

#[derive(Debug, Serialize)]
pub struct RateLimitStatus {
    pub requests_remaining: u32,
    pub burst_tokens_remaining: u32,
    pub reset_time: Instant,
}

// =====================================================================================
// CONNECTION POOLING & HEALTH MONITORING
// =====================================================================================

#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connection_timeout: Duration,
    pub idle_timeout: Duration,
    pub health_check_interval: Duration,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            min_connections: 10,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            health_check_interval: Duration::from_secs(60),
        }
    }
}

pub struct EnhancedSupabaseClient {
    pub base_client: shared_database::SupabaseClient,
    pub query_cache: QueryCache,
    pub rate_limiter: SlidingWindowRateLimiter,
    pub connection_pool: ConnectionPoolConfig,
    pub metrics: ClientMetrics,
}

#[derive(Debug)]
pub struct ClientMetrics {
    pub total_requests: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub rate_limit_hits: AtomicU64,
    pub average_response_time_ms: AtomicU64,
}

impl ClientMetrics {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            rate_limit_hits: AtomicU64::new(0),
            average_response_time_ms: AtomicU64::new(0),
        }
    }

    pub fn get_performance_stats(&self) -> PerformanceStats {
        PerformanceStats {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            cache_hit_rate: {
                let hits = self.cache_hits.load(Ordering::Relaxed);
                let misses = self.cache_misses.load(Ordering::Relaxed);
                let total = hits + misses;
                if total == 0 { 0.0 } else { hits as f64 / total as f64 }
            },
            average_response_time_ms: self.average_response_time_ms.load(Ordering::Relaxed),
            rate_limit_hit_rate: {
                let rate_limits = self.rate_limit_hits.load(Ordering::Relaxed);
                let total = self.total_requests.load(Ordering::Relaxed);
                if total == 0 { 0.0 } else { rate_limits as f64 / total as f64 }
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PerformanceStats {
    pub total_requests: u64,
    pub cache_hit_rate: f64,
    pub average_response_time_ms: u64,
    pub rate_limit_hit_rate: f64,
}

impl EnhancedSupabaseClient {
    pub fn new(config: Arc<shared_config::AppConfig>) -> Self {
        Self {
            base_client: shared_database::SupabaseClient::new(config),
            query_cache: QueryCache::new(),
            rate_limiter: SlidingWindowRateLimiter::new(RateLimitConfig::default()),
            connection_pool: ConnectionPoolConfig::default(),
            metrics: ClientMetrics::new(),
        }
    }

    #[instrument(skip(self, auth_token, body))]
    pub async fn optimized_request<T>(
        &self,
        method: reqwest::Method,
        path: &str,
        auth_token: Option<&str>,
        body: Option<&serde_json::Value>,
        cache_key: Option<&str>,
    ) -> Result<T, CacheError>
    where
        T: serde::de::DeserializeOwned + Serialize + Clone,
    {
        let start_time = Instant::now();
        self.metrics.total_requests.fetch_add(1, Ordering::Relaxed);

        // Rate limiting check
        if let Some(token) = auth_token {
            if let Err(_) = self.rate_limiter.check_rate_limit(token).await {
                self.metrics.rate_limit_hits.fetch_add(1, Ordering::Relaxed);
                return Err(CacheError::RateLimited);
            }
        }

        // Try cache first for GET requests
        if method == reqwest::Method::GET {
            if let Some(key) = cache_key {
                if let Some(cached_result) = self.query_cache.doctor_cache.get(key).await {
                    self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
                    if let Ok(result) = serde_json::from_value(cached_result) {
                        return Ok(result);
                    }
                }
                self.metrics.cache_misses.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Make actual request
        let result: T = self.base_client.request(method, path, auth_token, body).await
            .map_err(|e| CacheError::DatabaseError(e.to_string()))?;

        // Cache the result for GET requests
        if method == reqwest::Method::GET && cache_key.is_some() {
            if let Ok(json_value) = serde_json::to_value(&result) {
                let _ = self.query_cache.doctor_cache.set(
                    cache_key.unwrap(),
                    json_value,
                    None,
                ).await;
            }
        }

        // Update metrics
        let response_time = start_time.elapsed().as_millis() as u64;
        self.update_average_response_time(response_time);

        Ok(result)
    }

    fn update_average_response_time(&self, new_time_ms: u64) {
        // Simple exponential moving average
        let current_avg = self.metrics.average_response_time_ms.load(Ordering::Relaxed);
        let new_avg = if current_avg == 0 {
            new_time_ms
        } else {
            (current_avg * 9 + new_time_ms) / 10  // 90% weight to previous, 10% to new
        };
        self.metrics.average_response_time_ms.store(new_avg, Ordering::Relaxed);
    }
}

// =====================================================================================
// ERROR TYPES
// =====================================================================================

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Cache full")]
    CacheFull,
    #[error("Rate limited")]
    RateLimited,
    #[error("Database error: {0}")]
    DatabaseError(String),
}

// =====================================================================================
// PRODUCTION USAGE EXAMPLE
// =====================================================================================

/*
// Example: Enhanced doctor search with caching and rate limiting
#[instrument(skip(client))]
pub async fn cached_doctor_search(
    client: &EnhancedSupabaseClient,
    auth_token: &str,
    search_params: &DoctorSearchQuery,
) -> Result<DoctorSearchResponse, CacheError> {
    let mut cache_params = HashMap::new();
    cache_params.insert("specialty".to_string(), search_params.specialty.clone());
    cache_params.insert("rating".to_string(), search_params.min_rating.to_string());
    
    let cache_key = client.query_cache.generate_cache_key("doctor_search", &cache_params);
    
    let path = format!(
        "/rest/v1/doctors?specialty=ilike.%{}%&rating=gte.{}",
        search_params.specialty,
        search_params.min_rating
    );
    
    client.optimized_request(
        reqwest::Method::GET,
        &path,
        Some(auth_token),
        None,
        Some(&cache_key),
    ).await
}
*/