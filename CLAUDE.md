# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Build and Run
```bash
# Build the main API
npx nx build amae-clinic-api

# Run the main API server
npx nx run amae-clinic-api

# Build specific library/cell
npx nx build [cell-name]  # e.g., appointment-cell, doctor-cell

# Standard Rust commands also work
cargo build
cargo run --bin amae-clinic-api
cargo test
```

### Testing and Quality
```bash
# Test specific project
npx nx test [project-name]

# Lint with Clippy
npx nx lint [project-name]

# Check without building
npx nx check [project-name]

# Test all projects
cargo test
```

### Development Utilities
```bash
# Visual project dependency graph
npx nx graph

# List all projects
npx nx show projects

# Show available targets for a project
npx nx show project [project-name]
```

## Architecture Overview

### Cell-Based Microservices Pattern
This medical clinic backend uses a **"cell architecture"** where each major feature area is organized as an independent module:

- **auth-cell** - Authentication and authorization
- **health-profile-cell** - Patient health profiles and AI analysis
- **doctor-cell** - Doctor management, availability, and patient matching
- **appointment-cell** - Appointment booking with conflict detection and smart booking
- **patient-cell** - Patient management and registration
- **booking-queue-cell** - Async queue management for booking operations with Redis
- **video-conferencing-cell** - Video session management with Cloudflare integration
- **security-cell** - Security monitoring, audit logging, and password validation
- **monitoring-cell** - Health checks, metrics collection, and alerting
- **performance-cell** - Caching and performance optimization

### Cell Structure
Each cell follows this pattern:
```
libs/[cell-name]/
├── src/
│   ├── lib.rs          # Module exports
│   ├── models.rs       # Data structures & DTOs
│   ├── handlers.rs     # HTTP request handlers
│   ├── router.rs       # Route definitions
│   └── services/       # Business logic layer
```

### Shared Libraries
- **shared-config** - `AppConfig` with environment variables and Supabase settings
- **shared-database** - `SupabaseClient` for REST API interactions
- **shared-models** - `User`, `JwtClaims`, `AppError` common types
- **shared-utils** - JWT validation and authentication middleware

## Database & Authentication

### Supabase Integration
- **No ORM** - Direct HTTP calls to Supabase REST API via `SupabaseClient`
- **JWT Authentication** - Custom HMAC-SHA256 token validation
- **Row Level Security** - Database-level authorization through JWT claims
- **File Storage** - Document and image uploads handled by Supabase Storage

### Redis Integration
- **Queue Management** - Redis-backed async job queues with deadpool connection pooling
- **Caching** - Performance optimization through Redis caching layer
- **Real-time Updates** - WebSocket support for live booking updates

### Authentication Flow
1. JWT tokens validated using custom HMAC-SHA256 verification
2. User claims extracted and added to request extensions
3. Routes protected with auth middleware: `auth_middleware`
4. Public routes bypass authentication

## Key Patterns

### Route Protection
```rust
// Public routes (no auth)
let public_routes = Router::new().route("/search", get(handlers::search));

// Protected routes (auth required)
let protected_routes = Router::new()
    .route("/profile", post(handlers::get_profile))
    .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));
```

### Error Handling
- Use `AppError` enum which auto-converts to appropriate HTTP responses
- Log errors with structured logging (tracing)
- Return user-friendly error messages

### State Management
- `Arc<AppConfig>` shared across all handlers
- Database connections created per-request through `SupabaseClient`
- Configuration loaded from environment variables

## API Gateway Structure

The main API (`apps/api`) aggregates all cell routers:
- `/auth` → auth-cell
- `/health` → health-profile-cell
- `/doctors` → doctor-cell
- `/appointments` → appointment-cell
- `/patients` → patient-cell
- `/queue` → booking-queue-cell
- `/video` → video-conferencing-cell
- `/security` → security-cell
- `/monitoring` → monitoring-cell
- `/performance` → performance-cell

## Development Notes

### Adding New Cells
1. Create new library in `libs/[cell-name]/` following the established pattern
2. Add to workspace `Cargo.toml` dependencies
3. Register router in `apps/api/src/router.rs`
4. Follow the handlers → services → models layer separation

