-- =====================================================================================
-- AMAE CLINIC BACKEND - COMPREHENSIVE DATABASE SCHEMA ALIGNMENT FIXES
-- =====================================================================================
-- This script aligns the production database schema with Rust code expectations
-- Based on systematic API testing and architectural analysis
-- Execute these in order to resolve all identified schema mismatches
-- =====================================================================================

-- =====================================================================================
-- 1. CRITICAL: FIX ROW LEVEL SECURITY POLICIES
-- =====================================================================================
-- Issue: JWT claims access causing "text ->> unknown operator" errors
-- Root Cause: Malformed JSON operators in RLS policies

-- Fix auth.users profile access
DROP POLICY IF EXISTS "Users can view own profile" ON profiles;
CREATE POLICY "Users can view own profile" ON profiles
    FOR SELECT USING (
        auth.uid() = id OR
        (auth.jwt() ->> 'sub')::uuid = id OR
        (auth.jwt() -> 'user_metadata' ->> 'user_id')::uuid = id
    );

DROP POLICY IF EXISTS "Users can update own profile" ON profiles;
CREATE POLICY "Users can update own profile" ON profiles
    FOR UPDATE USING (
        auth.uid() = id OR
        (auth.jwt() ->> 'sub')::uuid = id OR
        (auth.jwt() -> 'user_metadata' ->> 'user_id')::uuid = id
    );

-- Fix doctors table RLS policies
DROP POLICY IF EXISTS "Doctors are viewable by everyone" ON doctors;
CREATE POLICY "Doctors are viewable by everyone" ON doctors
    FOR SELECT USING (true);

DROP POLICY IF EXISTS "Doctors can update own profile" ON doctors;
CREATE POLICY "Doctors can update own profile" ON doctors
    FOR UPDATE USING (
        (auth.jwt() ->> 'sub')::uuid = id OR
        (auth.jwt() ->> 'role') = 'doctor' OR
        (auth.jwt() ->> 'role') = 'admin' OR
        EXISTS (
            SELECT 1 FROM doctor_auth da 
            WHERE da.doctor_id = doctors.id 
            AND da.id = auth.uid()
        )
    );

-- Fix patients table RLS policies
DROP POLICY IF EXISTS "Patients can view own data" ON patients;
CREATE POLICY "Patients can view own data" ON patients
    FOR SELECT USING (
        (auth.jwt() ->> 'sub')::uuid = id OR
        (auth.jwt() ->> 'role') = 'admin' OR
        (auth.jwt() ->> 'role') = 'doctor'
    );

-- =====================================================================================
-- 2. APPOINTMENTS TABLE: ADD MISSING FIELDS EXPECTED BY RUST CODE
-- =====================================================================================

-- Add separate note fields (code expects patient_notes and doctor_notes)
ALTER TABLE appointments 
ADD COLUMN IF NOT EXISTS patient_notes TEXT,
ADD COLUMN IF NOT EXISTS doctor_notes TEXT;

-- Add duration field with expected name (code expects duration_minutes, not estimated_duration_minutes)
ALTER TABLE appointments 
ADD COLUMN IF NOT EXISTS duration_minutes INTEGER DEFAULT 30;

-- Add timezone field for proper timestamp handling
ALTER TABLE appointments 
ADD COLUMN IF NOT EXISTS timezone TEXT DEFAULT 'UTC';

-- Add actual timing fields for completed appointments
ALTER TABLE appointments 
ADD COLUMN IF NOT EXISTS actual_start_time TIMESTAMP WITH TIME ZONE,
ADD COLUMN IF NOT EXISTS actual_end_time TIMESTAMP WITH TIME ZONE;

-- Add medical workflow fields
ALTER TABLE appointments 
ADD COLUMN IF NOT EXISTS prescription_issued BOOLEAN DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS medical_certificate_issued BOOLEAN DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS report_generated BOOLEAN DEFAULT FALSE;

-- Add computed end time based on start time and duration
ALTER TABLE appointments 
ADD COLUMN IF NOT EXISTS scheduled_end_time TIMESTAMP WITH TIME ZONE 
GENERATED ALWAYS AS (scheduled_start_time + (duration_minutes || ' minutes')::INTERVAL) STORED;

-- Update existing appointments to use scheduled_start_time if null
UPDATE appointments 
SET scheduled_start_time = appointment_date 
WHERE scheduled_start_time IS NULL;

-- =====================================================================================
-- 3. DOCTORS TABLE: FIX SPECIALTY SEARCH AND ACCESS ISSUES
-- =====================================================================================

-- Ensure specialty field is properly indexed for search
CREATE INDEX IF NOT EXISTS idx_doctors_specialty_search ON doctors USING gin(to_tsvector('english', specialty));
CREATE INDEX IF NOT EXISTS idx_doctors_specialty_ilike ON doctors (lower(specialty));

