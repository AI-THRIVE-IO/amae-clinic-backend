-- =====================================================================================
-- FINAL ENUM MAPPING FIX
-- =====================================================================================

-- 1. CRITICAL: Map GeneralConsultation to InitialConsultation
UPDATE appointment_availabilities 
SET appointment_type = 'InitialConsultation',
    updated_at = NOW()
WHERE appointment_type = 'GeneralConsultation';

-- 2. Update any appointments with GeneralConsultation as well
UPDATE appointments 
SET appointment_type = 'InitialConsultation'
WHERE appointment_type = 'GeneralConsultation';

-- 3. Verification
SELECT 'Enum Fix Verification' as test_name,
       table_name,
       appointment_type,
       count(*) as count
FROM (
    SELECT 'appointments' as table_name, appointment_type FROM appointments
    UNION ALL
    SELECT 'appointment_availabilities' as table_name, appointment_type FROM appointment_availabilities
) combined
GROUP BY table_name, appointment_type
ORDER BY table_name, appointment_type;