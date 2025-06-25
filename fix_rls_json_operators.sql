-- =====================================================================================
-- FIX RLS JSON OPERATOR ERRORS
-- =====================================================================================
-- Eliminates "text ->> unknown operator" errors by using auth.uid() instead of JWT claims

-- =====================================================================================
-- 1. DROP PROBLEMATIC RLS POLICIES WITH JSON OPERATORS
-- =====================================================================================

-- Drop all policies that use problematic JSON operators
DROP POLICY IF EXISTS "Users can view own profile" ON profiles;
DROP POLICY IF EXISTS "Users can update own profile" ON profiles;
DROP POLICY IF EXISTS "Admins can do anything with doctors" ON doctors;
DROP POLICY IF EXISTS "Doctors can update own profile" ON doctors;
DROP POLICY IF EXISTS "Admin access to all profiles" ON profiles;
DROP POLICY IF EXISTS "Admins can access all health profiles" ON health_profiles;
DROP POLICY IF EXISTS "Admins can access all patients" ON patients;
DROP POLICY IF EXISTS "Patients can view own data" ON patients;
DROP POLICY IF EXISTS "Admins can access all appointments" ON appointments;

-- Drop appointment policies with JSON operators
DROP POLICY IF EXISTS "appointment_select_policy" ON appointments;
DROP POLICY IF EXISTS "appointment_insert_policy" ON appointments;
DROP POLICY IF EXISTS "appointment_update_policy" ON appointments;
DROP POLICY IF EXISTS "appointment_delete_policy" ON appointments;

-- =====================================================================================
-- 2. CREATE SAFE RLS POLICIES USING auth.uid() ONLY
-- =====================================================================================

-- PROFILES TABLE: Safe auth.uid() approach
CREATE POLICY "profiles_self_access_only" ON profiles
    FOR ALL USING (
        auth.uid() IS NOT NULL AND 
        auth.uid()::text = id::text
    );

-- PATIENTS TABLE: Self-access only (no admin table dependency)
CREATE POLICY "patients_secure_access" ON patients
    FOR ALL USING (
        auth.uid() IS NOT NULL AND
        auth.uid()::text = id::text
    );

-- DOCTORS TABLE: Public read, secure write
CREATE POLICY "doctors_public_read" ON doctors
    FOR SELECT USING (true);

CREATE POLICY "doctors_secure_write" ON doctors
    FOR INSERT WITH CHECK (
        auth.uid() IS NOT NULL AND
        auth.uid()::text = id::text
    );

CREATE POLICY "doctors_secure_update" ON doctors
    FOR UPDATE USING (
        auth.uid() IS NOT NULL AND
        auth.uid()::text = id::text
    );

-- APPOINTMENTS TABLE: Patient/Doctor access without JSON operators
CREATE POLICY "appointments_secure_access" ON appointments
    FOR ALL USING (
        auth.uid() IS NOT NULL AND (
            -- Patient can access their own appointments
            auth.uid()::text = patient_id::text OR
            -- Doctor can access their appointments
            auth.uid()::text = doctor_id::text
        )
    );

-- HEALTH PROFILES: Patient self-access only
CREATE POLICY "health_profiles_patient_only" ON health_profiles
    FOR ALL USING (
        auth.uid() IS NOT NULL AND
        auth.uid()::text = patient_id::text
    );

-- =====================================================================================
-- 3. VERIFICATION
-- =====================================================================================

-- Verify no JSON operators remain in policies
SELECT 'RLS POLICY VALIDATION' as validation_type,
       schemaname,
       tablename,
       policyname,
       cmd,
       CASE 
           WHEN qual LIKE '%->%' OR qual LIKE '%->>%' THEN 'CONTAINS_JSON_OPERATORS'
           ELSE 'SAFE'
       END as safety_status
FROM pg_policies 
WHERE schemaname = 'public'
ORDER BY tablename, policyname;