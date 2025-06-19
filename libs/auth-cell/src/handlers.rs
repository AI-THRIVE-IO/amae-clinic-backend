use std::sync::Arc;

use axum::{
    extract::State,
    http::HeaderMap,
    Json,
};
use axum_extra::TypedHeader;
use headers::Authorization;
use headers::authorization::Bearer;
use serde_json::json;
use tracing::debug;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;
use shared_models::auth::TokenResponse;
use shared_models::error::AppError;
use shared_utils::jwt::validate_token as jwt_validate_token;

// Helper function to extract token
fn extract_bearer_token(headers: &HeaderMap) -> Result<String, AppError> {
    let auth_header = headers
        .get("Authorization")
        .ok_or_else(|| AppError::Auth("Missing authorization header".to_string()))?;

    let auth_value = auth_header
        .to_str()
        .map_err(|_| AppError::Auth("Invalid authorization header format".to_string()))?;

    if !auth_value.starts_with("Bearer ") {
        return Err(AppError::Auth("Invalid authorization header format".to_string()));
    }

    Ok(auth_value[7..].to_string())
}

pub async fn validate_token(
    State(config): State<Arc<AppConfig>>,
    headers: HeaderMap,
) -> Result<Json<TokenResponse>, AppError> {
    debug!("Validating token");

    let token = extract_bearer_token(&headers)?;

    match jwt_validate_token(&token, &config.supabase_jwt_secret) {
        Ok(user) => {
            let response = TokenResponse {
                valid: true,
                user_id: user.id,
                email: user.email,
                role: user.role,
            };

            Ok(Json(response))
        },
        Err(err) => {
            Err(AppError::Auth(err))
        }
    }
}

pub async fn verify_token(
    State(config): State<Arc<AppConfig>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    debug!("Verifying token");

    let token = extract_bearer_token(&headers)?;

    match jwt_validate_token(&token, &config.supabase_jwt_secret) {
        Ok(_) => {
            Ok(Json(json!({ "valid": true })))
        },
        Err(_) => {
            Ok(Json(json!({ "valid": false })))
        }
    }
}

// Production-hardened get_profile handler with fallback logic
#[axum::debug_handler]
pub async fn get_profile(
    State(config): State<Arc<AppConfig>>,
    TypedHeader(auth_header): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let token = auth_header.token();

    // Get the user ID from the token
    let user = jwt_validate_token(token, &config.supabase_jwt_secret)
        .map_err(|e| AppError::Auth(e))?;

    debug!("Getting profile for user: {} with production-hardened logic", user.id);

    // Create Supabase client
    let client = SupabaseClient::new(&config);

    // Primary attempt: Try to get auth profile
    let auth_profile = match client.get_user_profile(&user.id, token).await {
        Ok(profile) => profile,
        Err(e) => {
            debug!("Auth profile query failed: {}, using fallback", e);
            json!({
                "id": user.id,
                "email": user.email,
                "role": user.role,
                "metadata": user.metadata,
                "created_at": user.created_at,
                "fallback": true
            })
        }
    };

    // Primary attempt: Try to get health profile with simplified query
    let health_profile = match get_simplified_health_profile(&client, &user.id, token).await {
        Ok(profile) => profile,
        Err(e) => {
            debug!("Health profile query failed: {}, using fallback", e);
            json!({
                "patient_id": user.id,
                "exists": false,
                "fallback": true
            })
        }
    };

    Ok(Json(json!({
        "user_id": user.id,
        "auth_profile": auth_profile,
        "health_profile": health_profile
    })))
}

// Simplified health profile query avoiding JSON operators
async fn get_simplified_health_profile(
    client: &SupabaseClient,
    user_id: &str,
    auth_token: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    // Directly call the existing method with fallback on error
    match client.get_health_profile(user_id, auth_token).await {
        Ok(profile) => Ok(profile),
        Err(_) => {
            // Fallback: return minimal health profile
            Ok(json!({
                "patient_id": user_id,
                "exists": false,
                "fallback": true
            }))
        }
    }
}
