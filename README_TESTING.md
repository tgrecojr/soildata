# Quick Testing Guide

## TL;DR - Run Tests Without Database Setup

```bash
# Make script executable (one time)
chmod +x test-local.sh

# Run tests (no database needed)
./test-local.sh
```

This runs **27 tests** without requiring PostgreSQL.

---

## What Gets Tested Without Database?

### ✅ Unit Tests (15 tests)
- Configuration parsing and validation
- Environment variable expansion
- Location filtering logic
- Date/time parsing
- Missing value handling
- Filename parsing

### ✅ Fetcher Integration Tests (8 tests)
- URL validation (SSRF protection)
- HTTPS enforcement
- Location filter matching (states, patterns, stations)

### ✅ Parser Tests (4 tests)
- Empty file handling
- Failure threshold validation
- Custom threshold configuration

**Total: 27 tests** - Perfect for local development!

---

## What Requires Database?

### 🗄️ Database Integration Tests (11 tests)
- Station insert/update operations
- Observation insert/update with deduplication
- Batch processing (2000+ records)
- File tracking queries

### 🗄️ Parser + Database Tests (3 tests)
- End-to-end parse → insert flow
- Missing value storage as NULL
- Re-import deduplication

**These run automatically in GitHub Actions CI/CD** with a PostgreSQL service container.

---

## Running Full Test Suite Locally (Optional)

If you want to run **all** tests including database integration tests:

### Option 1: Use Docker Compose
```bash
docker-compose up -d db
export DATABASE_URL=postgres://uscrn:uscrn_password@localhost:5432/uscrn
sqlx migrate run
cargo test
```

### Option 2: Quick Test Database
```bash
docker run -d --name postgres-test \
  -e POSTGRES_USER=test \
  -e POSTGRES_PASSWORD=test \
  -e POSTGRES_DB=test_db \
  -p 5432:5432 \
  postgres:18-alpine

export DATABASE_URL=postgres://test:test@localhost:5432/test_db
cargo install sqlx-cli --no-default-features --features postgres
sqlx migrate run
cargo test
```

---

## CI/CD Testing

GitHub Actions automatically:
1. ✅ Starts PostgreSQL container
2. ✅ Runs migrations
3. ✅ Executes all 42 tests (unit + integration)
4. ✅ Only builds Docker if tests pass

You don't need to worry about database setup - CI handles it!

---

## Mock vs Real Database

**Q: Why not use mocks for database tests?**

**A:** SQLx integration tests use real databases because:
- Catches SQL syntax errors that mocks miss
- Tests actual PostgreSQL behavior (ON CONFLICT, transactions, etc.)
- Validates migrations work correctly
- Ensures queries perform well

However, you can **develop and test core logic** with the 27 non-database tests, then let CI validate database integration.

---

## Test-Driven Development Workflow

```bash
# 1. Write code
vim src/parser.rs

# 2. Run fast local tests
./test-local.sh

# 3. Commit and push
git add .
git commit -m "Add feature"
git push

# 4. CI runs full test suite including database tests
# (Check GitHub Actions)
```

This gives you fast feedback locally (< 1 second) while still ensuring database tests pass in CI.

---

## Summary

| Test Suite | Count | Database Required? | Run Command |
|------------|-------|-------------------|-------------|
| Unit tests | 15 | ❌ No | `cargo test --lib` |
| Fetcher integration | 8 | ❌ No | `cargo test --test fetcher_integration_test` |
| Parser (non-DB) | 4 | ❌ No | `./test-local.sh` |
| **Subtotal (Local)** | **27** | ❌ **No** | `./test-local.sh` |
| Database integration | 11 | ✅ Yes | `cargo test --test database_integration_test` |
| Parser + DB | 3 | ✅ Yes | `cargo test --test parser_integration_test` |
| **Total (CI)** | **42** | ✅ **Yes** | `cargo test` |

**For local development:** Use `./test-local.sh` (no setup required)
**For full validation:** Let GitHub Actions handle it automatically
