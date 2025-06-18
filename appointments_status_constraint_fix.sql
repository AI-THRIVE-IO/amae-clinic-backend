-- =====================================================================================
-- APPOINTMENTS STATUS CONSTRAINT FIX
-- =====================================================================================

-- 1. Check what status values are allowed by the constraint
SELECT constraint_name, check_clause 
FROM information_schema.check_constraints 
WHERE constraint_name = 'appointments_status_check';

-- 2. Drop the constraint temporarily to update data
ALTER TABLE appointments DROP CONSTRAINT IF EXISTS appointments_status_check;

-- 3. Update appointments to use a valid status from the enum
UPDATE appointments 
SET status = 'pending'  -- Using 'pending' as it was in the enum list from earlier error
WHERE status = 'scheduled';

-- 4. Recreate constraint with all valid status values
ALTER TABLE appointments 
ADD CONSTRAINT appointments_status_check 
CHECK (status IN (
    'pending',
    'confirmed', 
    'in_progress',
    'completed',
    'cancelled',
    'no_show',
    'rescheduled'
));

-- 5. Final verification
SELECT 'Status Fix Verification' as test,
       status,
       count(*) as count
FROM appointments 
GROUP BY status;