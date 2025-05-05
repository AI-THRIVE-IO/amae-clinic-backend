use std::sync::Arc;

use axum::{
    extract::{State, FromRef},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    RequestExt,
};

use shared_models::auth::User;
use shared_models::error::AppError;
use shared_config::AppConfig;

use crate::jwt::validate_token;

// Middleware for authentication
pub async fn auth_middleware<B>(
    State(config): State<Arc<AppConfig>>,
    mut request: Request<B>,
    next: Next<B>,
) -> Result<Response, AppError> {
    // Extract token from headers
    let auth_header = request
        .headers()
        .get("Authorization")
        .ok_or_else(|| AppError::Auth("Missing authorization header".to_string()))?;
    
    let auth_value = auth_header
        .to_str()
        .map_err(|_| AppError::Auth("Invalid authorization header format".to_string()))?;
    
    if !auth_value.starts_with("Bearer ") {
        return Err(AppError::Auth("Invalid authorization header format".to_string()));
    }
    
    let token = &auth_value[7..];
    
    // Validate token
    let user = validate_token(token, &config.supabase_jwt_secret)
        .map_err(|e| AppError::Auth(e))?;
    
    // Add user to request extensions
    request.extensions_mut().insert(user);
    
    // Continue with the request
    Ok(next.run(request).await)
}

// Function to extract user from request extensions
pub async fn extract_user<B>(request: &Request<B>) -> Result<User, AppError> {
    request
        .extensions()
        .get::<User>()
        .cloned()
        .ok_or_else(|| AppError::Auth("User not found in request extensions".to_string()))
}