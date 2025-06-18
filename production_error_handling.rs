// =====================================================================================
// PRODUCTION-GRADE ERROR HANDLING & LOGGING IMPROVEMENTS
// =====================================================================================
// Senior Engineer Implementation: Enterprise-grade error handling with:
// - Structured logging with correlation IDs
// - Circuit breaker patterns for external dependencies
// - Comprehensive error categorization and recovery strategies
// - Performance monitoring and alerting integration
// =====================================================================================

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, warn, info, debug, instrument, Span};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

// =====================================================================================
// PRODUCTION ERROR TYPES WITH COMPREHENSIVE CATEGORIZATION
// =====================================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    pub correlation_id: String,
    pub user_id: Option<String>,
    pub endpoint: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub request_duration_ms: Option<u64>,
    pub additional_context: HashMap<String, serde_json::Value>,
}

impl ErrorContext {
    pub fn new(correlation_id: String) -> Self {
        Self {
            correlation_id,
            user_id: None,
            endpoint: None,
            timestamp: chrono::Utc::now(),
            request_duration_ms: None,
            additional_context: HashMap::new(),
        }
    }

    pub fn with_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.request_duration_ms = Some(duration_ms);
        self
    }

    pub fn add_context<T: Serialize>(mut self, key: &str, value: T) -> Self {
        if let Ok(serialized) = serde_json::to_value(value) {
            self.additional_context.insert(key.to_string(), serialized);
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Critical,   // System-wide failures, immediate attention required
    High,       // Service degradation, affects multiple users
    Medium,     // Single user failures, retry possible
    Low,        // Expected errors, graceful handling
    Info,       // Informational, no action required
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCategory {
    Authentication,     // JWT, permissions, user access
    Authorization,      // Role-based access, resource permissions
    Validation,         // Input validation, schema violations
    Database,          // SQL errors, connection issues, schema problems
    ExternalService,   // Supabase, Cloudflare, third-party APIs
    BusinessLogic,     // Domain-specific logic errors
    Performance,       // Timeouts, rate limits, resource exhaustion
    Security,          // Suspicious activity, potential attacks
    Infrastructure,    // Network, storage, compute failures
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionError {
    pub error_id: String,
    pub correlation_id: String,
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub message: String,
    pub technical_details: String,
    pub user_message: String,
    pub recovery_suggestions: Vec<String>,
    pub context: ErrorContext,
    pub stack_trace: Option<String>,
    pub metrics: ErrorMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorMetrics {
    pub occurrence_count: u64,
    pub first_seen: chrono::DateTime<chrono::Utc>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub affected_users: Vec<String>,
    pub performance_impact_ms: Option<u64>,
}

impl ProductionError {
    pub fn new(
        category: ErrorCategory,
        severity: ErrorSeverity,
        message: String,
        context: ErrorContext,
    ) -> Self {
        let error_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        
        Self {
            error_id: error_id.clone(),
            correlation_id: context.correlation_id.clone(),
            category: category.clone(),
            severity: severity.clone(),
            message: message.clone(),
            technical_details: message.clone(),
            user_message: Self::generate_user_message(&category, &severity),
            recovery_suggestions: Self::generate_recovery_suggestions(&category),
            context,
            stack_trace: None,
            metrics: ErrorMetrics {
                occurrence_count: 1,
                first_seen: now,
                last_seen: now,
                affected_users: vec![],
                performance_impact_ms: None,
            },
        }
    }

    fn generate_user_message(category: &ErrorCategory, severity: &ErrorSeverity) -> String {
        match (category, severity) {
            (ErrorCategory::Authentication, _) => 
                "Please check your login credentials and try again.".to_string(),
            (ErrorCategory::Authorization, _) => 
                "You don't have permission to perform this action.".to_string(),
            (ErrorCategory::Validation, _) => 
                "Please check your input and try again.".to_string(),
            (ErrorCategory::Database, ErrorSeverity::Critical) => 
                "Our systems are temporarily unavailable. Please try again in a few minutes.".to_string(),
            (ErrorCategory::ExternalService, _) => 
                "Our service is experiencing connectivity issues. Please try again shortly.".to_string(),
            (ErrorCategory::Performance, _) => 
                "The request is taking longer than expected. Please try again.".to_string(),
            _ => "An unexpected error occurred. Please try again or contact support if the problem persists.".to_string(),
        }
    }

    fn generate_recovery_suggestions(category: &ErrorCategory) -> Vec<String> {
        match category {
            ErrorCategory::Authentication => vec![
                "Verify your email and password".to_string(),
                "Clear browser cookies and try again".to_string(),
                "Reset your password if needed".to_string(),
            ],
            ErrorCategory::Database => vec![
                "Retry the operation after a brief delay".to_string(),
                "Check for concurrent operations".to_string(),
                "Verify data integrity".to_string(),
            ],
            ErrorCategory::ExternalService => vec![
                "Implement exponential backoff retry".to_string(),
                "Check external service status".to_string(),
                "Use cached data if available".to_string(),
            ],
            ErrorCategory::Performance => vec![
                "Reduce request payload size".to_string(),
                "Implement request pagination".to_string(),
                "Use background processing for heavy operations".to_string(),
            ],
            _ => vec!["Retry the operation".to_string()],
        }
    }

    #[instrument(skip(self))]
    pub fn log(&self) {
        match self.severity {
            ErrorSeverity::Critical => {
                error!(
                    error_id = %self.error_id,
                    correlation_id = %self.correlation_id,
                    category = ?self.category,
                    severity = ?self.severity,
                    user_id = ?self.context.user_id,
                    endpoint = ?self.context.endpoint,
                    duration_ms = ?self.context.request_duration_ms,
                    occurrence_count = self.metrics.occurrence_count,
                    "CRITICAL ERROR: {}", self.message
                );
            },
            ErrorSeverity::High => {
                error!(
                    error_id = %self.error_id,
                    correlation_id = %self.correlation_id,
                    category = ?self.category,
                    "HIGH SEVERITY ERROR: {}", self.message
                );
            },
            ErrorSeverity::Medium => {
                warn!(
                    error_id = %self.error_id,
                    correlation_id = %self.correlation_id,
                    category = ?self.category,
                    "MEDIUM SEVERITY ERROR: {}", self.message
                );
            },
            ErrorSeverity::Low => {
                info!(
                    error_id = %self.error_id,
                    correlation_id = %self.correlation_id,
                    "LOW SEVERITY ERROR: {}", self.message
                );
            },
            ErrorSeverity::Info => {
                debug!(
                    error_id = %self.error_id,
                    correlation_id = %self.correlation_id,
                    "INFO: {}", self.message
                );
            },
        }
    }
}

// =====================================================================================
// CIRCUIT BREAKER PATTERN FOR EXTERNAL DEPENDENCIES
// =====================================================================================

#[derive(Debug, Clone)]
pub enum CircuitBreakerState {
    Closed,    // Normal operation
    Open,      // Failing, reject requests
    HalfOpen,  // Testing if service recovered
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerState>>,
    failure_count: Arc<AtomicU64>,
    success_count: Arc<AtomicU64>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,
    pub recovery_timeout: Duration,
    pub success_threshold: u64,
    pub timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            success_threshold: 3,
            timeout: Duration::from_secs(30),
        }
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            config,
        }
    }

    #[instrument(skip(self, operation))]
    pub async fn execute<F, R, E>(&self, operation: F) -> Result<R, CircuitBreakerError<E>>
    where
        F: std::future::Future<Output = Result<R, E>>,
        E: std::fmt::Debug,
    {
        // Check if circuit breaker should allow the request
        if !self.should_allow_request().await {
            return Err(CircuitBreakerError::CircuitOpen);
        }

        // Execute the operation with timeout
        let start_time = Instant::now();
        let result = tokio::time::timeout(self.config.timeout, operation).await;

        match result {
            Ok(Ok(success)) => {
                self.on_success().await;
                Ok(success)
            },
            Ok(Err(error)) => {
                self.on_failure().await;
                Err(CircuitBreakerError::OperationFailed(error))
            },
            Err(_) => {
                self.on_failure().await;
                Err(CircuitBreakerError::Timeout)
            },
        }
    }

    async fn should_allow_request(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                // Check if enough time has passed to try again
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() >= self.config.recovery_timeout {
                        drop(state);
                        *self.state.write().await = CircuitBreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            },
            CircuitBreakerState::HalfOpen => true,
        }
    }

    async fn on_success(&self) {
        let current_state = {
            let state = self.state.read().await;
            state.clone()
        };

        match current_state {
            CircuitBreakerState::HalfOpen => {
                let success_count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if success_count >= self.config.success_threshold {
                    *self.state.write().await = CircuitBreakerState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                    self.success_count.store(0, Ordering::SeqCst);
                    info!("Circuit breaker reset to CLOSED state");
                }
            },
            _ => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
        }
    }

    async fn on_failure(&self) {
        let failure_count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure_time.write().await = Some(Instant::now());

        if failure_count >= self.config.failure_threshold {
            *self.state.write().await = CircuitBreakerState::Open;
            warn!("Circuit breaker opened due to {} failures", failure_count);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError<E> {
    #[error("Circuit breaker is open")]
    CircuitOpen,
    #[error("Operation timed out")]
    Timeout,
    #[error("Operation failed: {0:?}")]
    OperationFailed(E),
}

// =====================================================================================
// PRODUCTION ERROR HANDLER MIDDLEWARE
// =====================================================================================

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

#[derive(Clone)]
pub struct ErrorHandlerState {
    pub circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    pub error_tracker: Arc<RwLock<HashMap<String, ProductionError>>>,
}

impl ErrorHandlerState {
    pub fn new() -> Self {
        Self {
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            error_tracker: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_or_create_circuit_breaker(&self, service_name: &str) -> CircuitBreaker {
        let circuit_breakers = self.circuit_breakers.read().await;
        if let Some(cb) = circuit_breakers.get(service_name) {
            cb.clone()
        } else {
            drop(circuit_breakers);
            let mut circuit_breakers = self.circuit_breakers.write().await;
            let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
            circuit_breakers.insert(service_name.to_string(), cb.clone());
            cb
        }
    }
}

#[instrument(skip(state, request, next))]
pub async fn error_handling_middleware(
    State(state): State<ErrorHandlerState>,
    mut request: Request,
    next: Next,
) -> Response {
    let correlation_id = Uuid::new_v4().to_string();
    let start_time = Instant::now();
    
    // Add correlation ID to request headers
    request.headers_mut().insert(
        "x-correlation-id",
        HeaderValue::from_str(&correlation_id).unwrap(),
    );

    // Add correlation ID to tracing span
    Span::current().record("correlation_id", &correlation_id);

    let response = next.run(request).await;
    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Log response metrics
    info!(
        correlation_id = %correlation_id,
        duration_ms = duration_ms,
        status_code = %response.status(),
        "Request completed"
    );

    // Add correlation ID to response headers
    let mut response = response;
    response.headers_mut().insert(
        "x-correlation-id",
        HeaderValue::from_str(&correlation_id).unwrap(),
    );

    response
}

// =====================================================================================
// ENHANCED SUPABASE CLIENT WITH CIRCUIT BREAKER
// =====================================================================================

pub struct ProductionSupabaseClient {
    pub base_client: shared_database::SupabaseClient,
    pub circuit_breaker: CircuitBreaker,
    pub error_handler: ErrorHandlerState,
}

impl ProductionSupabaseClient {
    pub fn new(config: Arc<shared_config::AppConfig>) -> Self {
        Self {
            base_client: shared_database::SupabaseClient::new(config),
            circuit_breaker: CircuitBreaker::new(CircuitBreakerConfig::default()),
            error_handler: ErrorHandlerState::new(),
        }
    }

    #[instrument(skip(self, auth_token, body))]
    pub async fn request_with_circuit_breaker<T>(
        &self,
        method: reqwest::Method,
        path: &str,
        auth_token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<T, ProductionError>
    where
        T: serde::de::DeserializeOwned,
    {
        let correlation_id = Uuid::new_v4().to_string();
        let context = ErrorContext::new(correlation_id.clone())
            .with_endpoint(format!("{} {}", method, path));

        let operation = async {
            self.base_client.request(method, path, auth_token, body).await
        };

        match self.circuit_breaker.execute(operation).await {
            Ok(result) => Ok(result),
            Err(CircuitBreakerError::CircuitOpen) => {
                let error = ProductionError::new(
                    ErrorCategory::ExternalService,
                    ErrorSeverity::High,
                    "Supabase service circuit breaker is open".to_string(),
                    context,
                );
                error.log();
                Err(error)
            },
            Err(CircuitBreakerError::Timeout) => {
                let error = ProductionError::new(
                    ErrorCategory::Performance,
                    ErrorSeverity::Medium,
                    format!("Supabase request timeout for {}", path),
                    context,
                );
                error.log();
                Err(error)
            },
            Err(CircuitBreakerError::OperationFailed(e)) => {
                let error = ProductionError::new(
                    ErrorCategory::ExternalService,
                    ErrorSeverity::Medium,
                    format!("Supabase operation failed: {}", e),
                    context,
                );
                error.log();
                Err(error)
            },
        }
    }
}

// =====================================================================================
// PRODUCTION ERROR RESPONSES
// =====================================================================================

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub error_id: String,
    pub correlation_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub suggestions: Vec<String>,
}

impl IntoResponse for ProductionError {
    fn into_response(self) -> Response {
        let status_code = match (&self.category, &self.severity) {
            (ErrorCategory::Authentication, _) => StatusCode::UNAUTHORIZED,
            (ErrorCategory::Authorization, _) => StatusCode::FORBIDDEN,
            (ErrorCategory::Validation, _) => StatusCode::BAD_REQUEST,
            (ErrorCategory::Database, ErrorSeverity::Critical) => StatusCode::SERVICE_UNAVAILABLE,
            (ErrorCategory::ExternalService, _) => StatusCode::BAD_GATEWAY,
            (ErrorCategory::Performance, _) => StatusCode::REQUEST_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let error_response = ErrorResponse {
            error: self.user_message.clone(),
            error_id: self.error_id.clone(),
            correlation_id: self.correlation_id.clone(),
            timestamp: chrono::Utc::now(),
            suggestions: self.recovery_suggestions.clone(),
        };

        self.log();

        (status_code, Json(error_response)).into_response()
    }
}

// =====================================================================================
// PRODUCTION USAGE EXAMPLES
// =====================================================================================

/*
// Example: Using enhanced error handling in a handler
#[instrument(skip(state))]
pub async fn enhanced_doctor_search(
    State(state): State<Arc<AppConfig>>,
    Query(query): Query<DoctorSearchQuery>,
) -> Result<Json<DoctorSearchResponse>, ProductionError> {
    let correlation_id = Uuid::new_v4().to_string();
    let context = ErrorContext::new(correlation_id)
        .with_endpoint("GET /doctors/search".to_string());

    let client = ProductionSupabaseClient::new(state.clone());
    
    let result = client.request_with_circuit_breaker(
        reqwest::Method::GET,
        &format!("/rest/v1/doctors?specialty=ilike.%{}%", query.specialty),
        None,
        None,
    ).await?;

    Ok(Json(result))
}

// Example: Circuit breaker usage for external service
let supabase_cb = error_handler.get_or_create_circuit_breaker("supabase").await;
let result = supabase_cb.execute(async {
    // Your Supabase operation here
    supabase_client.request(method, path, token, body).await
}).await;
*/