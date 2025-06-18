-- =====================================================================================
-- REMAINING CRITICAL FIXES
-- =====================================================================================

-- 1. Fix appointment status enum - update sample data to use correct status
UPDATE appointments 
SET status = 'confirmed'
WHERE status = 'scheduled';

-- 2. Add appointment availability data so doctor availability endpoint works
INSERT INTO appointment_availabilities (
    id, doctor_id, day_of_week, duration_minutes, is_available,
    morning_start_time, morning_end_time, appointment_type, 
    buffer_minutes, max_concurrent_appointments, is_recurring,
    availability_status, exclude_holidays, notification_minutes_before
) VALUES 
(gen_random_uuid(), 'd5cfacac-cb98-46f0-bde0-41d8f6a2424c', 1, 30, true, 
 '2025-06-22 09:00:00+00', '2025-06-22 17:00:00+00', 'InitialConsultation',
 15, 1, true, 'active', true, 30),
(gen_random_uuid(), 'd5cfacac-cb98-46f0-bde0-41d8f6a2424c', 2, 30, true, 
 '2025-06-23 09:00:00+00', '2025-06-23 17:00:00+00', 'InitialConsultation',
 15, 1, true, 'active', true, 30),
(gen_random_uuid(), 'd5cfacac-cb98-46f0-bde0-41d8f6a2424c', 3, 30, true, 
 '2025-06-24 09:00:00+00', '2025-06-24 17:00:00+00', 'InitialConsultation',
 15, 1, true, 'active', true, 30)
ON CONFLICT DO NOTHING;

-- 3. Ensure patient exists in patients table (for patient info retrieval)
INSERT INTO patients (
    id, first_name, last_name, email, phone_number, date_of_birth, 
    birth_gender, address, eircode, created_at, updated_at
) VALUES (
    'a7b85492-b672-43ad-989a-1acef574a942'::uuid,
    'Juan Pablo',
    'Gaviria',
    'jpgaviria@ai-thrive.io',
    '+353-123-456-789',
    '1990-01-01',
    'male',
    'Dublin, Ireland',
    'D01 A123',
    NOW(),
    NOW()
) ON CONFLICT (id) DO UPDATE SET
    first_name = EXCLUDED.first_name,
    last_name = EXCLUDED.last_name,
    email = EXCLUDED.email,
    updated_at = NOW();

-- 4. Verification queries
SELECT 'Final Status Check' as test,
       'appointments' as table_name,
       status,
       count(*) as count
FROM appointments 
GROUP BY status
UNION ALL
SELECT 'Final Status Check' as test,
       'appointment_availabilities' as table_name,
       'availability_count' as status,
       count(*) as count
FROM appointment_availabilities
WHERE doctor_id = 'd5cfacac-cb98-46f0-bde0-41d8f6a2424c'
UNION ALL  
SELECT 'Final Status Check' as test,
       'patients' as table_name,
       'patient_exists' as status,
       count(*) as count
FROM patients
WHERE id = 'a7b85492-b672-43ad-989a-1acef574a942'::uuid;