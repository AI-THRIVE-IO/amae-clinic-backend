-- Fix Appointments Table Schema - Non-Destructive Approach
-- Adds missing columns that the code expects without breaking existing data

-- Option 1: Add computed columns using immutable functions
ALTER TABLE appointments 
ADD COLUMN scheduled_start_time timestamp with time zone 
GENERATED ALWAYS AS (appointment_date) STORED;

-- For end time, we need to create an immutable function first
CREATE OR REPLACE FUNCTION calculate_appointment_end_time(
    start_time timestamp with time zone, 
    duration_minutes integer
) RETURNS timestamp with time zone
LANGUAGE sql IMMUTABLE
AS $$
    SELECT start_time + (duration_minutes || ' minutes')::interval;
$$;

-- Now add the computed end time column
ALTER TABLE appointments 
ADD COLUMN scheduled_end_time timestamp with time zone 
GENERATED ALWAYS AS (calculate_appointment_end_time(appointment_date, estimated_duration_minutes)) STORED;

-- Add performance indexes for time-based queries
CREATE INDEX idx_appointments_scheduled_start_time ON appointments (scheduled_start_time);
CREATE INDEX idx_appointments_scheduled_end_time ON appointments (scheduled_end_time);
CREATE INDEX idx_appointments_doctor_scheduled ON appointments (doctor_id, scheduled_start_time);
CREATE INDEX idx_appointments_patient_scheduled ON appointments (patient_id, scheduled_start_time);
CREATE INDEX idx_appointments_status_scheduled ON appointments (status, scheduled_start_time);

-- Add index for conflict detection queries (doctor + time range)
CREATE INDEX idx_appointments_doctor_time_range ON appointments (doctor_id, scheduled_start_time, scheduled_end_time);

-- Alternative Option 2: If you prefer simpler approach, just add regular columns
-- (Uncomment these and comment out the computed columns above)
-- ALTER TABLE appointments ADD COLUMN scheduled_start_time timestamp with time zone;
-- ALTER TABLE appointments ADD COLUMN scheduled_end_time timestamp with time zone;
-- 
-- -- Update existing data
-- UPDATE appointments SET 
--     scheduled_start_time = appointment_date,
--     scheduled_end_time = appointment_date + (estimated_duration_minutes || ' minutes')::interval;
-- 
-- -- Add NOT NULL constraints after data is populated
-- ALTER TABLE appointments ALTER COLUMN scheduled_start_time SET NOT NULL;
-- ALTER TABLE appointments ALTER COLUMN scheduled_end_time SET NOT NULL;