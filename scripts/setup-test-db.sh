#!/bin/bash
# Setup test database for integration tests

set -e

echo "🔧 Setting up test database for integration tests..."

# Default values (can be overridden with environment variables)
export POSTGRES_USER=${POSTGRES_USER:-test_user}
export POSTGRES_PASSWORD=${POSTGRES_PASSWORD:-test_password}
export POSTGRES_DB=${POSTGRES_DB:-uscrn_test}
export POSTGRES_PORT=${POSTGRES_PORT:-5432}

export DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${POSTGRES_PORT}/${POSTGRES_DB}"

# Check if PostgreSQL is already running
if pg_isready -h localhost -p ${POSTGRES_PORT} >/dev/null 2>&1; then
    echo "✅ PostgreSQL is already running on port ${POSTGRES_PORT}"
else
    echo "🚀 Starting PostgreSQL with Docker..."
    docker run -d \
        --name postgres-test \
        -e POSTGRES_USER=${POSTGRES_USER} \
        -e POSTGRES_PASSWORD=${POSTGRES_PASSWORD} \
        -e POSTGRES_DB=${POSTGRES_DB} \
        -p ${POSTGRES_PORT}:5432 \
        postgres:18-alpine

    echo "⏳ Waiting for PostgreSQL to be ready..."
    for i in {1..30}; do
        if pg_isready -h localhost -p ${POSTGRES_PORT} >/dev/null 2>&1; then
            echo "✅ PostgreSQL is ready!"
            break
        fi
        sleep 1
    done
fi

# Check if sqlx-cli is installed
if ! command -v sqlx &> /dev/null; then
    echo "📦 Installing sqlx-cli..."
    cargo install sqlx-cli --no-default-features --features postgres
fi

# Run migrations
echo "🔄 Running database migrations..."
sqlx migrate run

echo ""
echo "✅ Test database setup complete!"
echo ""
echo "To run tests, use:"
echo "  export DATABASE_URL=\"${DATABASE_URL}\""
echo "  cargo test"
echo ""
echo "Or run this script with source to set DATABASE_URL in your shell:"
echo "  source scripts/setup-test-db.sh"
echo ""
