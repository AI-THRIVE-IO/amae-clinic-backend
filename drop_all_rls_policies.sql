-- =====================================================================================
-- NUCLEAR OPTION: DROP ALL RLS POLICIES WITH JSON OPERATORS
-- =====================================================================================
-- This will drop ALL existing RLS policies to eliminate JSON operator errors

-- First, let's see what policies exist
SELECT 'Current RLS Policies' as info, schemaname, tablename, policyname, cmd, qual
FROM pg_policies 
WHERE schemaname = 'public'
ORDER BY tablename, policyname;

-- Drop ALL existing RLS policies from common tables
-- APPOINTMENTS
DROP POLICY IF EXISTS "appointment_select_policy" ON appointments;
DROP POLICY IF EXISTS "appointment_insert_policy" ON appointments;
DROP POLICY IF EXISTS "appointment_update_policy" ON appointments;
DROP POLICY IF EXISTS "appointment_delete_policy" ON appointments;
DROP POLICY IF EXISTS "appointments_secure_access" ON appointments;

-- DOCTORS
DROP POLICY IF EXISTS "Admins can do anything with doctors" ON doctors;
DROP POLICY IF EXISTS "Doctors can update own profile" ON doctors;
DROP POLICY IF EXISTS "doctors_public_read" ON doctors;
DROP POLICY IF EXISTS "doctors_secure_write" ON doctors;
DROP POLICY IF EXISTS "doctors_secure_update" ON doctors;

-- PATIENTS
DROP POLICY IF EXISTS "Admins can access all patients" ON patients;
DROP POLICY IF EXISTS "Patients can view own data" ON patients;
DROP POLICY IF EXISTS "patients_secure_access" ON patients;

-- PROFILES
DROP POLICY IF EXISTS "Users can view own profile" ON profiles;
DROP POLICY IF EXISTS "Users can update own profile" ON profiles;
DROP POLICY IF EXISTS "Admin access to all profiles" ON profiles;
DROP POLICY IF EXISTS "profiles_self_access_only" ON profiles;

-- HEALTH PROFILES
DROP POLICY IF EXISTS "Admins can access all health profiles" ON health_profiles;
DROP POLICY IF EXISTS "health_profiles_patient_only" ON health_profiles;

-- VIDEO SESSIONS
DROP POLICY IF EXISTS "video_sessions_policy" ON video_sessions;

-- VIDEO ROOMS
DROP POLICY IF EXISTS "video_rooms_policy" ON video_rooms;

-- Now let's verify all policies are dropped
SELECT 'Remaining RLS Policies' as info, schemaname, tablename, policyname, cmd
FROM pg_policies 
WHERE schemaname = 'public'
ORDER BY tablename, policyname;