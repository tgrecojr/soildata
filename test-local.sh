#!/bin/bash
# Run tests that don't require database setup
# This is perfect for local development without PostgreSQL

set -e

echo "🧪 Running tests that don't require database..."
echo ""

echo "📦 Unit tests (in source files)..."
cargo test --lib

echo ""
echo "🔍 Fetcher integration tests (URL validation, filtering)..."
cargo test --test fetcher_integration_test

echo ""
echo "📝 Parser tests (non-database)..."
cargo test --test parser_integration_test -- \
    --skip parse_and_insert \
    --skip missing_values \
    --skip reimport

echo ""
echo "✅ All local tests passed!"
echo ""
echo "Tests run: ~27 tests (no database required)"
echo ""
echo "To run database integration tests (requires PostgreSQL):"
echo "  1. Start PostgreSQL: docker-compose up -d db"
echo "  2. Set DATABASE_URL: export DATABASE_URL=postgres://uscrn:uscrn_password@localhost:5432/uscrn"
echo "  3. Run migrations: sqlx migrate run"
echo "  4. Run all tests: cargo test"
echo ""
