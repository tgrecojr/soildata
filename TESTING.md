# Testing Guide

This document describes the testing strategy and how to run tests for the USCRN Data Ingestion Service.

## Test Coverage

The project includes comprehensive testing at multiple levels:

### 1. Unit Tests (Inline)
Location: Within source files (`src/**/*.rs`)

- **Config Module** (`src/config.rs`):
  - Environment variable expansion
  - Port deserialization (number and string)
  - Location filtering (state, station, pattern)
  - Configuration validation

- **Parser Module** (`src/parser.rs`):
  - DateTime parsing
  - Optional float/int parsing with missing values
  - Full line parsing
  - Parse failure threshold enforcement

- **Fetcher Module** (`src/fetcher.rs`):
  - Filename parsing
  - URL construction

### 2. Integration Tests
Location: `tests/` directory

#### Fetcher Integration Tests (`tests/fetcher_integration_test.rs`)
- URL validation against allowed hosts (SSRF protection)
- HTTPS enforcement
- Location filter matching (state, pattern, station)
- Empty filter behavior

#### Database Integration Tests (`tests/database_integration_test.rs`)
- Station upsert (insert and update)
- Batch station upsert
- Observation insertion
- Observation upsert/deduplication
- Large batch inserts (2000+ records)
- Processed file tracking
- Year-based file retrieval

#### Parser Integration Tests (`tests/parser_integration_test.rs`)
- Complete parse → database flow
- Missing value handling (-9999 → NULL)
- High failure rate rejection
- Empty file handling
- Custom failure thresholds
- Observation deduplication on re-import

## Running Tests

### ⚡ Quick Local Testing (No Database Required)

**Run this for fast local testing without PostgreSQL setup:**

```bash
./test-local.sh
```

Or manually:
```bash
# Unit tests (15 tests)
cargo test --lib

# Fetcher integration tests (8 tests)
cargo test --test fetcher_integration_test

# Parser tests that don't need database (4 tests)
cargo test --test parser_integration_test -- --skip parse_and_insert --skip missing_values --skip reimport
```

**Total: ~27 tests run without any database setup** ✅

---

### 🗄️ Full Test Suite (Requires PostgreSQL)

**Setup database first:**

```bash
# Option 1: Use existing docker-compose
docker-compose up -d db
export DATABASE_URL=postgres://uscrn:uscrn_password@localhost:5432/uscrn
sqlx migrate run

# Option 2: Quick test database
docker run -d --name postgres-test \
  -e POSTGRES_USER=test_user \
  -e POSTGRES_PASSWORD=test_password \
  -e POSTGRES_DB=test_db \
  -p 5432:5432 \
  postgres:18-alpine

export DATABASE_URL=postgres://test_user:test_password@localhost:5432/test_db
sqlx migrate run
```

**Then run all tests:**

```bash
cargo test --verbose
```

This runs **all 42 tests** including database integration tests.

---

### Run Specific Test Suites

```bash
# Unit tests only (no database)
cargo test --lib

# Fetcher tests only (no database)
cargo test --test fetcher_integration_test

# Database integration tests only (requires PostgreSQL)
cargo test --test database_integration_test

# Parser integration tests (requires PostgreSQL)
cargo test --test parser_integration_test
```

### Run Specific Test File
```bash
cargo test --test database_integration_test
cargo test --test fetcher_integration_test
cargo test --test parser_integration_test
```

### Run Specific Test
```bash
cargo test test_upsert_new_station
```

### Run Tests with Output
```bash
cargo test -- --nocapture
```

## Database Integration Tests

Integration tests use the `sqlx::test` macro which:
- Automatically creates a fresh database for each test
- Runs migrations before the test
- Rolls back changes after the test completes
- Provides isolation between tests

### Prerequisites

1. **Install SQLx CLI**:
   ```bash
   cargo install sqlx-cli --no-default-features --features postgres
   ```

2. **Set DATABASE_URL** (for sqlx-cli):
   ```bash
   export DATABASE_URL=postgres://test_user:test_password@localhost:5432/test_db
   ```

