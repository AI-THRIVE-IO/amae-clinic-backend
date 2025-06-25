-- =====================================================================================
-- DROP PROBLEMATIC RLS POLICIES WITH JSON OPERATORS
-- =====================================================================================
-- Target the specific policies causing "text ->> unknown operator" errors

-- Drop the problematic _policy suffixed policies for doctors, patients, profiles
DROP POLICY IF EXISTS "doctor_insert_policy" ON doctors;
DROP POLICY IF EXISTS "doctor_select_policy" ON doctors;
DROP POLICY IF EXISTS "doctor_update_policy" ON doctors;
DROP POLICY IF EXISTS "doctor_delete_policy" ON doctors;

DROP POLICY IF EXISTS "patient_insert_policy" ON patients;
DROP POLICY IF EXISTS "patient_select_policy" ON patients;
DROP POLICY IF EXISTS "patient_update_policy" ON patients;
DROP POLICY IF EXISTS "patient_delete_policy" ON patients;

DROP POLICY IF EXISTS "profile_insert_policy" ON profiles;
DROP POLICY IF EXISTS "profile_select_policy" ON profiles;
DROP POLICY IF EXISTS "profile_update_policy" ON profiles;
DROP POLICY IF EXISTS "profile_delete_policy" ON profiles;

-- Also drop any appointment policies that might have JSON operators
DROP POLICY IF EXISTS "Users can create appointments" ON appointments;
DROP POLICY IF EXISTS "Users can view own appointments" ON appointments;

-- Verify these specific policies are gone
SELECT 'Verification - Should be empty' as check_type,
       schemaname, tablename, policyname
FROM pg_policies 
WHERE schemaname = 'public'
  AND policyname IN (
    'doctor_insert_policy', 'doctor_select_policy', 'doctor_update_policy', 'doctor_delete_policy',
    'patient_insert_policy', 'patient_select_policy', 'patient_update_policy', 'patient_delete_policy',
    'profile_insert_policy', 'profile_select_policy', 'profile_update_policy', 'profile_delete_policy',
    'Users can create appointments', 'Users can view own appointments'
  )
ORDER BY tablename, policyname;