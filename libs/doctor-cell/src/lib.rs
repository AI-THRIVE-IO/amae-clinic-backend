pub mod handlers;
pub mod router;
pub mod models;
pub mod services;

// Re-export all models and services for external use
pub use models::*;
pub use services::*;

// Specifically re-export enhanced medical scheduling types
pub use models::{
    AppointmentType, SlotPriority, EnhancedDoctorAvailabilityResponse,
    MedicalSchedulingConfig, AvailableSlot, DoctorAvailability,
    CreateAvailabilityRequest, UpdateAvailabilityRequest,
};