### Medical Domain Logic
- **Doctor Matching** - Considers specialty and patient consultation history
- **Appointment Booking** - Smart booking with conflict detection and priority scoring
- **Health Profiles** - AI-powered document analysis and avatar generation
- **Authentication** - Medical-grade security with proper user isolation

### Important Files
- `apps/api/src/main.rs` - Server startup and configuration
- `apps/api/src/router.rs` - Main API gateway routing
- `shared/database/src/supabase.rs` - Database client implementation
- `shared/utils/src/jwt.rs` - JWT validation logic

## Testing Infrastructure

### Comprehensive Test Suite
Each cell has a complete test suite covering:
- **Handler Tests** - HTTP endpoint testing with mocked external dependencies
- **Service Tests** - Business logic unit tests
- **Integration Tests** - End-to-end request/response testing
- **Edge Cases** - Error handling, authorization, and boundary conditions

### Test Structure
```
libs/[cell-name]/tests/
├── handlers_test.rs      # HTTP handler tests
├── integration_test.rs   # End-to-end integration tests
└── services/            # Service layer unit tests
    ├── [service]_test.rs
    └── mod.rs
```

### Test Dependencies
- **wiremock** - HTTP service mocking for external API calls
- **tokio-test** - Async testing utilities
- **assert_matches** - Pattern matching assertions
- **tempfile** - Temporary file handling for tests

### Test Utilities (`shared-utils::test_utils`)
- `TestConfig` - Test-specific configuration helpers
- `TestUser` - User model creation for different roles (patient, doctor, admin)
- `JwtTestUtils` - JWT token creation and validation for testing
- `MockSupabaseResponses` - Predefined mock responses for Supabase API calls

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for specific cell
cargo test -p auth-cell
cargo test -p doctor-cell
cargo test -p appointment-cell
cargo test -p health-profile-cell
cargo test -p patient-cell
cargo test -p booking-queue-cell
cargo test -p video-conferencing-cell
cargo test -p security-cell
cargo test -p monitoring-cell
cargo test -p performance-cell

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_validate_token_success

# Run tests in parallel
cargo test -j 4
```

### Test Coverage Areas

#### Auth Cell Tests
- JWT token validation and extraction
- Authorization header parsing
- User profile retrieval with Supabase integration
- Error handling for expired/invalid tokens
- Different user roles (patient, doctor, admin)

#### Doctor Cell Tests
- Doctor profile CRUD operations
- Availability management and scheduling
- Doctor search and filtering
- Specialty management
- Doctor matching algorithms
- Authorization checks (doctor self-access, admin access)

#### Appointment Cell Tests
- Appointment booking with conflict detection
- Smart booking with doctor matching
- Appointment lifecycle (schedule, reschedule, cancel)
- Authorization checks (patient/doctor access)
- Search and filtering capabilities

#### Health Profile Cell Tests
- Health profile management
- Document upload and processing
- AI document analysis integration
- Avatar generation
- Authorization and privacy protection

#### Patient Cell Tests
- Patient registration and profile management
- Patient search and filtering
- Authorization checks for patient data access

#### Booking Queue Cell Tests
- Redis queue operations (producer/consumer)
- WebSocket real-time updates
- Async job processing and retry mechanisms
- Queue worker lifecycle management

#### Video Conferencing Cell Tests
- Cloudflare video session integration
- Session lifecycle management
- Video room creation and management
- Video track and stream handling

#### Security Cell Tests
- Security audit logging
- Password validation and strength checking
- Threat monitoring and detection
- Security event tracking

#### Monitoring Cell Tests
- Health check endpoints
- Metrics collection and reporting
- Alert system functionality
- System status monitoring

#### Performance Cell Tests
- Redis caching operations
- Cache invalidation strategies
- Performance optimization utilities

### Mock Strategy
Tests use **wiremock** to mock external dependencies:
- Supabase REST API calls
- Redis operations and queue management
- Cloudflare video service integration
- File storage operations
- AI service integrations
- Database operations

This ensures tests are:
- **Fast** - No external network calls
- **Reliable** - No dependency on external services
- **Isolated** - Each test is independent
- **Comprehensive** - Can test error scenarios easily