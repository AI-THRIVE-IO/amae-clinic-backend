use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info};

use shared_database::supabase::SupabaseClient;

use crate::models::{AppointmentType, AppointmentPricing, AppointmentError};

pub struct PricingService {
    supabase: Arc<SupabaseClient>,
    pricing_rules: Vec<AppointmentPricing>,
}

impl PricingService {
    pub fn new(supabase: Arc<SupabaseClient>) -> Self {
        Self {
            supabase,
            pricing_rules: AppointmentPricing::get_standard_pricing(),
        }
    }

    /// Calculate the consultation fee for an appointment
    pub async fn calculate_price(
        &self,
        appointment_type: &AppointmentType,
        duration_minutes: i32,
    ) -> Result<f64, AppointmentError> {
        debug!("Calculating price for appointment type: {:?}, duration: {} minutes", 
               appointment_type, duration_minutes);

        // Find pricing rule for appointment type
        let pricing_rule = self.pricing_rules.iter()
            .find(|rule| rule.appointment_type == *appointment_type)
            .ok_or_else(|| AppointmentError::ValidationError(
                format!("No pricing rule found for appointment type: {:?}", appointment_type)
            ))?;

        let base_price = pricing_rule.get_effective_price();
        
        // Apply duration adjustments for certain appointment types
        let adjusted_price = match appointment_type {
            AppointmentType::GeneralConsultation |
            AppointmentType::MentalHealth |
            AppointmentType::WomensHealth => {
                self.apply_duration_pricing(base_price, duration_minutes)
            },
            _ => base_price, // Fixed pricing for other types
        };

        info!("Calculated price: €{:.2} for {:?} appointment ({} minutes)", 
              adjusted_price, appointment_type, duration_minutes);

        Ok(adjusted_price)
    }

    /// Get pricing information for all appointment types
    pub fn get_pricing_info(&self) -> &[AppointmentPricing] {
        &self.pricing_rules
    }

    /// Get pricing for a specific appointment type
    pub fn get_pricing_for_type(&self, appointment_type: &AppointmentType) -> Option<&AppointmentPricing> {
        self.pricing_rules.iter()
            .find(|rule| rule.appointment_type == *appointment_type)
    }

    /// Check if promotional pricing is available
    pub fn has_promotional_pricing(&self, appointment_type: &AppointmentType) -> bool {
        self.get_pricing_for_type(appointment_type)
            .map(|rule| rule.promotional_price.is_some())
            .unwrap_or(false)
    }

    /// Get what's included in the consultation price
    pub fn get_inclusions(&self, appointment_type: &AppointmentType) -> Option<Vec<String>> {
        self.get_pricing_for_type(appointment_type).map(|rule| {
            let mut inclusions = Vec::new();
            
            if rule.includes_prescription {
                inclusions.push("Immediate prescription".to_string());
            }
            if rule.includes_medical_certificate {
                inclusions.push("Medical certificate".to_string());
            }
            if rule.includes_report {
                inclusions.push("Consultation report".to_string());
            }
            
            inclusions
        })
    }

    /// Calculate potential savings with promotional pricing
    pub fn calculate_savings(&self, appointment_type: &AppointmentType) -> Option<f64> {
        self.get_pricing_for_type(appointment_type).and_then(|rule| {
            rule.promotional_price.map(|promo_price| {
                rule.base_price - promo_price
            })
        })
    }

    // ==============================================================================
    // PRIVATE HELPER METHODS
    // ==============================================================================

    /// Apply duration-based pricing adjustments
    fn apply_duration_pricing(&self, base_price: f64, duration_minutes: i32) -> f64 {
        match duration_minutes {
            // Standard consultation (15-30 minutes)
            15..=30 => base_price,
            
            // Extended consultation (31-45 minutes) - 25% surcharge
            31..=45 => base_price * 1.25,
            
            // Long consultation (46-60 minutes) - 50% surcharge
            46..=60 => base_price * 1.50,
            
            // Very long consultation (60+ minutes) - 75% surcharge
            61.. => base_price * 1.75,
            
            // Short consultation (under 15 minutes) - 25% discount
            _ => base_price * 0.75,
        }
    }

    /// Apply discount based on patient history (future implementation)
    #[allow(dead_code)]
    fn apply_loyalty_discount(&self, base_price: f64, _patient_consultation_count: i32) -> f64 {
        // TODO: Implement loyalty discount logic
        // Example: 5% discount after 5 consultations, 10% after 10, etc.
        base_price
    }

    /// Apply seasonal or promotional discounts (future implementation)
    #[allow(dead_code)]
    fn apply_promotional_discount(&self, base_price: f64, _promo_code: Option<&str>) -> f64 {
        // TODO: Implement promotional code logic
        base_price
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_config::AppConfig;

    fn create_test_pricing_service() -> PricingService {
        let config = AppConfig {
            supabase_url: "test".to_string(),
            supabase_anon_key: "test".to_string(),
            supabase_jwt_secret: "test".to_string(),
        };
        let supabase = SupabaseClient::new(&config);
        PricingService::new(Arc::new(supabase))
    }

    #[tokio::test]
    async fn test_general_consultation_pricing() {
        let service = create_test_pricing_service();
        
        let price = service.calculate_price(&AppointmentType::GeneralConsultation, 30).await.unwrap();
        assert_eq!(price, 29.0); // Promotional price
    }

    #[tokio::test]
    async fn test_duration_pricing_adjustment() {
        let service = create_test_pricing_service();
        
        // Standard duration
        let standard_price = service.calculate_price(&AppointmentType::GeneralConsultation, 30).await.unwrap();
        
        // Extended duration should cost more
        let extended_price = service.calculate_price(&AppointmentType::GeneralConsultation, 45).await.unwrap();
        
        assert!(extended_price > standard_price);
    }

    #[tokio::test]
    async fn test_fixed_pricing_types() {
        let service = create_test_pricing_service();
        
        // Prescription pricing should be fixed regardless of duration
        let price_15min = service.calculate_price(&AppointmentType::Prescription, 15).await.unwrap();
        let price_30min = service.calculate_price(&AppointmentType::Prescription, 30).await.unwrap();
        
        assert_eq!(price_15min, price_30min);
    }

    #[test]
    fn test_promotional_pricing_check() {
        let service = create_test_pricing_service();
        
        assert!(service.has_promotional_pricing(&AppointmentType::GeneralConsultation));
        assert!(!service.has_promotional_pricing(&AppointmentType::Prescription));
    }

    #[test]
    fn test_inclusions() {
        let service = create_test_pricing_service();
        
        let inclusions = service.get_inclusions(&AppointmentType::GeneralConsultation).unwrap();
        assert!(inclusions.contains(&"Immediate prescription".to_string()));
        assert!(inclusions.contains(&"Medical certificate".to_string()));
        assert!(inclusions.contains(&"Consultation report".to_string()));
    }

    #[test]
    fn test_savings_calculation() {
        let service = create_test_pricing_service();
        
        let savings = service.calculate_savings(&AppointmentType::GeneralConsultation).unwrap();
        assert_eq!(savings, 10.0); // 39.0 - 29.0 = 10.0
    }
}