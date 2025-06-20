#!/bin/bash

# Comprehensive endpoint testing script
BASE_URL="https://amae-clinic-backend.onrender.com"
TOKEN="eyJhbGciOiJIUzI1NiIsImtpZCI6ImQyTmZlVFRWUHJNTU9ZcjgiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2x2Y2ZkZWh4bXVreGlvYnN4Z3lhLnN1cGFiYXNlLmNvL2F1dGgvdjEiLCJzdWIiOiJhN2I4NTQ5Mi1iNjcyLTQzYWQtOTg5YS0xYWNlZjU3NGE5NDIiLCJhdWQiOiJhdXRoZW50aWNhdGVkIiwiZXhwIjoxNzUwNDAzOTA5LCJpYXQiOjE3NTA0MDAzMDksImVtYWlsIjoianBnYXZpcmlhQGFpLXRocml2ZS5pbyIsInBob25lIjoiIiwiYXBwX21ldGFkYXRhIjp7InByb3ZpZGVyIjoiZW1haWwiLCJwcm92aWRlcnMiOlsiZW1haWwiXX0sInVzZXJfbWV0YWRhdGEiOnsiZW1haWxfdmVyaWZpZWQiOnRydWV9LCJyb2xlIjoiYXV0aGVudGljYXRlZCIsImFhbCI6ImFhbDEiLCJhbXIiOlt7Im1ldGhvZCI6InBhc3N3b3JkIiwidGltZXN0YW1wIjoxNzUwNDAwMzA5fV0sInNlc3Npb25faWQiOiI0Zjg0ZjM3Yi0zZGFkLTRhZTktOWU5ZS02OTk2MjFlOTFlMjkiLCJpc19hbm9ueW1vdXMiOmZhbHNlfQ.M4p0rGLfp6lMTnqZywKHidydoYkHbquWdB2Y3nfSOkg"
PATIENT_UUID="a7b85492-b672-43ad-989a-1acef574a942"
DOCTOR_UUID="d5cfacac-cb98-46f0-bde0-41d8f6a2424c"

echo "🧪 Testing Critical Endpoints..."
echo "================================="

echo "✅ 1. Doctor Search"
curl -s -X GET "${BASE_URL}/doctors/search?specialty=cardiology" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" | jq -r '.doctors[0].first_name // "ERROR"'

echo ""
echo "🚫 2. Doctor Availability"
curl -s -X GET "${BASE_URL}/doctors/${DOCTOR_UUID}/availability?date=2025-01-20" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" | jq -r '.error // "SUCCESS"'

echo ""
echo "🚫 3. Smart Appointment Booking"
curl -s -X POST "${BASE_URL}/appointments/book/smart" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "patient_id": "'${PATIENT_UUID}'",
    "preferred_date": "2025-01-22", 
    "specialty_required": "cardiology",
    "appointment_type": "FollowUpConsultation",
    "duration_minutes": 30,
    "timezone": "UTC"
  }' | jq -r '.error // "SUCCESS"'

echo ""
echo "🚫 4. Health Profile"
curl -s -X GET "${BASE_URL}/health/health-profiles/${PATIENT_UUID}" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" | jq -r '.error // "SUCCESS"'

echo ""
echo "🚫 5. Create Health Profile"
curl -s -X POST "${BASE_URL}/health/health-profiles" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "patient_id": "'${PATIENT_UUID}'",
    "gender": "female",
    "date_of_birth": "1990-05-15",
    "height_cm": 165,
    "weight_kg": 60,
    "emergency_contact_name": "Jane Doe",
    "emergency_contact_phone": "+1234567890",
    "medical_history": ["Hypertension"],
    "current_medications": ["Metformin"],
    "allergies": ["Penicillin"],
    "is_pregnant": false,
    "is_breastfeeding": false,
    "reproductive_stage": "reproductive"
  }' | jq -r '.error // "SUCCESS"'

echo ""
echo "==============================="
echo "Test Complete"