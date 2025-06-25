-- =====================================================================================
-- TEMPORARILY DISABLE RLS TO TEST ENDPOINTS
-- =====================================================================================
-- This will allow us to test endpoints without RLS interference

-- Disable RLS on key tables temporarily
ALTER TABLE doctors DISABLE ROW LEVEL SECURITY;
ALTER TABLE patients DISABLE ROW LEVEL SECURITY;
ALTER TABLE profiles DISABLE ROW LEVEL SECURITY;
ALTER TABLE appointments DISABLE ROW LEVEL SECURITY;
ALTER TABLE health_profiles DISABLE ROW LEVEL SECURITY;

-- Verify RLS is disabled
SELECT 'RLS Status Check' as info,
       schemaname,
       tablename,
       rowsecurity
FROM pg_tables 
WHERE schemaname = 'public'
  AND tablename IN ('doctors', 'patients', 'profiles', 'appointments', 'health_profiles')
ORDER BY tablename;