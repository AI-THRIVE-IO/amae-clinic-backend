use std::env;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub supabase_url: String,
    pub supabase_anon_key: String,
    pub supabase_jwt_secret: String,
    pub cloudflare_realtime_app_id: String,
    pub cloudflare_realtime_api_token: String,
    pub cloudflare_realtime_base_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let config = Self {
            supabase_url: env::var("SUPABASE_URL")
                .unwrap_or_else(|_| {
                    warn!("SUPABASE_URL not set, using empty value");
                    String::new()
                }),
            supabase_anon_key: env::var("SUPABASE_ANON_PUBLIC_KEY")
                .unwrap_or_else(|_| {
                    warn!("SUPABASE_ANON_PUBLIC_KEY not set, using empty value");
                    String::new()
                }),
            supabase_jwt_secret: env::var("SUPABASE_JWT_SECRET")
                .unwrap_or_else(|_| {
                    warn!("SUPABASE_JWT_SECRET not set, using empty value");
                    String::new()
                }),
            cloudflare_realtime_app_id: env::var("CLOUDFLARE_REALTIME_APP_ID")
                .unwrap_or_else(|_| {
                    warn!("CLOUDFLARE_REALTIME_APP_ID not set, using empty value");
                    String::new()
                }),
            cloudflare_realtime_api_token: env::var("CLOUDFLARE_REALTIME_API_TOKEN")
                .unwrap_or_else(|_| {
                    warn!("CLOUDFLARE_REALTIME_API_TOKEN not set, using empty value");
                    String::new()
                }),
            cloudflare_realtime_base_url: env::var("CLOUDFLARE_REALTIME_BASE_URL")
                .unwrap_or_else(|_| {
                    warn!("CLOUDFLARE_REALTIME_BASE_URL not set, using default");
                    "https://rtc.live.cloudflare.com/v1".to_string()
                }),
        };
        
        if !config.is_configured() {
            warn!("Application not fully configured - missing environment variables");
        }
        
        config
    }
    
    pub fn is_configured(&self) -> bool {
        !self.supabase_url.is_empty() 
            && !self.supabase_anon_key.is_empty()
            && !self.supabase_jwt_secret.is_empty()
    }
    
    pub fn is_video_conferencing_configured(&self) -> bool {
        !self.cloudflare_realtime_app_id.is_empty()
            && !self.cloudflare_realtime_api_token.is_empty()
            && !self.cloudflare_realtime_base_url.is_empty()
    }
}