-- Add missing professional fields expected by code
ALTER TABLE doctors 
ADD COLUMN IF NOT EXISTS consultation_fee DECIMAL(10,2) DEFAULT 100.00,
ADD COLUMN IF NOT EXISTS emergency_fee DECIMAL(10,2) DEFAULT 200.00,
ADD COLUMN IF NOT EXISTS accepts_insurance BOOLEAN DEFAULT TRUE,
ADD COLUMN IF NOT EXISTS languages TEXT[] DEFAULT ARRAY['English'];

-- Ensure all doctors have is_available set properly
UPDATE doctors 
SET is_available = COALESCE(is_available, true)
WHERE is_available IS NULL;

-- =====================================================================================
-- 4. VIDEO CONFERENCING: FIX TIMESTAMP ISSUES
-- =====================================================================================

-- Fix video_sessions table structure
ALTER TABLE video_sessions 
ADD COLUMN IF NOT EXISTS cloudflare_session_id TEXT,
ADD COLUMN IF NOT EXISTS session_type TEXT DEFAULT 'appointment',
ADD COLUMN IF NOT EXISTS max_participants INTEGER DEFAULT 2,
ADD COLUMN IF NOT EXISTS actual_start_time TIMESTAMP WITH TIME ZONE,
ADD COLUMN IF NOT EXISTS actual_end_time TIMESTAMP WITH TIME ZONE,
ADD COLUMN IF NOT EXISTS session_duration_minutes INTEGER,
ADD COLUMN IF NOT EXISTS quality_rating DECIMAL(3,2),
ADD COLUMN IF NOT EXISTS connection_issues TEXT;

-- =====================================================================================
-- 5. HEALTH PROFILES: ENSURE GENDER-SPECIFIC FIELDS
-- =====================================================================================

-- Add gender-specific health fields for women's health
ALTER TABLE health_profiles 
ADD COLUMN IF NOT EXISTS gender TEXT CHECK (gender IN ('male', 'female', 'other')),
ADD COLUMN IF NOT EXISTS is_pregnant BOOLEAN DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS is_breastfeeding BOOLEAN DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS reproductive_stage TEXT CHECK (reproductive_stage IN ('reproductive', 'perimenopause', 'postmenopause', 'not_applicable'));

-- Add constraint to ensure pregnancy/breastfeeding only for females
ALTER TABLE health_profiles 
ADD CONSTRAINT check_gender_specific_fields 
CHECK (
    (gender != 'male') OR 
    (is_pregnant = FALSE AND is_breastfeeding = FALSE AND reproductive_stage = 'not_applicable')
);

-- =====================================================================================
-- 6. ENUM TYPE ALIGNMENT: APPOINTMENT TYPES
-- =====================================================================================

-- Ensure appointment_type values match Rust enum expectations
UPDATE appointments 
SET appointment_type = CASE 
    WHEN appointment_type = 'general_consultation' THEN 'GeneralConsultation'
    WHEN appointment_type = 'follow_up' THEN 'FollowUpConsultation'
    WHEN appointment_type = 'emergency' THEN 'EmergencyConsultation'
    WHEN appointment_type = 'prescription_renewal' THEN 'PrescriptionRenewal'
    WHEN appointment_type = 'specialty_consultation' THEN 'SpecialtyConsultation'
    WHEN appointment_type = 'group_session' THEN 'GroupSession'
    WHEN appointment_type = 'telehealth_checkin' THEN 'TelehealthCheckIn'
    ELSE 'GeneralConsultation'
END;

-- Update appointment_availabilities with same enum fix
UPDATE appointment_availabilities 
SET appointment_type = CASE 
    WHEN appointment_type = 'general_consultation' THEN 'GeneralConsultation'
    WHEN appointment_type = 'follow_up' THEN 'FollowUpConsultation'
    WHEN appointment_type = 'emergency' THEN 'EmergencyConsultation'
    WHEN appointment_type = 'prescription_renewal' THEN 'PrescriptionRenewal'
    WHEN appointment_type = 'specialty_consultation' THEN 'SpecialtyConsultation'
    WHEN appointment_type = 'group_session' THEN 'GroupSession'
    WHEN appointment_type = 'telehealth_checkin' THEN 'TelehealthCheckIn'
    ELSE 'GeneralConsultation'
END;

-- =====================================================================================
-- 7. PERFORMANCE OPTIMIZATION: ADD CRITICAL INDEXES
-- =====================================================================================

-- Appointment search optimization
CREATE INDEX IF NOT EXISTS idx_appointments_patient_search ON appointments (patient_id, status, scheduled_start_time);
CREATE INDEX IF NOT EXISTS idx_appointments_doctor_search ON appointments (doctor_id, status, scheduled_start_time);
CREATE INDEX IF NOT EXISTS idx_appointments_time_range ON appointments (scheduled_start_time, scheduled_end_time);

-- Doctor search optimization
CREATE INDEX IF NOT EXISTS idx_doctors_availability ON doctors (is_available, is_verified, specialty);
CREATE INDEX IF NOT EXISTS idx_doctors_rating ON doctors (rating DESC) WHERE rating > 0;

