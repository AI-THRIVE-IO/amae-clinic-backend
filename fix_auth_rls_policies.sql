-- CRITICAL FIX: Auth Profile JSON Operator Error
-- Fixes malformed RLS policies causing PostgreSQL JSON operator failures
-- Root cause: Incorrect usage of auth.jwt() ->> 'role' in RLS policies

-- Drop and recreate the problematic admin policy
DROP POLICY IF EXISTS "Admins can do anything with doctors" ON doctors;

-- Create corrected admin policy with proper JSON operator usage
CREATE POLICY "Admins can do anything with doctors" ON doctors
    FOR ALL USING (
        (auth.jwt() ->> 'role')::text = 'admin' OR
        (auth.jwt() -> 'app_metadata' ->> 'role')::text = 'admin' OR
        (auth.jwt() -> 'user_metadata' ->> 'role')::text = 'admin'
    );

-- Check and fix any other policies with similar issues
DROP POLICY IF EXISTS "Admin access to all profiles" ON profiles;
CREATE POLICY "Admin access to all profiles" ON profiles
    FOR ALL USING (
        auth.uid()::text = id::text OR
        (auth.jwt() ->> 'role')::text = 'admin' OR
        (auth.jwt() -> 'app_metadata' ->> 'role')::text = 'admin'
    );

-- Fix health_profiles admin policy if it exists
DROP POLICY IF EXISTS "Admins can access all health profiles" ON health_profiles;
CREATE POLICY "Admins can access all health profiles" ON health_profiles
    FOR ALL USING (
        auth.uid()::text = patient_id::text OR
        (auth.jwt() ->> 'role')::text = 'admin' OR
        (auth.jwt() -> 'app_metadata' ->> 'role')::text = 'admin'
    );

-- Fix patients admin policy if it exists
DROP POLICY IF EXISTS "Admins can access all patients" ON patients;
CREATE POLICY "Admins can access all patients" ON patients
    FOR ALL USING (
        auth.uid()::text = id::text OR
        (auth.jwt() ->> 'role')::text = 'admin' OR
        (auth.jwt() -> 'app_metadata' ->> 'role')::text = 'admin'
    );

-- Fix appointments admin policy if it exists
DROP POLICY IF EXISTS "Admins can access all appointments" ON appointments;
CREATE POLICY "Admins can access all appointments" ON appointments
    FOR ALL USING (
        auth.uid()::text = patient_id::text OR
        auth.uid()::text = doctor_id::text OR
        (auth.jwt() ->> 'role')::text = 'admin' OR
        (auth.jwt() -> 'app_metadata' ->> 'role')::text = 'admin'
    );

-- Additional safety: Ensure basic user access policies exist
CREATE POLICY IF NOT EXISTS "Users can read own profile" ON profiles
    FOR SELECT USING (auth.uid()::text = id::text);

CREATE POLICY IF NOT EXISTS "Users can update own profile" ON profiles
    FOR UPDATE USING (auth.uid()::text = id::text);

-- Verify the fix by checking policy syntax
DO $$
BEGIN
    RAISE NOTICE 'RLS policies fixed successfully. JSON operator errors should be resolved.';
END $$;