use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use chrono::{ Utc, TimeZone};
use tracing::debug;
use shared_models::auth::{JwtClaims, User};

type HmacSha256 = Hmac<Sha256>;

pub fn validate_token(token: &str, jwt_secret: &str) -> Result<User, String> {
    if jwt_secret.is_empty() {
        return Err("JWT secret is not set".to_string());
    }

    // Split token into parts
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid token format".to_string());
    }

    let header_b64 = parts[0];
    let claims_b64 = parts[1];
    let signature_b64 = parts[2];

    // Verify signature
    let signature = match URL_SAFE_NO_PAD.decode(signature_b64) {
        Ok(sig) => sig,
        Err(e) => {
            debug!("Failed to decode signature: {}", e);
            return Err("Invalid signature encoding".to_string());
        }
    };

    // Create signature string
    let signature_string = format!("{}.{}", header_b64, claims_b64);
    
    // Validate signature
    let mut mac = match HmacSha256::new_from_slice(jwt_secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return Err("Failed to create HMAC".to_string()),
    };
    
    mac.update(signature_string.as_bytes());
    
    // Verify signature
    if let Err(_) = mac.verify_slice(&signature) {
        debug!("Token signature verification failed");
        return Err("Invalid token signature".to_string());
    }

    // Decode claims
    let claims_json = match URL_SAFE_NO_PAD.decode(claims_b64) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(json_str) => json_str,
            Err(_) => return Err("Invalid claims encoding".to_string()),
        },
        Err(_) => return Err("Invalid claims encoding".to_string()),
    };
    
    // Parse claims
    let claims: JwtClaims = match serde_json::from_str(&claims_json) {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to parse claims: {}", e);
            return Err("Invalid claims format".to_string());
        },
    };
    
    // Check expiration
    if let Some(exp) = claims.exp {
        let now = chrono::Utc::now().timestamp() as u64;
        if exp < now {
            debug!("Token expired at {} (now: {})", exp, now);
            return Err("Token expired".to_string());
        }
    }

    // Extract creation time
    let created_at = claims.iat
        .map(|timestamp| Utc.timestamp_opt(timestamp as i64, 0).single());
    
    // Return user
    let user = User {
        id: claims.sub,
        email: claims.email,
        role: claims.role,
        metadata: claims.user_metadata,
        created_at: created_at.flatten(),
    };
    
    debug!("Token validated successfully for user: {}", user.id);
    Ok(user)
}