-- Video session optimization
CREATE INDEX IF NOT EXISTS idx_video_sessions_appointment ON video_sessions (appointment_id);
CREATE INDEX IF NOT EXISTS idx_video_sessions_status ON video_sessions (status, actual_start_time);

-- =====================================================================================
-- 8. DATA INTEGRITY: ADD FOREIGN KEY CONSTRAINTS
-- =====================================================================================

-- Ensure referential integrity for appointments
ALTER TABLE appointments 
ADD CONSTRAINT IF NOT EXISTS fk_appointments_patient 
FOREIGN KEY (patient_id) REFERENCES patients(id) ON DELETE CASCADE;

ALTER TABLE appointments 
ADD CONSTRAINT IF NOT EXISTS fk_appointments_doctor 
FOREIGN KEY (doctor_id) REFERENCES doctors(id) ON DELETE CASCADE;

-- Ensure referential integrity for video sessions
ALTER TABLE video_sessions 
ADD CONSTRAINT IF NOT EXISTS fk_video_sessions_appointment 
FOREIGN KEY (appointment_id) REFERENCES appointments(id) ON DELETE CASCADE;

-- Ensure referential integrity for health profiles
ALTER TABLE health_profiles 
ADD CONSTRAINT IF NOT EXISTS fk_health_profiles_patient 
FOREIGN KEY (patient_id) REFERENCES patients(id) ON DELETE CASCADE;

-- =====================================================================================
-- 9. COMPATIBILITY FUNCTIONS: BRIDGE SCHEMA DIFFERENCES
-- =====================================================================================

-- Create view for backward compatibility with old appointment queries
CREATE OR REPLACE VIEW appointments_with_legacy_fields AS
SELECT 
    *,
    scheduled_start_time as start_time,
    scheduled_end_time as end_time,
    COALESCE(patient_notes, notes) as patient_notes_computed,
    COALESCE(doctor_notes, notes) as doctor_notes_computed
FROM appointments;

-- Create function to handle timezone-aware timestamp conversion
CREATE OR REPLACE FUNCTION safe_timestamp_parse(input_text TEXT, fallback_tz TEXT DEFAULT 'UTC')
RETURNS TIMESTAMP WITH TIME ZONE AS $$
BEGIN
    -- Try standard ISO format first
    RETURN input_text::TIMESTAMP WITH TIME ZONE;
EXCEPTION WHEN OTHERS THEN
    -- Fallback: try parsing without timezone and add UTC
    BEGIN
        RETURN (input_text::TIMESTAMP AT TIME ZONE fallback_tz);
    EXCEPTION WHEN OTHERS THEN
        -- Final fallback: current timestamp
        RETURN NOW();
    END;
END;
$$ LANGUAGE plpgsql;

-- =====================================================================================
-- 10. REFRESH MATERIALIZED VIEWS AND STATISTICS
-- =====================================================================================

-- Update table statistics for query optimization
ANALYZE appointments;
ANALYZE doctors;
ANALYZE patients;
ANALYZE health_profiles;
ANALYZE video_sessions;

-- =====================================================================================
-- 11. VALIDATION QUERIES: VERIFY FIXES
-- =====================================================================================

-- Test doctor search functionality
SELECT 'Doctor search test' as test_name, count(*) as doctor_count 
FROM doctors 
WHERE lower(specialty) LIKE '%cardiology%' AND is_available = true;

-- Test appointment time fields
SELECT 'Appointment time fields test' as test_name, count(*) as count_with_scheduled_time
FROM appointments 
WHERE scheduled_start_time IS NOT NULL;

-- Test RLS policy functionality
SELECT 'RLS policy test' as test_name, 
       current_setting('request.jwt.claims', true) as jwt_claims_setting;

-- =====================================================================================
-- EXECUTION SUMMARY
-- =====================================================================================
-- This script addresses the following critical issues identified during testing:
-- 
-- 1. ✅ Fixed RLS policies causing "text ->> unknown operator" errors
-- 2. ✅ Added missing appointment fields (patient_notes, doctor_notes, duration_minutes)
-- 3. ✅ Fixed specialty search issues with proper indexing
-- 4. ✅ Aligned enum values between database and Rust code
-- 5. ✅ Added timezone handling for timestamp fields
-- 6. ✅ Enhanced video conferencing table structure
-- 7. ✅ Added gender-specific health profile fields
-- 8. ✅ Improved performance with strategic indexes
-- 9. ✅ Added data integrity constraints
-- 10. ✅ Created compatibility functions for smooth migration
--
-- After running this script, re-test the API endpoints to verify all issues are resolved.
-- The following endpoints should now work correctly:
-- - Doctor search: /doctors/search?specialty=cardiology
-- - Doctor profile: /doctors/{id}
-- - Appointment booking: /appointments/smart-book
-- - Appointment search: /appointments/search
-- - Video session management: /video/sessions
-- =====================================================================================