3. **Start PostgreSQL** (if running tests locally):
   ```bash
   docker run -d \
     --name postgres-test \
     -e POSTGRES_USER=test_user \
     -e POSTGRES_PASSWORD=test_password \
     -e POSTGRES_DB=test_db \
     -p 5432:5432 \
     postgres:18-alpine
   ```

4. **Run Migrations**:
   ```bash
   sqlx migrate run
   ```

### CI/CD Testing

GitHub Actions automatically:
1. Starts PostgreSQL service container
2. Runs migrations
3. Executes all tests
4. Only builds Docker image if tests pass

See `.github/workflows/ci.yml` for the complete CI pipeline.

## Test Quality Gates

### Code Quality Checks
- **Formatting**: `cargo fmt --check`
- **Linting**: `cargo clippy -- -D warnings`
- **Security**: `cargo audit`

### Coverage Goals
- Unit tests: All public functions
- Integration tests: All critical user flows
- Database tests: All repository operations

## Writing New Tests

### Unit Test Example
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        let result = my_function();
        assert_eq!(result, expected_value);
    }
}
```

### Database Integration Test Example
```rust
use sqlx::PgPool;

#[sqlx::test]
async fn test_database_operation(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    // Your test code here
    let result = repo.some_operation().await.unwrap();

    assert_eq!(result, expected);
}
```

### Async Test Example
```rust
#[tokio::test]
async fn test_async_operation() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

## Common Test Patterns

### Testing Error Cases
```rust
#[test]
fn test_invalid_input() {
    let result = function_with_validation("invalid");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("expected message"));
}
```

### Testing with Mock Data
```rust
#[tokio::test]
async fn test_with_mock_server() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/data"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // Test code using mock_server.uri()
}
```

## Debugging Tests

### Run Single Test with Logs
```bash
RUST_LOG=debug cargo test test_name -- --nocapture
```

### Show SQL Queries
```bash
RUST_LOG=sqlx=debug cargo test test_name -- --nocapture
```

### Keep Test Database
For debugging, you can manually create a test database and run tests against it:
```bash
createdb test_db
export DATABASE_URL=postgres://user:pass@localhost/test_db
sqlx migrate run
cargo test
```

## Continuous Integration

The CI pipeline runs on every push and pull request:

1. **Test Job**:
   - Spins up PostgreSQL container
   - Runs migrations
   - Executes format check
   - Runs Clippy linter
   - Runs all tests (unit + integration)

2. **Security Job**:
   - Runs `cargo audit` for dependency vulnerabilities

3. **Check Job**:
   - Verifies compilation for x86_64 and ARM64

4. **Docker Build**:
   - Only runs if all tests pass
   - Builds multi-platform images
   - Publishes to GitHub Container Registry

See [.github/workflows/ci.yml](.github/workflows/ci.yml) for details.

## Test Data

### Sample USCRN Data Format
```
WBANNO UTC_DATE UTC_TIME LST_DATE LST_TIME CRX_VN LONGITUDE LATITUDE T_CALC T_HR_AVG...
53104 20240115 1400 20240115 0600 3 -81.74 36.53 -9999.0 4.1 4.9 3.4 0.0 45.5 0 ...
```

### Missing Values
- Numeric: `-9999.0` or `-9999`
- Stored as: `NULL` in database

## Performance Benchmarks

Integration tests include performance validation:
- Large batch insert: 2000 records in < 5s
- Station upsert: < 100ms per station
- Observation deduplication: < 200ms

## Troubleshooting

### "database does not exist"
Ensure PostgreSQL is running and migrations have been applied.

### "failed to connect to database"
Check that `DATABASE_URL` is set correctly and PostgreSQL is accessible.

### "sqlx::test not found"
Make sure you have the sqlx dev-dependency with the macros feature enabled.

### Slow test runs
SQLx tests create fresh databases for each test. For faster iteration:
1. Run specific test files: `cargo test --test database_integration_test`
2. Run unit tests only: `cargo test --lib`
3. Use `--release` mode for faster execution: `cargo test --release`
