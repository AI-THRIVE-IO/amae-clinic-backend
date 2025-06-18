// =====================================================================================
// PASSWORD SECURITY SERVICE - SECURE PASSWORD HANDLING
// =====================================================================================

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use anyhow::Result;
use tracing::{debug, instrument};

use crate::models::{PasswordStrength, PasswordStrengthResult};

pub struct PasswordSecurityService;

impl PasswordSecurityService {
    pub fn new() -> Self {
        Self
    }

    #[instrument(skip(password))]
    pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        
        let password_hash = argon2.hash_password(password.as_bytes(), &salt)?;
        Ok(password_hash.to_string())
    }

    #[instrument(skip(password, hash))]
    pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
        let parsed_hash = PasswordHash::new(hash)?;
        let argon2 = Argon2::default();
        
        match argon2.verify_password(password.as_bytes(), &parsed_hash) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(e) => Err(e),
        }
    }

    #[instrument(skip(password))]
    pub fn validate_password_strength(password: &str) -> PasswordStrengthResult {
        let mut score = 0u8;
        let mut issues = Vec::new();

        // Length check
        if password.len() >= 12 {
            score += 25;
        } else if password.len() >= 8 {
            score += 15;
            issues.push("Password should be at least 12 characters long".to_string());
        } else {
            issues.push("Password must be at least 8 characters long".to_string());
        }

        // Character variety
        if password.chars().any(|c| c.is_lowercase()) {
            score += 15;
        } else {
            issues.push("Password should contain lowercase letters".to_string());
        }

        if password.chars().any(|c| c.is_uppercase()) {
            score += 15;
        } else {
            issues.push("Password should contain uppercase letters".to_string());
        }

        if password.chars().any(|c| c.is_numeric()) {
            score += 15;
        } else {
            issues.push("Password should contain numbers".to_string());
        }

        if password.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c)) {
            score += 15;
        } else {
            issues.push("Password should contain special characters".to_string());
        }

        // Entropy check - avoid simple patterns
        if Self::has_sequential_chars(password) {
            score = score.saturating_sub(20);
            issues.push("Avoid sequential characters (abc, 123)".to_string());
        }

        if Self::has_repeated_chars(password) {
            score = score.saturating_sub(15);
            issues.push("Avoid repeated characters (aaa, 111)".to_string());
        }

        // Common password check (simplified)
        let common_passwords = [
            "password", "123456", "password123", "admin", "qwerty",
            "letmein", "welcome", "monkey", "dragon", "123456789",
            "password1", "abc123", "111111", "123123", "admin123"
        ];
        
        if common_passwords.iter().any(|&common| password.to_lowercase().contains(common)) {
            score = score.saturating_sub(50);
            issues.push("Password contains common patterns".to_string());
        }

        // Medical context specific checks
        let medical_terms = [
            "doctor", "patient", "medical", "clinic", "hospital", 
            "health", "nurse", "medicine", "treatment"
        ];
        
        if medical_terms.iter().any(|&term| password.to_lowercase().contains(term)) {
            score = score.saturating_sub(10);
            issues.push("Avoid using medical terms in passwords".to_string());
        }

        let strength = match score {
            0..=25 => PasswordStrength::Weak,
            26..=50 => PasswordStrength::Fair,
            51..=75 => PasswordStrength::Good,
            76..=100 => PasswordStrength::Strong,
            _ => PasswordStrength::Strong,
        };

        PasswordStrengthResult {
            strength,
            score,
            issues,
        }
    }

    fn has_sequential_chars(password: &str) -> bool {
        let chars: Vec<char> = password.chars().collect();
        
        for window in chars.windows(3) {
            if let [a, b, c] = window {
                // Check for ascending sequence
                if (*b as u8).saturating_sub(*a as u8) == 1 && 
                   (*c as u8).saturating_sub(*b as u8) == 1 {
                    return true;
                }
                // Check for descending sequence
                if (*a as u8).saturating_sub(*b as u8) == 1 && 
                   (*b as u8).saturating_sub(*c as u8) == 1 {
                    return true;
                }
            }
        }
        
        false
    }

    fn has_repeated_chars(password: &str) -> bool {
        let chars: Vec<char> = password.chars().collect();
        
        for window in chars.windows(3) {
            if let [a, b, c] = window {
                if a == b && b == c {
                    return true;
                }
            }
        }
        
        false
    }

    pub fn generate_secure_password(length: usize) -> String {
        use rand::Rng;
        
        let lowercase = "abcdefghijklmnopqrstuvwxyz";
        let uppercase = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let numbers = "0123456789";
        let symbols = "!@#$%^&*()_+-=[]{}|;:,.<>?";
        
        let all_chars = format!("{}{}{}{}", lowercase, uppercase, numbers, symbols);
        let mut rng = rand::thread_rng();
        
        // Ensure at least one character from each category
        let mut password = String::new();
        password.push(lowercase.chars().nth(rng.gen_range(0..lowercase.len())).unwrap());
        password.push(uppercase.chars().nth(rng.gen_range(0..uppercase.len())).unwrap());
        password.push(numbers.chars().nth(rng.gen_range(0..numbers.len())).unwrap());
        password.push(symbols.chars().nth(rng.gen_range(0..symbols.len())).unwrap());
        
        // Fill the rest randomly
        for _ in 4..length {
            let idx = rng.gen_range(0..all_chars.len());
            password.push(all_chars.chars().nth(idx).unwrap());
        }
        
        // Shuffle the password
        let mut chars: Vec<char> = password.chars().collect();
        for i in (1..chars.len()).rev() {
            let j = rng.gen_range(0..=i);
            chars.swap(i, j);
        }
        
        chars.into_iter().collect()
    }

    pub fn check_password_breaches(password: &str) -> bool {
        // In production, this would check against known password breach databases
        // like HaveIBeenPwned API
        // For now, just check against a small list of known breached passwords
        let known_breached = [
            "123456", "password", "123456789", "12345678", "12345",
            "111111", "1234567", "sunshine", "qwerty", "iloveyou",
            "princess", "admin", "welcome", "666666", "abc123",
            "football", "123123", "monkey", "654321", "!@#$%^&*"
        ];
        
        known_breached.contains(&password)
    }

    pub fn get_password_recommendations() -> Vec<String> {
        vec![
            "Use at least 12 characters".to_string(),
            "Include uppercase and lowercase letters".to_string(),
            "Include numbers and special characters".to_string(),
            "Avoid common words and patterns".to_string(),
            "Don't reuse passwords across services".to_string(),
            "Consider using a passphrase with random words".to_string(),
            "Use a password manager".to_string(),
            "Enable two-factor authentication when available".to_string(),
        ]
    }

    pub fn validate_medical_professional_password(password: &str, user_role: &str) -> PasswordStrengthResult {
        let mut result = Self::validate_password_strength(password);
        
        // Medical professionals need stronger passwords
        if matches!(user_role, "doctor" | "admin" | "nurse") {
            // Require higher standards
            if result.score < 60 {
                result.issues.push("Medical professionals require stronger passwords (minimum score: 60)".to_string());
            }
            
            if password.len() < 14 {
                result.issues.push("Medical professionals should use passwords of at least 14 characters".to_string());
            }
        }
        
        result
    }
}