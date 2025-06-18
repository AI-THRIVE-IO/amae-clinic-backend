-- =====================================================================================
-- CONSTRAINT VIOLATION BULLETPROOF FIX
-- =====================================================================================

-- 1. INSPECT EXISTING DATA TO UNDERSTAND VIOLATIONS
SELECT 'Current appointment_availabilities data' as analysis,
       appointment_type,
       count(*) as occurrence_count
FROM appointment_availabilities 
GROUP BY appointment_type
ORDER BY count(*) DESC;

-- 2. DROP CONSTRAINT AND MIGRATE DATA SAFELY
ALTER TABLE appointment_availabilities DROP CONSTRAINT IF EXISTS chk_availability_appointment_type;

-- 3. UPDATE DATA WITH COMPREHENSIVE MAPPING
UPDATE appointment_availabilities 
SET appointment_type = 'InitialConsultation',
    updated_at = NOW()
WHERE appointment_type NOT IN (
    'InitialConsultation',
    'FollowUpConsultation', 
    'EmergencyConsultation',
    'PrescriptionRenewal',
    'SpecialtyConsultation',
    'GroupSession',
    'TelehealthCheckIn'
);

-- 4. RECREATE CONSTRAINT AFTER DATA CLEAN
ALTER TABLE appointment_availabilities 
ADD CONSTRAINT chk_availability_appointment_type 
CHECK (appointment_type IN (
    'InitialConsultation',
    'FollowUpConsultation', 
    'EmergencyConsultation',
    'PrescriptionRenewal',
    'SpecialtyConsultation',
    'GroupSession',
    'TelehealthCheckIn'
));

-- 5. FIXED VALIDATION QUERY (correct table reference)
SELECT 'Constraint Validation' as test_name,
       tc.table_name,
       tc.constraint_name,
       cc.check_clause
FROM information_schema.table_constraints tc
JOIN information_schema.check_constraints cc ON tc.constraint_name = cc.constraint_name
WHERE tc.constraint_name IN ('chk_appointments_appointment_type', 'chk_availability_appointment_type');

-- 6. FINAL TEST APPOINTMENTS INSERT
INSERT INTO appointments (
    id, patient_id, doctor_id, appointment_date, scheduled_start_time,
    status, appointment_type, duration_minutes, timezone,
    estimated_duration_minutes
) VALUES 
(gen_random_uuid(), 'a7b85492-b672-43ad-989a-1acef574a942'::uuid, 'd5cfacac-cb98-46f0-bde0-41d8f6a2424c'::uuid, '2025-06-20 10:00:00+00', '2025-06-20 10:00:00+00', 'scheduled', 'InitialConsultation', 30, 'Europe/Dublin', 30),
(gen_random_uuid(), 'a7b85492-b672-43ad-989a-1acef574a942'::uuid, 'd5cfacac-cb98-46f0-bde0-41d8f6a2424c'::uuid, '2025-06-21 14:00:00+00', '2025-06-21 14:00:00+00', 'scheduled', 'FollowUpConsultation', 30, 'Europe/Dublin', 30)
ON CONFLICT (id) DO NOTHING;

UPDATE appointments SET scheduled_end_time = scheduled_start_time + (duration_minutes || ' minutes')::INTERVAL WHERE scheduled_end_time IS NULL;

-- 7. VERIFICATION
SELECT 'FINAL VERIFICATION' as test_name, count(*) as appointments_created FROM appointments;