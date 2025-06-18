-- Create Performance Indexes for Appointments Table
-- Run these after the scheduled_end_time column was successfully added

-- Primary time-based indexes
CREATE INDEX IF NOT EXISTS idx_appointments_scheduled_start_time ON appointments (scheduled_start_time);
CREATE INDEX IF NOT EXISTS idx_appointments_scheduled_end_time ON appointments (scheduled_end_time);

-- Doctor-focused indexes for availability queries
CREATE INDEX IF NOT EXISTS idx_appointments_doctor_scheduled ON appointments (doctor_id, scheduled_start_time);
CREATE INDEX IF NOT EXISTS idx_appointments_doctor_time_range ON appointments (doctor_id, scheduled_start_time, scheduled_end_time);

-- Patient-focused indexes
CREATE INDEX IF NOT EXISTS idx_appointments_patient_scheduled ON appointments (patient_id, scheduled_start_time);

-- Status-based indexes for search queries
CREATE INDEX IF NOT EXISTS idx_appointments_status_scheduled ON appointments (status, scheduled_start_time);
CREATE INDEX IF NOT EXISTS idx_appointments_type_scheduled ON appointments (appointment_type, scheduled_start_time);

-- Composite index for common search patterns
CREATE INDEX IF NOT EXISTS idx_appointments_search_composite ON appointments (patient_id, status, scheduled_start_time);

-- Index for upcoming appointments queries
CREATE INDEX IF NOT EXISTS idx_appointments_upcoming ON appointments (scheduled_start_time) 
WHERE status IN ('pending', 'confirmed', 'scheduled');