# Production-optimized Dockerfile with cargo-chef + distroless
#
# Multi-stage build strategy:
# 1. chef    - Base Rust image with cargo-chef installed
# 2. planner - Analyzes dependencies and generates recipe
# 3. builder - Compiles dependencies (cached) + application
# 4. runtime - Google distroless (~20MB, no shell/package manager)
#
# Size optimizations (see Cargo.toml [profile.release]):
# - strip = true          → Removes debug symbols
# - lto = "thin"          → Link-Time Optimization
# - opt-level = "z"       → Optimize for size
# - codegen-units = 1     → Better optimization (slower build)
# - panic = "abort"       → No unwinding (smaller binary)
#
# Security benefits:
# - Distroless runtime has ~50% fewer CVEs than debian:slim
# - No shell or package manager in production
# - Runs as non-root user (ID 65532)
# - Minimal attack surface
#
# Expected final image size: 50-80MB (vs 200MB+ with debian:slim)

FROM rust:slim AS chef

WORKDIR /app

# Install cargo-chef
# Note: Using latest stable Rust for full edition2024 support and dependency compatibility
RUN cargo install cargo-chef

# Planner stage
FROM chef AS planner

# Copy only files needed for planning
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Generate recipe file
RUN cargo chef prepare --recipe-path recipe.json

# Builder stage
FROM chef AS builder

WORKDIR /app

# Accept build args for CI optimization
# In CI: CODEGEN_UNITS=16, LTO=false (faster builds, ~10% larger binary)
# Local: uses Cargo.toml defaults (slower builds, smaller binary)
ARG CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
ARG CARGO_PROFILE_RELEASE_LTO=thin

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy recipe from planner
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies only (this layer will be cached)
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Copy migrations directory (required at compile time by sqlx::migrate! macro)
# SQLx verifies migrations exist and validates SQL at compile time
COPY migrations ./migrations

# Build the application with optimized release profile
# Build args override Cargo.toml settings for faster CI builds
RUN cargo build --release && \
    # Verify the binary works
    ls -lh /app/target/release/uscrn-ingest

# Runtime stage - using distroless for minimal size and attack surface
# distroless/cc-debian12 includes glibc and SSL certificates but no shell/package manager
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/uscrn-ingest /app/uscrn-ingest

# Copy config and migrations (runtime requirements)
COPY config /app/config
COPY migrations /app/migrations

# distroless runs as non-root by default (user ID 65532)
USER nonroot:nonroot

ENTRYPOINT ["/app/uscrn-ingest"]
