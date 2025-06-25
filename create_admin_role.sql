-- =====================================================================================
-- ADMIN ROLE CREATION AND CONFIGURATION
-- =====================================================================================
-- Creates the PostgreSQL admin role with proper permissions for Supabase

-- Create the admin role if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'admin') THEN
        CREATE ROLE admin NOINHERIT;
    END IF;
END
$$;

-- Grant essential permissions to the admin role
GRANT USAGE ON SCHEMA public TO admin;
GRANT USAGE ON SCHEMA auth TO admin;

-- Allow admin role to be assumed by the authenticated role
GRANT admin TO authenticated;

-- Allow admin role to be assumed by the service_role  
GRANT admin TO service_role;

-- Grant permission to set role to admin
GRANT admin TO postgres;

-- Verify the role was created successfully
SELECT 'Admin role configuration complete' as status,
       rolname, 
       rolsuper, 
       rolinherit, 
       rolcreaterole, 
       rolcreatedb
FROM pg_roles 
WHERE rolname = 'admin';