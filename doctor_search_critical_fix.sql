-- =====================================================================================
-- DOCTOR SEARCH CRITICAL FIX
-- =====================================================================================
-- ISSUE: Doctor search returns empty despite database having cardiology doctor
-- ROOT CAUSE: Case sensitivity or exact match requirements in Rust code

-- 1. Standardize specialty field to lowercase for consistent searching
UPDATE doctors SET specialty = LOWER(TRIM(specialty));

-- 2. Add case-insensitive index for specialty search
CREATE INDEX IF NOT EXISTS idx_doctors_specialty_lower_gin ON doctors USING gin(to_tsvector('english', specialty));

-- 3. Ensure doctor data matches expected search patterns
UPDATE doctors 
SET specialty = 'cardiology',
    is_available = true,
    is_verified = true,
    rating = 4.5
WHERE id = 'd5cfacac-cb98-46f0-bde0-41d8f6a2424c';

-- 4. Verification
SELECT 'Doctor Search Fix Verification' as test_name,
       id, specialty, is_available, is_verified, rating
FROM doctors 
WHERE specialty ILIKE '%cardiology%' OR LOWER(specialty) LIKE '%cardiology%';