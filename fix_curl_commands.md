# FIXED CURL COMMANDS - Enterprise Grade API Testing
# All commands corrected to match the actual API schema requirements

BASE_URL="https://amae-clinic-backend.onrender.com"
PATIENT_UUID="a7b85492-b672-43ad-989a-1acef574a942"
DOCTOR_UUID="d5cfacac-cb98-46f0-bde0-41d8f6a2424c"
APPOINTMENT_UUID="f1e2d3c4-b5a6-9798-8182-736455443322"
SESSION_UUID="e9f8g7h6-i5j4-k3l2-m1n0-o9p8q7r6s5t4"
YOUR_SUPABASE_JWT_TOKEN_HERE="eyJhbGciOiJIUzI1NiIsImtpZCI6ImQyTmZlVFRWUHJNTU9ZcjgiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2x2Y2ZkZWh4bXVreGlvYnN4Z3lhLnN1cGFiYXNlLmNvL2F1dGgvdjEiLCJzdWIiOiJhN2I4NTQ5Mi1iNjcyLTQzYWQtOTg5YS0xYWNlZjU3NGE5NDIiLCJhdWQiOiJhdXRoZW50aWNhdGVkIiwiZXhwIjoxNzUwNjM2NTEzLCJpYXQiOjE3NTA2MzI5MTMsImVtYWlsIjoianBnYXZpcmlhQGFpLXRocml2ZS5pbyIsInBob25lIjoiIiwiYXBwX21ldGFkYXRhIjp7InByb3ZpZGVyIjoiZW1haWwiLCJwcm92aWRlcnMiOlsiZW1haWwiXX0sInVzZXJfbWV0YWRhdGEiOnsiZW1haWxfdmVyaWZpZWQiOnRydWV9LCJyb2xlIjoiYXV0aGVudGljYXRlZCIsImFhbCI6ImFhbDEiLCJhbXIiOlt7Im1ldGhvZCI6InBhc3N3b3JkIiwidGltZXN0YW1wIjoxNzUwNjMyOTEzfV0sInNlc3Npb25faWQiOiJjM2RmNmRmOS1jNTAzLTRmMjMtYjY4YS0xNjQzNmFlMjRiZmQiLCJpc19hbm9ueW1vdXMiOmZhbHNlfQ.xGKXGksQsTRHBCqZTt8BBa3xB2hOXnBAU-nLFv7iIEE"

# ============================================================================
# HEALTH PROFILE - FIXED SCHEMA
# ============================================================================

echo "Creating Health Profile with fixed schema..."
curl -X POST "${BASE_URL}/health/health-profiles" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${YOUR_SUPABASE_JWT_TOKEN_HERE}" \
  -d '{
    "patient_id": "'${PATIENT_UUID}'",
    "gender": "male",
    "date_of_birth": "1990-05-15",
    "height_cm": 180,
    "weight_kg": 75.5,
    "emergency_contact_name": "Emergency Contact",
    "emergency_contact_phone": "+353-1-234-5678",
    "medical_history": ["No significant medical history"],
    "current_medications": ["None"],
    "allergies": ["None known"],
    "is_pregnant": false,
    "is_breastfeeding": false,
    "reproductive_stage": "unknown"
  }' -w "\nStatus: %{http_code}\n"

# ============================================================================
# DOCTOR CREATION - FIXED WITH date_of_birth
# ============================================================================

echo "Creating Doctor with required date_of_birth..."
curl -X POST "${BASE_URL}/doctors" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${YOUR_SUPABASE_JWT_TOKEN_HERE}" \
  -d '{
    "user_id": "c3d4e5f6-g7h8-i9j0-k1l2-m3n4o5p6q7r8",
    "first_name": "Dr. Sarah",
    "last_name": "Johnson",
    "email": "dr.sarah@example.com",
    "phone": "+1234567890",
    "specialty": "Cardiology",
    "sub_specialty": "Interventional Cardiology",
    "years_of_experience": 15,
    "license_number": "MD123456",
    "education": "Harvard Medical School",
    "certifications": ["Board Certified Cardiologist", "ACLS Certified"],
    "languages": ["English", "Spanish"],
    "bio": "Experienced cardiologist specializing in interventional procedures.",
    "consultation_fee": 200.00,
    "emergency_fee": 400.00,
    "is_available": true,
    "accepts_insurance": true,
    "date_of_birth": "1980-01-01"
  }' -w "\nStatus: %{http_code}\n"

# ============================================================================
# AVAILABILITY - FIXED WITH morning_start_time/morning_end_time
# ============================================================================

echo "Creating Availability with correct time fields..."
curl -X POST "${BASE_URL}/doctors/${DOCTOR_UUID}/availability" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${YOUR_SUPABASE_JWT_TOKEN_HERE}" \
  -d '{
    "day_of_week": 1,
    "duration_minutes": 30,
    "morning_start_time": "2025-06-23T09:00:00Z",
    "morning_end_time": "2025-06-23T12:00:00Z",
    "afternoon_start_time": "2025-06-23T14:00:00Z",
    "afternoon_end_time": "2025-06-23T17:00:00Z",
    "timezone": "Europe/Dublin",
    "max_concurrent_patients": 1,
    "appointment_type": "GeneralConsultation",
    "is_active": true
  }' -w "\nStatus: %{http_code}\n"

# ============================================================================
# APPOINTMENT BOOKING - SHOULD WORK AFTER FIXES
# ============================================================================

echo "Booking Appointment..."
curl -X POST "${BASE_URL}/appointments/" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${YOUR_SUPABASE_JWT_TOKEN_HERE}" \
  -d '{
    "patient_id": "'${PATIENT_UUID}'",
    "doctor_id": "'${DOCTOR_UUID}'",
    "start_time": "2025-06-23T10:00:00Z",
    "appointment_type": "GeneralConsultation",
    "duration_minutes": 30,
    "timezone": "Europe/Dublin",
    "patient_notes": "Annual checkup",
    "preferred_language": "English"
  }' -w "\nStatus: %{http_code}\n"